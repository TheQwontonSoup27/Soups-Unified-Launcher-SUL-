//! Live-code attestation (D-159, decision 0015): code SUL creates **after** the install-time baseline is signed as new live
//! code, so the watchdog verifies it instead of flagging it or, worse, skipping it.
//!
//! # The problem
//!
//! The baseline (D-157) lists the files that were there at install time. Code that appears later is either reported (a new
//! mod DLL is an unlisted file, a trampoline is an RWX region) or the caller says "that is mine" and the scan stops looking
//! at it (`Expectations::own_rwx` before D-159). The second is a hole: whoever overwrites a trampoline SUL declared is
//! invisible. Attestation closes it: the watchdog keeps looking, and what it looks for is a **digest it did not choose**.
//!
//! # What is signed, and who may sign what
//!
//! A [`LiveCodeRecord`] binds one piece of code (a file's SHA-256, or an in-memory region's BLAKE3-256) to a
//! [`Purpose`], the module that produced it, a serial and a root. Four purposes, three authorities, and **a key never signs
//! outside its own purposes**:
//!
//! | Purpose          | Scope      | Signed by          | May do                                                              |
//! |------------------|------------|--------------------|---------------------------------------------------------------------|
//! | `HotUpdate`      | persistent | release key        | add or replace any file, including ones the baseline lists           |
//! | `LocalBuild`     | persistent | device key         | add a **new** file (sidecar, mod, model adapter); never replace one  |
//! | `JitCode`        | session    | session key        | one in-memory region                                                 |
//! | `HookTrampoline` | session    | session key        | one in-memory region (the MinHook opt-in, Ch. 7 §2.1)               |
//!
//! So a stolen session key cannot forge an update, and a stolen device key cannot rewrite SUL's own binaries. The two
//! kinds of ledger (persistent and session) are separate objects with separate roots: the persistent one is rooted in the
//! SHA-256 of the signed baseline (a ledger made for one baseline is refused by another), the session one in a random nonce
//! chosen at start-up (a record captured in a previous run is refused in this one).
//!
//! # What the watchdog checks, in this order
//!
//! Before anything is read from the record, the Ed25519 signature is checked (strictly, through
//! [`sul_trust::verify_ed25519`]) against every configured key. Then: the bytes are the **canonical** serialisation of the
//! record (so there is exactly one byte string per record: no whitespace, field order or extra field can differ); the schema;
//! the signing key is the one the purpose requires; the scope; the root; the serial (persistent: strictly greater than the
//! last, so a rollback to an older signed state is refused; session: never seen before, so concurrent signers may register
//! in any order but nothing is replayed); the clock (not from the future beyond [`CLOCK_SKEW_SECS`], not expired); the
//! producer (a module the baseline lists as code); and the content rules. For a region the watchdog then **reads the bytes
//! itself** through a [`RegionDigester`] and compares: a record whose digest is not what is in memory is refused, however
//! well it is signed. Nothing is changed unless every check passed.
//!
//! # Performance
//!
//! The signature is verified once, at registration. After that a region costs one BLAKE3 pass per scan (BLAKE3 is the
//! fastest widely reviewed hash here and a region is re-read every scan; files keep SHA-256 because the baseline and the
//! spec's manifests do). This crate computes no BLAKE3 itself: the [`RegionDigester`] supplied by the Windows adapter hashes
//! the memory in place with no copy, and this module only compares 32 bytes.
//!
//! # What this does not do (said plainly)
//!
//! * It cannot stand against an attacker who already runs arbitrary code in the process **and** can ask the signer to sign:
//!   if the signer lives in the same process, so does the key. The signing step is therefore a trait ([`LiveCodeSigner`])
//!   that the vault supplies at layer 4, in another process; nothing in this crate holds a private key.
//! * It detects a change between scans, not a change that is made and undone between two scans.
//! * Nothing here reads a live process; the digester and the probe are the next cut.
//! * Where the device and session public keys are anchored (the installer, the vault) is not decided here.

use crate::baseline::{Baseline, Role, Watched, key};
use crate::scan::same_name;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use sul_core::{Error, Result, safepath};
use sul_trust::verify_ed25519;

pub const LIVE_SCHEMA: u32 = 1;
/// The largest in-memory region one record may cover. Bigger is refused: a region this size is not code SUL generated.
pub const MAX_REGION_BYTES: u64 = 256 * 1024 * 1024;
/// How far ahead of the watchdog's clock a record's creation time may be (clocks differ between processes).
pub const CLOCK_SKEW_SECS: u64 = 120;
/// A record is a few hundred bytes; anything this large is refused before it is parsed.
pub const MAX_RECORD_BYTES: usize = 4096;

type Admit<T> = std::result::Result<T, Refusal>;

/// Which ledger a record belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Files. Survives a restart (the caller stores the signed records and re-admits them, each re-verified).
    Persistent,
    /// Memory. Lives and dies with the process.
    Session,
}

/// A key that may sign. Three, and no key may hold two roles ([`TrustRoots::distinct`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Authority {
    /// The offline release key that signs the baseline and updates.
    Release,
    /// The device key the vault holds, for what this machine builds.
    Device,
    /// A key made for one run of the process, for what the process generates.
    Session,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    HotUpdate,
    LocalBuild,
    JitCode,
    HookTrampoline,
}

impl Purpose {
    pub const fn scope(self) -> Scope {
        match self {
            Self::HotUpdate | Self::LocalBuild => Scope::Persistent,
            Self::JitCode | Self::HookTrampoline => Scope::Session,
        }
    }

    /// The only key that may sign this purpose.
    pub const fn authority(self) -> Authority {
        match self {
            Self::HotUpdate => Authority::Release,
            Self::LocalBuild => Authority::Device,
            Self::JitCode | Self::HookTrampoline => Authority::Session,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::HotUpdate => "hot update",
            Self::LocalBuild => "local build",
            Self::JitCode => "generated code",
            Self::HookTrampoline => "hook trampoline",
        }
    }
}

/// What a record vouches for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Content {
    /// A file under the guarded root: its path (forward slashes, relative), SHA-256, size and role.
    File {
        path: String,
        sha256: String,
        size: u64,
        role: Role,
    },
    /// A range of this process's memory. The address is **not** signed: the operating system chooses it at run time and the
    /// registrant names it, but the watchdog reads the bytes there itself and compares the digest.
    Region { size: u64, blake3: String },
}

impl Content {
    pub const fn scope(&self) -> Scope {
        match self {
            Self::File { .. } => Scope::Persistent,
            Self::Region { .. } => Scope::Session,
        }
    }
}

/// The signed statement. Compact JSON, fields in declaration order: [`signing_bytes`](Self::signing_bytes) is the one byte
/// string that is signed and the only one a ledger accepts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveCodeRecord {
    pub schema_version: u32,
    /// Lower-case hex SHA-256 the ledger is rooted in (the signed baseline's bytes, or the session nonce).
    pub root: String,
    /// Starts at 1.
    pub serial: u64,
    pub purpose: Purpose,
    /// The file name (no folder) of the module that produced the code. Must be one the baseline lists as code.
    pub producer: String,
    pub created_unix: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_unix: Option<u64>,
    pub content: Content,
}

impl LiveCodeRecord {
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }
}

/// Signs the bytes of a record. **Implemented by the vault, in another process, at layer 4**; this crate only checks. A
/// signer must refuse to sign a purpose its key is not for and should refuse a producer it does not know.
pub trait LiveCodeSigner {
    fn sign(&mut self, raw: &[u8]) -> Result<[u8; 64]>;
}

/// Reads and hashes memory in the watchdog's own process. The Windows adapter implements it (decision 0015); it hashes in
/// place with no copy. The contract: the BLAKE3-256 of exactly `len` bytes starting at `base`, or an error if any of it is
/// unreadable.
pub trait RegionDigester {
    fn blake3(&mut self, base: u64, len: u64) -> Result<[u8; 32]>;
}

/// A record as it is stored and passed around: the exact signed text and the signature (128 lower-case hex digits).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedRecord {
    pub raw: String,
    pub signature: String,
}

impl SignedRecord {
    pub fn new(record: &LiveCodeRecord, signer: &mut dyn LiveCodeSigner) -> Result<Self> {
        let raw = record.signing_bytes()?;
        let signature = signer.sign(&raw)?;
        let raw = String::from_utf8(raw)
            .map_err(|_| Error::Invalid("a live-code record is not UTF-8".into()))?;
        Ok(Self {
            raw,
            signature: hex::encode(signature),
        })
    }
}

/// The public keys the watchdog trusts, one per [`Authority`]. Public only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrustRoots {
    pub release: Option<[u8; 32]>,
    pub device: Option<[u8; 32]>,
    pub session: Option<[u8; 32]>,
}

impl TrustRoots {
    fn key(&self, authority: Authority) -> Option<&[u8; 32]> {
        match authority {
            Authority::Release => self.release.as_ref(),
            Authority::Device => self.device.as_ref(),
            Authority::Session => self.session.as_ref(),
        }
    }

    /// No two authorities share a key. One key in two roles would let the weaker role's compromise act as the stronger.
    pub fn distinct(&self) -> bool {
        let keys: Vec<&[u8; 32]> = [&self.release, &self.device, &self.session]
            .into_iter()
            .flatten()
            .collect();
        keys.iter()
            .enumerate()
            .all(|(i, k)| keys[i + 1..].iter().all(|other| other != k))
    }
}

/// The modules allowed to be named as the producer of live code: the baseline's own code (programs, helpers, the trust
/// layer). A module that was itself attested later is not added: trust is anchored in the release-signed baseline.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrustedProducers {
    names: Vec<String>,
}

impl TrustedProducers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn trust(&mut self, module: &str) {
        if valid_producer_name(module) && !self.contains(module) {
            self.names.push(module.to_owned());
        }
    }

    pub fn contains(&self, module: &str) -> bool {
        self.names.iter().any(|n| same_name(n, module))
    }

    /// Every `Binary`, `Sidecar` and `TrustPrimitive` entry of `baseline`, by file name.
    pub fn from_baseline(baseline: &Baseline) -> Self {
        let mut out = Self::new();
        for entry in &baseline.entries {
            if matches!(
                entry.role,
                Role::Binary | Role::Sidecar | Role::TrustPrimitive
            ) {
                let name = entry.path.rsplit('/').next().unwrap_or(&entry.path);
                out.trust(name);
            }
        }
        out
    }
}

fn valid_producer_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.contains(['/', '\\'])
        && !name.chars().any(char::is_control)
}

/// Why a record was refused. A closed list of words: it goes in the threat log, so it carries no path and no sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    TooLarge,
    BadSignature,
    NotCanonical,
    WrongSchema,
    WrongAuthority,
    WrongScope,
    WrongRoot,
    OutOfOrder,
    Expired,
    FromTheFuture,
    UntrustedProducer,
    ForbiddenPath,
    ForbiddenRole,
    CannotReplace,
    BadDigest,
    BadSize,
    Overlap,
    ContentMismatch,
    Unreadable,
}

impl Refusal {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TooLarge => "too_large",
            Self::BadSignature => "bad_signature",
            Self::NotCanonical => "not_canonical",
            Self::WrongSchema => "wrong_schema",
            Self::WrongAuthority => "wrong_authority",
            Self::WrongScope => "wrong_scope",
            Self::WrongRoot => "wrong_root",
            Self::OutOfOrder => "out_of_order",
            Self::Expired => "expired",
            Self::FromTheFuture => "from_the_future",
            Self::UntrustedProducer => "untrusted_producer",
            Self::ForbiddenPath => "forbidden_path",
            Self::ForbiddenRole => "forbidden_role",
            Self::CannotReplace => "cannot_replace",
            Self::BadDigest => "bad_digest",
            Self::BadSize => "bad_size",
            Self::Overlap => "overlap",
            Self::ContentMismatch => "content_mismatch",
            Self::Unreadable => "unreadable",
        }
    }

    /// One plain sentence for a person.
    pub const fn describe(self) -> &'static str {
        match self {
            Self::TooLarge => "the record is larger than any record should be",
            Self::BadSignature => "the signature does not check against any trusted key",
            Self::NotCanonical => "the signed text is not the one canonical form of a record",
            Self::WrongSchema => "the record is from a schema this version does not read",
            Self::WrongAuthority => "the record was signed by a key that may not sign this purpose",
            Self::WrongScope => "the record belongs to the other kind of ledger",
            Self::WrongRoot => "the record was signed for a different baseline or a different run",
            Self::OutOfOrder => {
                "the record's serial was already used or is older than one that was accepted"
            }
            Self::Expired => "the record has expired",
            Self::FromTheFuture => "the record says it was made later than now",
            Self::UntrustedProducer => {
                "the record names a producer the baseline does not list as code"
            }
            Self::ForbiddenPath => "the record's path is not a safe relative path",
            Self::ForbiddenRole => "a local build may not claim this role",
            Self::CannotReplace => "this purpose may not replace a file that is already guarded",
            Self::BadDigest => "a digest in the record is not 64 lower-case hex digits",
            Self::BadSize => "the record's size or address is not valid",
            Self::Overlap => "the memory range overlaps live code that is already registered",
            Self::ContentMismatch => {
                "the bytes in memory are not the bytes the record was signed for"
            }
            Self::Unreadable => "the memory range could not be read",
        }
    }

    /// Whether the fact (not the intent) is certain. A signature that does not check, a key used outside its purpose, or
    /// memory that is not what was signed for is certain; a stale or reordered record is not (a restart does that).
    pub const fn is_certain(self) -> bool {
        matches!(
            self,
            Self::BadSignature | Self::WrongAuthority | Self::ContentMismatch
        )
    }
}

/// One file vouched for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttestedFile {
    pub serial: u64,
    pub path: String,
    pub sha256: String,
    pub size: u64,
    pub role: Role,
    pub purpose: Purpose,
    pub producer: String,
    pub expires_unix: Option<u64>,
}

/// One range of memory vouched for, bound to the address it was registered at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttestedRegion {
    pub serial: u64,
    pub base: u64,
    pub size: u64,
    pub blake3: [u8; 32],
    pub producer: String,
    pub purpose: Purpose,
    pub expires_unix: Option<u64>,
}

impl AttestedRegion {
    pub fn end(&self) -> u64 {
        self.base.saturating_add(self.size)
    }

    pub fn contains(&self, address: u64) -> bool {
        address >= self.base && address - self.base < self.size
    }

    /// For the person: which record this is.
    pub fn label(&self) -> String {
        format!(
            "serial {}, {}, from {}",
            self.serial,
            self.purpose.label(),
            self.producer
        )
    }
}

fn hex32(text: &str) -> Option<[u8; 32]> {
    let mut out = [0u8; 32];
    lower_hex(text, 64)?;
    hex::decode_to_slice(text, &mut out).ok()?;
    Some(out)
}

fn hex64(text: &str) -> Option<[u8; 64]> {
    let mut out = [0u8; 64];
    lower_hex(text, 128)?;
    hex::decode_to_slice(text, &mut out).ok()?;
    Some(out)
}

fn lower_hex(text: &str, len: usize) -> Option<()> {
    (text.len() == len
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    .then_some(())
}

/// The ledger of live code the watchdog accepts. One per [`Scope`]; see the module documentation.
#[derive(Debug, Clone)]
pub struct LiveCodeLedger {
    scope: Scope,
    root: [u8; 32],
    roots: TrustRoots,
    producers: TrustedProducers,
    /// Persistent: the highest serial accepted.
    high_water: u64,
    /// Session: every serial accepted, retired or not.
    seen: BTreeSet<u64>,
    files: Vec<AttestedFile>,
    regions: Vec<AttestedRegion>,
}

impl LiveCodeLedger {
    /// The persistent ledger for the baseline whose signed bytes are `baseline_raw`.
    pub fn persistent(
        baseline_raw: &[u8],
        roots: TrustRoots,
        producers: TrustedProducers,
    ) -> Result<Self> {
        Self::new(
            Scope::Persistent,
            Sha256::digest(baseline_raw).into(),
            roots,
            producers,
        )
    }

    /// The session ledger for one run. `nonce` must come from a cryptographic random source and be given to the signer.
    pub fn session(
        nonce: [u8; 32],
        roots: TrustRoots,
        producers: TrustedProducers,
    ) -> Result<Self> {
        Self::new(Scope::Session, nonce, roots, producers)
    }

    fn new(
        scope: Scope,
        root: [u8; 32],
        roots: TrustRoots,
        producers: TrustedProducers,
    ) -> Result<Self> {
        if !roots.distinct() {
            return Err(Error::Invalid(
                "two live-code authorities share one public key".into(),
            ));
        }
        Ok(Self {
            scope,
            root,
            roots,
            producers,
            high_water: 0,
            seen: BTreeSet::new(),
            files: Vec::new(),
            regions: Vec::new(),
        })
    }

    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// The root a record must carry, as lower-case hex.
    pub fn root_hex(&self) -> String {
        hex::encode(self.root)
    }

    pub fn files(&self) -> &[AttestedFile] {
        &self.files
    }

    pub fn regions(&self) -> &[AttestedRegion] {
        &self.regions
    }

    /// The memory ranges the probe should digest for the next scan: `(base, size)`.
    pub fn ranges(&self) -> Vec<(u64, u64)> {
        self.regions.iter().map(|r| (r.base, r.size)).collect()
    }

    /// Every check that does not look at the content. Reads nothing from the record until a trusted key has signed it.
    fn check(&self, signed: &SignedRecord, now: u64) -> Admit<LiveCodeRecord> {
        if signed.raw.len() > MAX_RECORD_BYTES {
            return Err(Refusal::TooLarge);
        }
        let signature = hex64(&signed.signature).ok_or(Refusal::BadSignature)?;
        let raw = signed.raw.as_bytes();
        let signers: Vec<Authority> = [Authority::Release, Authority::Device, Authority::Session]
            .into_iter()
            .filter(|a| {
                self.roots
                    .key(*a)
                    .is_some_and(|public| verify_ed25519(raw, &signature, public).is_ok())
            })
            .collect();
        if signers.is_empty() {
            return Err(Refusal::BadSignature);
        }
        let record: LiveCodeRecord =
            serde_json::from_slice(raw).map_err(|_| Refusal::NotCanonical)?;
        match serde_json::to_vec(&record) {
            Ok(again) if again == raw => {}
            _ => return Err(Refusal::NotCanonical),
        }
        if record.schema_version != LIVE_SCHEMA {
            return Err(Refusal::WrongSchema);
        }
        if !signers.contains(&record.purpose.authority()) {
            return Err(Refusal::WrongAuthority);
        }
        if record.purpose.scope() != self.scope || record.content.scope() != self.scope {
            return Err(Refusal::WrongScope);
        }
        if record.root != self.root_hex() {
            return Err(Refusal::WrongRoot);
        }
        let fresh = match self.scope {
            Scope::Persistent => record.serial > self.high_water,
            Scope::Session => record.serial != 0 && !self.seen.contains(&record.serial),
        };
        if !fresh {
            return Err(Refusal::OutOfOrder);
        }
        if record.created_unix > now.saturating_add(CLOCK_SKEW_SECS) {
            return Err(Refusal::FromTheFuture);
        }
        if record
            .expires_unix
            .is_some_and(|e| e <= now || e <= record.created_unix)
        {
            return Err(Refusal::Expired);
        }
        if !valid_producer_name(&record.producer) || !self.producers.contains(&record.producer) {
            return Err(Refusal::UntrustedProducer);
        }
        Ok(record)
    }

    /// Admit a file record. `baseline` is the signed baseline this ledger is rooted in (not the effective one).
    /// Returns the serial. Nothing changes unless every check passes.
    pub fn admit_file(
        &mut self,
        signed: &SignedRecord,
        baseline: &Baseline,
        now: u64,
    ) -> Admit<u64> {
        let record = self.check(signed, now)?;
        let Content::File {
            path,
            sha256,
            size,
            role,
        } = &record.content
        else {
            return Err(Refusal::WrongScope);
        };
        if safepath::validate_relative(path).is_err() || path.contains('\\') {
            return Err(Refusal::ForbiddenPath);
        }
        if hex32(sha256).is_none() {
            return Err(Refusal::BadDigest);
        }
        if record.purpose == Purpose::LocalBuild
            && matches!(role, Role::Binary | Role::Manifest | Role::TrustPrimitive)
        {
            return Err(Refusal::ForbiddenRole);
        }
        // A file the release-signed baseline lists, or a release-signed update added, is changed only by a release-signed
        // update. A device key can add files and change its own; it cannot rewrite SUL.
        let k = key(path);
        let in_baseline = baseline.entries.iter().any(|e| key(&e.path) == k);
        let existing = self.files.iter().position(|f| key(&f.path) == k);
        let protected =
            in_baseline || existing.is_some_and(|i| self.files[i].purpose == Purpose::HotUpdate);
        if protected && record.purpose != Purpose::HotUpdate {
            return Err(Refusal::CannotReplace);
        }
        let attested = AttestedFile {
            serial: record.serial,
            path: path.clone(),
            sha256: sha256.clone(),
            size: *size,
            role: *role,
            purpose: record.purpose,
            producer: record.producer.clone(),
            expires_unix: record.expires_unix,
        };
        match existing {
            Some(i) => self.files[i] = attested,
            None => self.files.push(attested),
        }
        self.high_water = record.serial;
        Ok(record.serial)
    }

    /// Admit a region record for the memory at `base`. The watchdog reads and hashes that memory itself through
    /// `digester`; a record that does not match what is there is refused. Returns the serial. Nothing changes (and the
    /// serial is not used up) unless every check passes.
    pub fn admit_region(
        &mut self,
        signed: &SignedRecord,
        base: u64,
        now: u64,
        digester: &mut dyn RegionDigester,
    ) -> Admit<u64> {
        let record = self.check(signed, now)?;
        let Content::Region { size, blake3 } = &record.content else {
            return Err(Refusal::WrongScope);
        };
        let signed_digest = hex32(blake3).ok_or(Refusal::BadDigest)?;
        let end = base.checked_add(*size).ok_or(Refusal::BadSize)?;
        if base == 0 || *size == 0 || *size > MAX_REGION_BYTES {
            return Err(Refusal::BadSize);
        }
        if self.regions.iter().any(|r| base < r.end() && r.base < end) {
            return Err(Refusal::Overlap);
        }
        // The expensive step is last, and it is the one that does not trust the record: the bytes are read here.
        let actual = digester
            .blake3(base, *size)
            .map_err(|_| Refusal::Unreadable)?;
        if actual != signed_digest {
            return Err(Refusal::ContentMismatch);
        }
        self.seen.insert(record.serial);
        self.regions.push(AttestedRegion {
            serial: record.serial,
            base,
            size: *size,
            blake3: signed_digest,
            producer: record.producer,
            purpose: record.purpose,
            expires_unix: record.expires_unix,
        });
        Ok(record.serial)
    }

    /// The owner says the code is gone (freed, unloaded, deleted). Removes it from what is checked; its serial stays used, so
    /// the record cannot be presented again. Returns whether something was removed.
    pub fn retire(&mut self, serial: u64) -> bool {
        let before = self.files.len() + self.regions.len();
        self.files.retain(|f| f.serial != serial);
        self.regions.retain(|r| r.serial != serial);
        self.files.len() + self.regions.len() != before
    }

    /// Drop what has expired. Returns how many records went.
    pub fn prune_expired(&mut self, now: u64) -> usize {
        let before = self.files.len() + self.regions.len();
        self.files
            .retain(|f| f.expires_unix.is_none_or(|e| e > now));
        self.regions
            .retain(|r| r.expires_unix.is_none_or(|e| e > now));
        before - (self.files.len() + self.regions.len())
    }

    /// The signed baseline plus the files attested here that have not expired, as one valid [`Baseline`]: what the guard and
    /// the monitor should compare the disk with. A replaced entry keeps the artifact code it had.
    pub fn effective_baseline(&self, base: &Baseline, now: u64) -> Result<Baseline> {
        let mut entries: BTreeMap<String, Watched> = base
            .entries
            .iter()
            .map(|e| (key(&e.path), e.clone()))
            .collect();
        for f in &self.files {
            if f.expires_unix.is_some_and(|e| e <= now) {
                continue;
            }
            let k = key(&f.path);
            let artifact_code = entries.get(&k).and_then(|e| e.artifact_code.clone());
            entries.insert(
                k,
                Watched {
                    path: f.path.clone(),
                    sha256: f.sha256.clone(),
                    size: f.size,
                    role: f.role,
                    artifact_code,
                },
            );
        }
        let mut out = base.clone();
        out.entries = entries.into_values().collect();
        out.validate()?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::baseline::BASELINE_SCHEMA;
    use ed25519_dalek::{Signer, SigningKey};

    const NOW: u64 = 1_000_000;

    struct TestSigner(SigningKey);

    impl LiveCodeSigner for TestSigner {
        fn sign(&mut self, raw: &[u8]) -> Result<[u8; 64]> {
            Ok(self.0.sign(raw).to_bytes())
        }
    }

    fn release() -> SigningKey {
        SigningKey::from_bytes(&[1u8; 32])
    }
    fn device() -> SigningKey {
        SigningKey::from_bytes(&[2u8; 32])
    }
    fn session() -> SigningKey {
        SigningKey::from_bytes(&[3u8; 32])
    }

    fn roots() -> TrustRoots {
        TrustRoots {
            release: Some(release().verifying_key().to_bytes()),
            device: Some(device().verifying_key().to_bytes()),
            session: Some(session().verifying_key().to_bytes()),
        }
    }

    fn watched(path: &str, role: Role, code: Option<&str>) -> Watched {
        Watched {
            path: path.into(),
            sha256: "a".repeat(64),
            size: 10,
            role,
            artifact_code: code.map(str::to_owned),
        }
    }

    fn baseline() -> Baseline {
        Baseline {
            schema_version: BASELINE_SCHEMA,
            label: "t".into(),
            created_unix: 1,
            entries: vec![
                watched("sul-updater.exe", Role::Binary, None),
                watched(
                    "sul-vault.dll",
                    Role::TrustPrimitive,
                    Some("E_VAULT_TAMPER_DETECTED"),
                ),
                watched("sul.exe", Role::Binary, None),
            ],
        }
    }

    const BASELINE_RAW: &[u8] = b"signed baseline bytes";

    fn persistent() -> LiveCodeLedger {
        LiveCodeLedger::persistent(
            BASELINE_RAW,
            roots(),
            TrustedProducers::from_baseline(&baseline()),
        )
        .unwrap()
    }

    const NONCE: [u8; 32] = [7u8; 32];

    fn session_ledger() -> LiveCodeLedger {
        LiveCodeLedger::session(NONCE, roots(), TrustedProducers::from_baseline(&baseline()))
            .unwrap()
    }

    fn file_record(
        ledger: &LiveCodeLedger,
        serial: u64,
        purpose: Purpose,
        path: &str,
        role: Role,
    ) -> LiveCodeRecord {
        LiveCodeRecord {
            schema_version: LIVE_SCHEMA,
            root: ledger.root_hex(),
            serial,
            purpose,
            producer: "sul-updater.exe".into(),
            created_unix: NOW,
            expires_unix: None,
            content: Content::File {
                path: path.into(),
                sha256: "b".repeat(64),
                size: 99,
                role,
            },
        }
    }

    fn region_record(
        ledger: &LiveCodeLedger,
        serial: u64,
        digest: [u8; 32],
        size: u64,
    ) -> LiveCodeRecord {
        LiveCodeRecord {
            schema_version: LIVE_SCHEMA,
            root: ledger.root_hex(),
            serial,
            purpose: Purpose::JitCode,
            producer: "sul.exe".into(),
            created_unix: NOW,
            expires_unix: None,
            content: Content::Region {
                size,
                blake3: hex::encode(digest),
            },
        }
    }

    fn sign(record: &LiveCodeRecord, by: SigningKey) -> SignedRecord {
        SignedRecord::new(record, &mut TestSigner(by)).unwrap()
    }

    /// A memory that answers for the ranges it was given and records what it was asked.
    #[derive(Default)]
    struct Memory {
        known: Vec<(u64, u64, [u8; 32])>,
        asked: Vec<(u64, u64)>,
    }

    impl RegionDigester for Memory {
        fn blake3(&mut self, base: u64, len: u64) -> Result<[u8; 32]> {
            self.asked.push((base, len));
            self.known
                .iter()
                .find(|(b, l, _)| *b == base && *l == len)
                .map(|(_, _, d)| *d)
                .ok_or_else(|| Error::Invalid("unmapped".into()))
        }
    }

    fn memory(base: u64, len: u64, digest: [u8; 32]) -> Memory {
        Memory {
            known: vec![(base, len, digest)],
            asked: Vec::new(),
        }
    }

    const DIGEST: [u8; 32] = [0x42; 32];

    // ---- regions ---------------------------------------------------------------------------------------------------

    #[test]
    fn a_signed_region_that_matches_memory_is_admitted_and_the_watchdog_reads_the_bytes_itself() {
        let mut ledger = session_ledger();
        let signed = sign(&region_record(&ledger, 1, DIGEST, 4096), session());
        let mut mem = memory(0x10000, 4096, DIGEST);
        assert_eq!(ledger.admit_region(&signed, 0x10000, NOW, &mut mem), Ok(1));
        assert_eq!(
            mem.asked,
            vec![(0x10000, 4096)],
            "it asks for exactly the registered range"
        );
        let r = &ledger.regions()[0];
        assert_eq!(
            (r.base, r.size, r.blake3, r.serial),
            (0x10000, 4096, DIGEST, 1)
        );
        assert_eq!(ledger.ranges(), vec![(0x10000, 4096)]);
    }

    #[test]
    fn a_well_signed_record_whose_digest_is_not_what_is_in_memory_is_refused_and_does_not_use_up_its_serial()
     {
        let mut ledger = session_ledger();
        let signed = sign(&region_record(&ledger, 1, DIGEST, 4096), session());
        let mut wrong = memory(0x10000, 4096, [0x99; 32]);
        assert_eq!(
            ledger.admit_region(&signed, 0x10000, NOW, &mut wrong),
            Err(Refusal::ContentMismatch)
        );
        assert!(ledger.regions().is_empty());
        // The same record is fine once the memory is what was signed.
        let mut right = memory(0x10000, 4096, DIGEST);
        assert_eq!(
            ledger.admit_region(&signed, 0x10000, NOW, &mut right),
            Ok(1)
        );
    }

    #[test]
    fn unreadable_memory_is_a_refusal_not_a_pass() {
        let mut ledger = session_ledger();
        let signed = sign(&region_record(&ledger, 1, DIGEST, 4096), session());
        let mut nothing = Memory::default();
        assert_eq!(
            ledger.admit_region(&signed, 0x10000, NOW, &mut nothing),
            Err(Refusal::Unreadable)
        );
        assert!(ledger.regions().is_empty());
    }

    #[test]
    fn region_sizes_and_addresses_are_bounded() {
        let mut ledger = session_ledger();
        let mut mem = Memory::default();
        let zero = sign(&region_record(&ledger, 1, DIGEST, 0), session());
        assert_eq!(
            ledger.admit_region(&zero, 0x10000, NOW, &mut mem),
            Err(Refusal::BadSize)
        );
        let huge = sign(
            &region_record(&ledger, 2, DIGEST, MAX_REGION_BYTES + 1),
            session(),
        );
        assert_eq!(
            ledger.admit_region(&huge, 0x10000, NOW, &mut mem),
            Err(Refusal::BadSize)
        );
        let ok_size = sign(&region_record(&ledger, 3, DIGEST, 4096), session());
        assert_eq!(
            ledger.admit_region(&ok_size, 0, NOW, &mut mem),
            Err(Refusal::BadSize),
            "address zero"
        );
        assert_eq!(
            ledger.admit_region(&ok_size, u64::MAX - 10, NOW, &mut mem),
            Err(Refusal::BadSize),
            "base plus size overflows"
        );
        assert!(
            mem.asked.is_empty(),
            "none of these reached the memory read"
        );
    }

    #[test]
    fn a_region_may_not_overlap_one_already_registered() {
        let mut ledger = session_ledger();
        let a = sign(&region_record(&ledger, 1, DIGEST, 4096), session());
        let mut mem = Memory {
            known: vec![
                (0x10000, 4096, DIGEST),
                (0x10800, 4096, DIGEST),
                (0x11000, 4096, DIGEST),
            ],
            asked: Vec::new(),
        };
        assert_eq!(ledger.admit_region(&a, 0x10000, NOW, &mut mem), Ok(1));
        let overlapping = sign(&region_record(&ledger, 2, DIGEST, 4096), session());
        assert_eq!(
            ledger.admit_region(&overlapping, 0x10800, NOW, &mut mem),
            Err(Refusal::Overlap)
        );
        // Touching is not overlapping.
        assert_eq!(
            ledger.admit_region(&overlapping, 0x11000, NOW, &mut mem),
            Ok(2)
        );
    }

    #[test]
    fn retiring_removes_coverage_and_the_record_cannot_come_back() {
        let mut ledger = session_ledger();
        let signed = sign(&region_record(&ledger, 1, DIGEST, 4096), session());
        let mut mem = memory(0x10000, 4096, DIGEST);
        ledger
            .admit_region(&signed, 0x10000, NOW, &mut mem)
            .unwrap();
        assert!(ledger.retire(1));
        assert!(ledger.regions().is_empty());
        assert!(!ledger.retire(1), "already gone");
        assert_eq!(
            ledger.admit_region(&signed, 0x10000, NOW, &mut mem),
            Err(Refusal::OutOfOrder),
            "a retired record is not a way back in"
        );
    }

    #[test]
    fn session_serials_may_arrive_in_any_order_but_never_twice() {
        let mut ledger = session_ledger();
        let mut mem = Memory {
            known: vec![(0x10000, 4096, DIGEST), (0x20000, 4096, DIGEST)],
            asked: Vec::new(),
        };
        let two = sign(&region_record(&ledger, 2, DIGEST, 4096), session());
        let one = sign(&region_record(&ledger, 1, DIGEST, 4096), session());
        assert_eq!(ledger.admit_region(&two, 0x10000, NOW, &mut mem), Ok(2));
        assert_eq!(
            ledger.admit_region(&one, 0x20000, NOW, &mut mem),
            Ok(1),
            "two signers can race"
        );
        assert_eq!(
            ledger.admit_region(&two, 0x30000, NOW, &mut mem),
            Err(Refusal::OutOfOrder)
        );
        let zero = sign(&region_record(&ledger, 0, DIGEST, 4096), session());
        assert_eq!(
            ledger.admit_region(&zero, 0x30000, NOW, &mut mem),
            Err(Refusal::OutOfOrder)
        );
    }

    #[test]
    fn a_record_from_another_run_is_refused() {
        let other = LiveCodeLedger::session(
            [8u8; 32],
            roots(),
            TrustedProducers::from_baseline(&baseline()),
        )
        .unwrap();
        let captured = sign(&region_record(&other, 1, DIGEST, 4096), session());
        let mut ledger = session_ledger();
        let mut mem = memory(0x10000, 4096, DIGEST);
        assert_eq!(
            ledger.admit_region(&captured, 0x10000, NOW, &mut mem),
            Err(Refusal::WrongRoot)
        );
    }

    // ---- signatures and who may sign what -----------------------------------------------------------------------

    #[test]
    fn a_record_signed_by_an_unknown_key_or_changed_after_signing_is_refused() {
        let mut ledger = session_ledger();
        let mut mem = memory(0x10000, 4096, DIGEST);
        let stranger = sign(
            &region_record(&ledger, 1, DIGEST, 4096),
            SigningKey::from_bytes(&[9u8; 32]),
        );
        assert_eq!(
            ledger.admit_region(&stranger, 0x10000, NOW, &mut mem),
            Err(Refusal::BadSignature)
        );

        let mut tampered = sign(&region_record(&ledger, 1, DIGEST, 4096), session());
        tampered.raw = tampered.raw.replace("\"serial\":1", "\"serial\":2");
        assert_eq!(
            ledger.admit_region(&tampered, 0x10000, NOW, &mut mem),
            Err(Refusal::BadSignature)
        );

        let mut short = sign(&region_record(&ledger, 1, DIGEST, 4096), session());
        short.signature.truncate(100);
        assert_eq!(
            ledger.admit_region(&short, 0x10000, NOW, &mut mem),
            Err(Refusal::BadSignature)
        );

        let mut upper = sign(&region_record(&ledger, 1, DIGEST, 4096), session());
        upper.signature = upper.signature.to_uppercase();
        assert_eq!(
            ledger.admit_region(&upper, 0x10000, NOW, &mut mem),
            Err(Refusal::BadSignature),
            "only the lower-case form is accepted"
        );
        assert!(ledger.regions().is_empty());
        assert!(
            mem.asked.is_empty(),
            "nothing unsigned reached the memory read"
        );
    }

    #[test]
    fn a_key_signs_only_its_own_purposes() {
        let mut files = persistent();
        let b = baseline();
        // A device key cannot sign a hot update; a session key cannot sign a local build; a release key cannot sign
        // generated code.
        let hot_by_device = sign(
            &file_record(&files, 1, Purpose::HotUpdate, "new.dll", Role::Other),
            device(),
        );
        assert_eq!(
            files.admit_file(&hot_by_device, &b, NOW),
            Err(Refusal::WrongAuthority)
        );
        let local_by_session = sign(
            &file_record(&files, 1, Purpose::LocalBuild, "mod.dll", Role::Other),
            session(),
        );
        assert_eq!(
            files.admit_file(&local_by_session, &b, NOW),
            Err(Refusal::WrongAuthority)
        );
        let local_by_release = sign(
            &file_record(&files, 1, Purpose::LocalBuild, "mod.dll", Role::Other),
            release(),
        );
        assert_eq!(
            files.admit_file(&local_by_release, &b, NOW),
            Err(Refusal::WrongAuthority)
        );

        let mut mem_ledger = session_ledger();
        let mut mem = memory(0x10000, 4096, DIGEST);
        let jit_by_release = sign(&region_record(&mem_ledger, 1, DIGEST, 4096), release());
        assert_eq!(
            mem_ledger.admit_region(&jit_by_release, 0x10000, NOW, &mut mem),
            Err(Refusal::WrongAuthority)
        );
        assert!(files.files().is_empty() && mem_ledger.regions().is_empty());
    }

    #[test]
    fn one_key_in_two_roles_is_refused_at_construction() {
        let same = release().verifying_key().to_bytes();
        let bad = TrustRoots {
            release: Some(same),
            device: Some(same),
            session: None,
        };
        assert!(!bad.distinct());
        assert!(LiveCodeLedger::session(NONCE, bad, TrustedProducers::new()).is_err());
        assert!(LiveCodeLedger::persistent(b"x", bad, TrustedProducers::new()).is_err());
        assert!(roots().distinct());
        assert!(TrustRoots::default().distinct());
    }

    #[test]
    fn only_the_one_canonical_text_of_a_record_is_accepted() {
        let ledger = session_ledger();
        let record = region_record(&ledger, 1, DIGEST, 4096);
        let canonical = String::from_utf8(record.signing_bytes().unwrap()).unwrap();
        let mut mem = memory(0x10000, 4096, DIGEST);
        let key = session();
        // Each variant is signed by the real key, so only the shape check can stop it.
        let spaced = canonical.replacen(",\"serial\"", ", \"serial\"", 1);
        let extra = canonical.replacen(
            "{\"schema_version\":1",
            "{\"schema_version\":1,\"note\":\"x\"",
            1,
        );
        let newline = format!("{canonical}\n");
        for text in [spaced, extra, newline] {
            let mut forged = SignedRecord {
                raw: text.clone(),
                signature: String::new(),
            };
            forged.signature = hex::encode(key.sign(text.as_bytes()).to_bytes());
            let mut l = session_ledger();
            assert_eq!(
                l.admit_region(&forged, 0x10000, NOW, &mut mem),
                Err(Refusal::NotCanonical),
                "{text}"
            );
        }
        // And the canonical text itself is fine.
        let mut l = session_ledger();
        assert_eq!(
            l.admit_region(&sign(&record, session()), 0x10000, NOW, &mut mem),
            Ok(1)
        );
    }

    #[test]
    fn oversized_and_wrong_schema_records_are_refused() {
        let mut ledger = session_ledger();
        let mut mem = memory(0x10000, 4096, DIGEST);
        let mut big = sign(&region_record(&ledger, 1, DIGEST, 4096), session());
        big.raw.push_str(&" ".repeat(MAX_RECORD_BYTES));
        assert_eq!(
            ledger.admit_region(&big, 0x10000, NOW, &mut mem),
            Err(Refusal::TooLarge)
        );
        let mut old = region_record(&ledger, 1, DIGEST, 4096);
        old.schema_version = 2;
        assert_eq!(
            ledger.admit_region(&sign(&old, session()), 0x10000, NOW, &mut mem),
            Err(Refusal::WrongSchema)
        );
    }

    #[test]
    fn a_region_record_cannot_go_in_the_persistent_ledger_nor_a_file_in_the_session_one() {
        let mut files = persistent();
        let mut mem = memory(0x10000, 4096, DIGEST);
        let region_in_files = sign(&region_record(&files, 1, DIGEST, 4096), session());
        assert_eq!(
            files.admit_region(&region_in_files, 0x10000, NOW, &mut mem),
            Err(Refusal::WrongScope)
        );
        let mut mem_ledger = session_ledger();
        let file_in_session = sign(
            &file_record(&mem_ledger, 1, Purpose::HotUpdate, "a.dll", Role::Other),
            release(),
        );
        assert_eq!(
            mem_ledger.admit_file(&file_in_session, &baseline(), NOW),
            Err(Refusal::WrongScope)
        );
    }

    // ---- time and producers ----------------------------------------------------------------------------------------

    #[test]
    fn time_rules() {
        let mut ledger = session_ledger();
        let mut mem = memory(0x10000, 4096, DIGEST);
        let mut future = region_record(&ledger, 1, DIGEST, 4096);
        future.created_unix = NOW + CLOCK_SKEW_SECS + 1;
        assert_eq!(
            ledger.admit_region(&sign(&future, session()), 0x10000, NOW, &mut mem),
            Err(Refusal::FromTheFuture)
        );
        let mut edge = region_record(&ledger, 1, DIGEST, 4096);
        edge.created_unix = NOW + CLOCK_SKEW_SECS;
        edge.expires_unix = Some(NOW + CLOCK_SKEW_SECS + 10);
        assert_eq!(
            ledger.admit_region(&sign(&edge, session()), 0x10000, NOW, &mut mem),
            Ok(1),
            "skew is allowed"
        );

        let mut expired = region_record(&ledger, 2, DIGEST, 4096);
        expired.expires_unix = Some(NOW);
        assert_eq!(
            ledger.admit_region(&sign(&expired, session()), 0x20000, NOW, &mut mem),
            Err(Refusal::Expired),
            "expires at the instant it is presented"
        );
        // Made earlier, expires exactly now: expired. (Made at `NOW` this would be caught by the rule below instead.)
        let mut at_the_instant = region_record(&ledger, 4, DIGEST, 4096);
        at_the_instant.created_unix = NOW - 10;
        at_the_instant.expires_unix = Some(NOW);
        assert_eq!(
            ledger.admit_region(&sign(&at_the_instant, session()), 0x20000, NOW, &mut mem),
            Err(Refusal::Expired),
            "a record is valid until its expiry, not at it"
        );
        let mut backwards = region_record(&ledger, 3, DIGEST, 4096);
        backwards.expires_unix = Some(backwards.created_unix);
        assert_eq!(
            ledger.admit_region(&sign(&backwards, session()), 0x20000, NOW, &mut mem),
            Err(Refusal::Expired),
            "expires no later than it was made"
        );
        // Expiry removes coverage.
        assert_eq!(ledger.prune_expired(NOW + CLOCK_SKEW_SECS + 9), 0);
        assert_eq!(ledger.prune_expired(NOW + CLOCK_SKEW_SECS + 10), 1);
        assert!(ledger.regions().is_empty());
    }

    #[test]
    fn the_producer_must_be_code_the_baseline_lists() {
        let ledger = session_ledger();
        let mut mem = memory(0x10000, 4096, DIGEST);
        for (producer, expect) in [
            ("sul.exe", true),
            ("SUL.EXE", true),
            ("sul-vault.dll", true),
            ("evil.dll", false),
            ("", false),
            ("..\\sul.exe", false),
            ("bin/sul.exe", false),
        ] {
            let mut l = ledger.clone();
            let mut r = region_record(&l, 1, DIGEST, 4096);
            r.producer = producer.into();
            let got = l.admit_region(&sign(&r, session()), 0x10000, NOW, &mut mem);
            assert_eq!(got.is_ok(), expect, "{producer:?}: {got:?}");
            if !expect {
                assert_eq!(got, Err(Refusal::UntrustedProducer));
            }
        }
    }

    #[test]
    fn producers_come_from_code_roles_only() {
        let mut b = baseline();
        b.entries.push(watched("notes.txt", Role::Other, None));
        b.entries.push(watched("model.gguf", Role::ModelFile, None));
        let p = TrustedProducers::from_baseline(&b);
        assert!(
            p.contains("sul.exe") && p.contains("sul-vault.dll") && p.contains("sul-updater.exe")
        );
        assert!(!p.contains("notes.txt") && !p.contains("model.gguf"));
    }

    // ---- files -----------------------------------------------------------------------------------------------------

    #[test]
    fn persistent_serials_must_rise_so_a_rollback_is_refused() {
        let mut ledger = persistent();
        let b = baseline();
        let five = sign(
            &file_record(&ledger, 5, Purpose::HotUpdate, "a.dll", Role::Other),
            release(),
        );
        assert_eq!(ledger.admit_file(&five, &b, NOW), Ok(5));
        let four = sign(
            &file_record(&ledger, 4, Purpose::HotUpdate, "b.dll", Role::Other),
            release(),
        );
        assert_eq!(
            ledger.admit_file(&four, &b, NOW),
            Err(Refusal::OutOfOrder),
            "older"
        );
        assert_eq!(
            ledger.admit_file(&five, &b, NOW),
            Err(Refusal::OutOfOrder),
            "the same one again"
        );
        let nine = sign(
            &file_record(&ledger, 9, Purpose::HotUpdate, "c.dll", Role::Other),
            release(),
        );
        assert_eq!(ledger.admit_file(&nine, &b, NOW), Ok(9), "gaps are fine");
    }

    #[test]
    fn a_ledger_made_for_another_baseline_is_refused() {
        let other = LiveCodeLedger::persistent(
            b"another baseline",
            roots(),
            TrustedProducers::from_baseline(&baseline()),
        )
        .unwrap();
        let record = sign(
            &file_record(&other, 1, Purpose::HotUpdate, "a.dll", Role::Other),
            release(),
        );
        let mut ledger = persistent();
        assert_eq!(
            ledger.admit_file(&record, &baseline(), NOW),
            Err(Refusal::WrongRoot)
        );
    }

    #[test]
    fn file_paths_digests_and_roles_are_checked() {
        let b = baseline();
        for path in [
            "../x.dll",
            "/etc/x.dll",
            "C:/x.dll",
            "a\\b.dll",
            "",
            "a/../../b.dll",
        ] {
            let mut l = persistent();
            let s = sign(
                &file_record(&l, 1, Purpose::HotUpdate, path, Role::Other),
                release(),
            );
            assert_eq!(
                l.admit_file(&s, &b, NOW),
                Err(Refusal::ForbiddenPath),
                "{path:?}"
            );
        }
        let mut l = persistent();
        let mut bad = file_record(&l, 1, Purpose::HotUpdate, "a.dll", Role::Other);
        bad.content = Content::File {
            path: "a.dll".into(),
            sha256: "ABC".into(),
            size: 1,
            role: Role::Other,
        };
        assert_eq!(
            l.admit_file(&sign(&bad, release()), &b, NOW),
            Err(Refusal::BadDigest)
        );
        // A local build may be a helper, a mod or a model adapter, never SUL's own program, a manifest or the trust layer.
        for (role, ok) in [
            (Role::Other, true),
            (Role::Sidecar, true),
            (Role::ModelFile, true),
            (Role::Binary, false),
            (Role::Manifest, false),
            (Role::TrustPrimitive, false),
        ] {
            let mut l = persistent();
            let s = sign(
                &file_record(&l, 1, Purpose::LocalBuild, "mods/x.dll", role),
                device(),
            );
            let got = l.admit_file(&s, &b, NOW);
            assert_eq!(got.is_ok(), ok, "{role:?}: {got:?}");
            if !ok {
                assert_eq!(got, Err(Refusal::ForbiddenRole));
            }
        }
        // A hot update may carry any role.
        let mut l = persistent();
        let s = sign(
            &file_record(
                &l,
                1,
                Purpose::HotUpdate,
                "sul-new.dll",
                Role::TrustPrimitive,
            ),
            release(),
        );
        assert_eq!(l.admit_file(&s, &b, NOW), Ok(1));
    }

    #[test]
    fn a_device_key_adds_files_but_never_replaces_a_guarded_one() {
        let b = baseline();
        let mut l = persistent();
        // Replacing a file the baseline lists: only a release-signed update may.
        let local_over_baseline = sign(
            &file_record(&l, 1, Purpose::LocalBuild, "sul.exe", Role::Other),
            device(),
        );
        assert_eq!(
            l.admit_file(&local_over_baseline, &b, NOW),
            Err(Refusal::CannotReplace)
        );
        // Case does not hide it on Windows.
        if cfg!(windows) {
            let upper = sign(
                &file_record(&l, 1, Purpose::LocalBuild, "SUL.EXE", Role::Other),
                device(),
            );
            assert_eq!(l.admit_file(&upper, &b, NOW), Err(Refusal::CannotReplace));
        }
        let update = sign(
            &file_record(&l, 1, Purpose::HotUpdate, "sul.exe", Role::Binary),
            release(),
        );
        assert_eq!(l.admit_file(&update, &b, NOW), Ok(1));

        // A local build can replace its own earlier build, but not a file a hot update owns.
        let local = sign(
            &file_record(&l, 2, Purpose::LocalBuild, "mods/m.dll", Role::Other),
            device(),
        );
        assert_eq!(l.admit_file(&local, &b, NOW), Ok(2));
        let local_again = sign(
            &file_record(&l, 3, Purpose::LocalBuild, "mods/m.dll", Role::Other),
            device(),
        );
        assert_eq!(l.admit_file(&local_again, &b, NOW), Ok(3));
        assert_eq!(
            l.files().iter().filter(|f| f.path == "mods/m.dll").count(),
            1,
            "replaced, not duplicated"
        );
        let hot_new = sign(
            &file_record(&l, 4, Purpose::HotUpdate, "mods/m.dll", Role::Other),
            release(),
        );
        assert_eq!(
            l.admit_file(&hot_new, &b, NOW),
            Ok(4),
            "a hot update may take over a local build's path"
        );
        let local_over_hot = sign(
            &file_record(&l, 5, Purpose::LocalBuild, "mods/m.dll", Role::Other),
            device(),
        );
        assert_eq!(
            l.admit_file(&local_over_hot, &b, NOW),
            Err(Refusal::CannotReplace)
        );
        let local_over_hot_update = sign(
            &file_record(&l, 6, Purpose::LocalBuild, "sul.exe", Role::Other),
            device(),
        );
        assert_eq!(
            l.admit_file(&local_over_hot_update, &b, NOW),
            Err(Refusal::CannotReplace)
        );
    }

    #[test]
    fn a_refused_record_changes_nothing() {
        let mut l = persistent();
        let b = baseline();
        let ok = sign(
            &file_record(&l, 1, Purpose::LocalBuild, "mods/a.dll", Role::Other),
            device(),
        );
        l.admit_file(&ok, &b, NOW).unwrap();
        let before = (l.files().to_vec(), l.high_water);
        let bad = sign(
            &file_record(&l, 7, Purpose::LocalBuild, "sul.exe", Role::Other),
            device(),
        );
        assert!(l.admit_file(&bad, &b, NOW).is_err());
        assert_eq!(
            (l.files().to_vec(), l.high_water),
            before,
            "the high-water mark did not move either"
        );
    }

    #[test]
    fn the_effective_baseline_adds_replaces_keeps_codes_and_drops_the_expired() {
        let b = baseline();
        let mut l = persistent();
        let add = sign(
            &file_record(&l, 1, Purpose::LocalBuild, "mods/z.dll", Role::Other),
            device(),
        );
        let replace = sign(
            &file_record(
                &l,
                2,
                Purpose::HotUpdate,
                "sul-vault.dll",
                Role::TrustPrimitive,
            ),
            release(),
        );
        let mut temp = file_record(&l, 3, Purpose::LocalBuild, "tools/tmp.exe", Role::Sidecar);
        temp.expires_unix = Some(NOW + 100);
        let temp = sign(&temp, device());
        for s in [&add, &replace, &temp] {
            l.admit_file(s, &b, NOW).unwrap();
        }
        let e = l.effective_baseline(&b, NOW).unwrap();
        let paths: Vec<&str> = e.entries.iter().map(|w| w.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "mods/z.dll",
                "sul-updater.exe",
                "sul-vault.dll",
                "sul.exe",
                "tools/tmp.exe"
            ],
            "sorted, valid"
        );
        let vault = e.get("sul-vault.dll").unwrap();
        assert_eq!(vault.sha256, "b".repeat(64), "replaced");
        assert_eq!(
            vault.artifact_code.as_deref(),
            Some("E_VAULT_TAMPER_DETECTED"),
            "the code it had is kept"
        );
        assert_eq!(
            b.get("sul-vault.dll").unwrap().sha256,
            "a".repeat(64),
            "the signed baseline itself is untouched"
        );
        // After the temporary tool expires it is no longer vouched for.
        let later = l.effective_baseline(&b, NOW + 100).unwrap();
        assert!(later.get("tools/tmp.exe").is_none() && later.get("mods/z.dll").is_some());
        assert_eq!(l.prune_expired(NOW + 100), 1);
        assert!(l.retire(1));
        assert!(
            l.effective_baseline(&b, NOW)
                .unwrap()
                .get("mods/z.dll")
                .is_none()
        );
    }

    // ---- shape of the data ------------------------------------------------------------------------------------------

    #[test]
    fn signed_records_survive_being_stored() {
        let ledger = session_ledger();
        let s = sign(&region_record(&ledger, 1, DIGEST, 4096), session());
        let stored = serde_json::to_string(&s).unwrap();
        let back: SignedRecord = serde_json::from_str(&stored).unwrap();
        assert_eq!(back, s);
        let mut l = session_ledger();
        let mut mem = memory(0x10000, 4096, DIGEST);
        assert_eq!(
            l.admit_region(&back, 0x10000, NOW, &mut mem),
            Ok(1),
            "it still verifies after a round trip"
        );
    }

    #[test]
    fn every_refusal_has_a_distinct_word_and_a_sentence() {
        let all = [
            Refusal::TooLarge,
            Refusal::BadSignature,
            Refusal::NotCanonical,
            Refusal::WrongSchema,
            Refusal::WrongAuthority,
            Refusal::WrongScope,
            Refusal::WrongRoot,
            Refusal::OutOfOrder,
            Refusal::Expired,
            Refusal::FromTheFuture,
            Refusal::UntrustedProducer,
            Refusal::ForbiddenPath,
            Refusal::ForbiddenRole,
            Refusal::CannotReplace,
            Refusal::BadDigest,
            Refusal::BadSize,
            Refusal::Overlap,
            Refusal::ContentMismatch,
            Refusal::Unreadable,
        ];
        let words: BTreeSet<&str> = all.iter().map(|r| r.as_str()).collect();
        assert_eq!(words.len(), all.len());
        for r in all {
            assert!(
                r.as_str()
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'_'),
                "{}",
                r.as_str()
            );
            assert!(!r.describe().is_empty());
        }
        let certain: Vec<Refusal> = all.into_iter().filter(|r| r.is_certain()).collect();
        assert_eq!(
            certain,
            [
                Refusal::BadSignature,
                Refusal::WrongAuthority,
                Refusal::ContentMismatch
            ]
        );
    }

    /// The digest contract is BLAKE3-256 of the bytes, and nothing else: a digester built on the real hash agrees with a
    /// record signed over the same bytes.
    #[test]
    fn the_digest_contract_is_blake3_of_the_bytes() {
        struct Real(Vec<u8>);
        impl RegionDigester for Real {
            fn blake3(&mut self, _base: u64, len: u64) -> Result<[u8; 32]> {
                Ok(*blake3::hash(&self.0[..len as usize]).as_bytes())
            }
        }
        let code = vec![0x90u8; 512];
        let digest = *blake3::hash(&code).as_bytes();
        let mut l = session_ledger();
        let s = sign(&region_record(&l, 1, digest, 512), session());
        assert_eq!(
            l.admit_region(&s, 0x4000, NOW, &mut Real(code.clone())),
            Ok(1)
        );
        let mut changed = code;
        changed[100] ^= 1;
        let mut l2 = session_ledger();
        assert_eq!(
            l2.admit_region(&s, 0x4000, NOW, &mut Real(changed)),
            Err(Refusal::ContentMismatch)
        );
    }
}
