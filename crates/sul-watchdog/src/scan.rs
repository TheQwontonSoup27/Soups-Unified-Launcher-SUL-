//! Process tamper analysis (Ch. 12 §2.8, Ch. 14 §14.13.3): given a snapshot of SUL's own process, find
//!
//! * **RWX regions** SUL did not allocate (readable, writable and executable memory is where injected code lives),
//! * **API hooks**: a function that starts with different bytes than the file it was loaded from,
//! * **foreign threads**: a thread that starts in memory that is not a loaded file, or in a module that was not loaded at the
//!   known-good point,
//! * **DLL hollowing**: a loaded image whose code section differs from its file in so many bytes that it was replaced, not
//!   patched.
//!
//! This module is analysis only: it takes a [`Snapshot`] and returns what is suspicious. It reads no process, calls no
//! operating-system function and has no `unsafe` code, so every rule is tested here with made-up snapshots and is open to
//! Kani and Miri. Filling a snapshot from a live Windows process (`VirtualQuery`, a toolhelp snapshot, reading the module
//! files) is the job of a [`ProcessProbe`], which is the next cut (decision 0014); nothing implements it yet.
//!
//! Every finding here is **suspicious**, never confirmed: these are heuristics about a running process, and an antivirus
//! product, an overlay or a debugger legitimately does all four. The user-visible [`Allowlist`] is how a person says "this
//! one is expected"; allowed findings are still returned (in [`ScanReport::allowed`]) so the screen can show what was
//! allowed and why, and the log can say so. Assumes 64-bit code: the patch shapes are the x64 and x86 detours MinHook and
//! its relatives write.
//!
//! The spec lists four detections and three error codes (`E_WATCHDOG_TAMPER`, `E_WATCHDOG_HOOK_DETECTED`,
//! `E_WATCHDOG_RWX_REGION`). Hooks and RWX regions have theirs; foreign threads and hollowing have none (decision 0014).

use crate::code::WatchdogCode;
use serde::Serialize;
use sul_core::Result;
use sul_trust::findings::Certainty;

/// A changed section is "hollowed" rather than "patched" when at least this share of its bytes differ (numerator, over 100).
/// A hook changes a handful of bytes; a replaced image changes most of them. Measured against nothing yet: it is a
/// starting value, named so that it is one place to change.
pub const HOLLOW_PERCENT: u64 = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Protection {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl Protection {
    pub const fn new(read: bool, write: bool, execute: bool) -> Self {
        Self {
            read,
            write,
            execute,
        }
    }

    pub const READ_WRITE: Self = Self::new(true, true, false);
    pub const READ_EXECUTE: Self = Self::new(true, false, true);
    pub const READ_WRITE_EXECUTE: Self = Self::new(true, true, true);

    pub const fn is_rwx(self) -> bool {
        self.read && self.write && self.execute
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    /// Backed by a loaded program or library file.
    Image,
    /// A mapped file or shared section that is not a loaded image.
    Mapped,
    /// Memory the process allocated for itself.
    Private,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    pub base: u64,
    pub size: u64,
    pub protection: Protection,
    pub kind: RegionKind,
    /// The file name (no folder) of the module an image region belongs to.
    pub module: Option<String>,
}

impl Region {
    pub fn end(&self) -> u64 {
        self.base.saturating_add(self.size)
    }

    pub fn contains(&self, address: u64) -> bool {
        address >= self.base && address - self.base < self.size
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thread {
    pub id: u32,
    /// The address the thread's own code started at (not the loader stub that runs first).
    pub start_address: u64,
}

/// The first bytes of one exported function, as loaded and as in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prologue {
    pub module: String,
    pub function: String,
    pub in_memory: Vec<u8>,
    pub on_disk: Vec<u8>,
    /// The module a detour jumps to, when the probe could resolve it (the usual way to tell an antivirus hook from a cheat).
    pub target_module: Option<String>,
}

/// How much of one code section differs between memory and its file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionDiff {
    pub module: String,
    pub section: String,
    pub total_bytes: u64,
    pub changed_bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub regions: Vec<Region>,
    pub threads: Vec<Thread>,
    pub prologues: Vec<Prologue>,
    pub sections: Vec<SectionDiff>,
}

/// Something that can look at the live process. Implemented by the Windows adapter (decision 0014); the analysis does not
/// care how a snapshot was taken.
pub trait ProcessProbe {
    fn snapshot(&mut self) -> Result<Snapshot>;
}

/// What SUL itself did, so that it is not reported against it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Expectations {
    /// Memory SUL allocated as RWX on purpose (the trampolines of the MinHook opt-in, Ch. 7 §2.1): `(base, size)`.
    pub own_rwx: Vec<(u64, u64)>,
    /// Hooks SUL installed itself: `(module file name, function)`.
    pub own_hooks: Vec<(String, String)>,
    /// File names of the modules loaded at the known-good point (after start-up, before anything else could be injected).
    pub known_modules: Vec<String>,
}

impl Expectations {
    fn covers(&self, region: &Region) -> bool {
        self.own_rwx
            .iter()
            .any(|&(base, size)| region.base >= base && region.end() <= base.saturating_add(size))
    }

    fn is_known_module(&self, name: &str) -> bool {
        self.known_modules.iter().any(|m| same_name(m, name))
    }

    fn is_own_hook(&self, module: &str, function: &str) -> bool {
        self.own_hooks
            .iter()
            .any(|(m, f)| same_name(m, module) && f == function)
    }
}

/// One thing a person said is expected (an antivirus, an overlay), and why. Shown to them, so the reason is in their words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AllowEntry {
    /// A module file name, compared whole and without regard to case: `av_hook.dll` allows that file and no other.
    pub module: String,
    pub reason: String,
    pub added_unix: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Allowlist {
    entries: Vec<AllowEntry>,
}

fn file_name(text: &str) -> &str {
    text.rsplit(['/', '\\']).next().unwrap_or(text)
}

fn same_name(a: &str, b: &str) -> bool {
    file_name(a).eq_ignore_ascii_case(file_name(b))
}

impl Allowlist {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn allow(&mut self, module: &str, reason: &str, added_unix: u64) {
        let module = file_name(module).to_owned();
        if module.is_empty() || self.entries.iter().any(|e| same_name(&e.module, &module)) {
            return;
        }
        self.entries.push(AllowEntry {
            module,
            reason: reason.to_owned(),
            added_unix,
        });
    }

    pub fn remove(&mut self, module: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| !same_name(&e.module, module));
        self.entries.len() != before
    }

    pub fn matches(&self, module: &str) -> Option<&AllowEntry> {
        self.entries.iter().find(|e| same_name(&e.module, module))
    }

    pub fn entries(&self) -> &[AllowEntry] {
        &self.entries
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanKind {
    RwxRegion,
    InlineHook,
    ForeignThread,
    ImageHollowing,
}

impl ScanKind {
    /// The ledger code for this kind of detection, when it has one (see the module documentation).
    pub const fn code(self) -> Option<WatchdogCode> {
        match self {
            Self::RwxRegion => Some(WatchdogCode::RwxRegion),
            Self::InlineHook => Some(WatchdogCode::HookDetected),
            Self::ForeignThread | Self::ImageHollowing => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScanFinding {
    pub kind: ScanKind,
    pub certainty: Certainty,
    /// What it is about: an address range, a `module!function`, a thread id. Module names, never folders.
    pub subject: String,
    pub detail: String,
    /// The reason the person gave, when an allowlist entry matched.
    pub allowed_because: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct ScanReport {
    pub findings: Vec<ScanFinding>,
    pub allowed: Vec<ScanFinding>,
    pub regions_examined: usize,
    pub threads_examined: usize,
    pub prologues_examined: usize,
    pub sections_examined: usize,
}

impl ScanReport {
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}

/// How a changed function start is shaped, judged from the bytes now in memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchShape {
    /// `E9 rel32`: a jump within 2 GiB.
    RelativeJump,
    /// `EB rel8`: a short jump (usually onward to a bigger one).
    ShortJump,
    /// `FF 25 disp32`: a jump through a pointer.
    IndirectJump,
    /// `mov rax, imm64 ; jmp rax` or `mov r11, imm64 ; jmp r11`: a jump to any address.
    AbsoluteJump,
    /// `push imm32 ; ret`.
    PushReturn,
    /// `CC`: a software breakpoint.
    Breakpoint,
    /// Different bytes that are none of the above.
    Other,
}

impl PatchShape {
    pub const fn describe(self) -> &'static str {
        match self {
            Self::RelativeJump => "a relative jump",
            Self::ShortJump => "a short jump",
            Self::IndirectJump => "a jump through a pointer",
            Self::AbsoluteJump => "a jump to an absolute address",
            Self::PushReturn => "a push-and-return jump",
            Self::Breakpoint => "a breakpoint",
            Self::Other => "different bytes",
        }
    }
}

/// `None` when the bytes in memory match the file over the length both have; otherwise what the change looks like.
pub fn classify_patch(in_memory: &[u8], on_disk: &[u8]) -> Option<PatchShape> {
    let n = in_memory.len().min(on_disk.len());
    if n == 0 || in_memory[..n] == on_disk[..n] {
        return None;
    }
    let m = in_memory;
    Some(match m {
        [0xE9, ..] => PatchShape::RelativeJump,
        [0xEB, ..] => PatchShape::ShortJump,
        [0xFF, 0x25, ..] => PatchShape::IndirectJump,
        [0x48, 0xB8, _, _, _, _, _, _, _, _, 0xFF, 0xE0, ..] => PatchShape::AbsoluteJump,
        [0x49, 0xBB, _, _, _, _, _, _, _, _, 0x41, 0xFF, 0xE3, ..] => PatchShape::AbsoluteJump,
        [0x68, _, _, _, _, 0xC3, ..] => PatchShape::PushReturn,
        [0xCC, ..] => PatchShape::Breakpoint,
        _ => PatchShape::Other,
    })
}

fn finding(kind: ScanKind, subject: String, detail: String) -> ScanFinding {
    ScanFinding {
        kind,
        certainty: Certainty::Suspicious,
        subject,
        detail,
        allowed_because: None,
    }
}

fn file(report: &mut ScanReport, mut found: ScanFinding, module: Option<&str>, allow: &Allowlist) {
    match module.and_then(|m| allow.matches(m)) {
        Some(entry) => {
            found.allowed_because = Some(entry.reason.clone());
            report.allowed.push(found);
        }
        None => report.findings.push(found),
    }
}

/// Apply every rule to one snapshot. Findings come out in a fixed order (kind, then subject), whatever order the snapshot
/// listed things in.
pub fn analyze(snapshot: &Snapshot, expect: &Expectations, allow: &Allowlist) -> ScanReport {
    let mut report = ScanReport {
        regions_examined: snapshot.regions.len(),
        threads_examined: snapshot.threads.len(),
        prologues_examined: snapshot.prologues.len(),
        sections_examined: snapshot.sections.len(),
        ..ScanReport::default()
    };

    for region in &snapshot.regions {
        if !region.protection.is_rwx() || expect.covers(region) {
            continue;
        }
        let what = match region.kind {
            RegionKind::Image => "image",
            RegionKind::Mapped => "mapped",
            RegionKind::Private => "private",
        };
        let found = finding(
            ScanKind::RwxRegion,
            format!("{:#x} ({} bytes)", region.base, region.size),
            format!("readable, writable and executable {what} memory that SUL did not allocate"),
        );
        let module = (region.kind == RegionKind::Image)
            .then_some(region.module.as_deref())
            .flatten();
        file(&mut report, found, module, allow);
    }

    for thread in &snapshot.threads {
        let region = snapshot
            .regions
            .iter()
            .find(|r| r.contains(thread.start_address));
        let subject = format!("thread {} at {:#x}", thread.id, thread.start_address);
        let (detail, module) = match region {
            None => ("starts at an address that is not mapped".to_owned(), None),
            Some(r) if r.kind == RegionKind::Image => {
                let name = r.module.as_deref().unwrap_or("");
                if !name.is_empty() && expect.is_known_module(name) && r.protection.execute {
                    continue;
                }
                if !r.protection.execute {
                    ("starts in memory that is not executable".to_owned(), None)
                } else {
                    (
                        format!("starts in {name:?}, which was not loaded at the known-good point"),
                        Some(name),
                    )
                }
            }
            Some(r) if r.protection.execute => (
                "starts in executable memory that is not a loaded file".to_owned(),
                None,
            ),
            Some(_) => ("starts in memory that is not executable".to_owned(), None),
        };
        file(
            &mut report,
            finding(ScanKind::ForeignThread, subject, detail),
            module.filter(|m| !m.is_empty()),
            allow,
        );
    }

    for p in &snapshot.prologues {
        if expect.is_own_hook(&p.module, &p.function) {
            continue;
        }
        let Some(shape) = classify_patch(&p.in_memory, &p.on_disk) else {
            continue;
        };
        let found = finding(
            ScanKind::InlineHook,
            format!("{}!{}", file_name(&p.module), p.function),
            format!(
                "starts with {} instead of the bytes in the file it was loaded from",
                shape.describe()
            ),
        );
        file(&mut report, found, p.target_module.as_deref(), allow);
    }

    for s in &snapshot.sections {
        if s.changed_bytes == 0 || s.total_bytes == 0 {
            continue;
        }
        let changed = s.changed_bytes.min(s.total_bytes);
        let hollowed = changed.saturating_mul(100) >= s.total_bytes.saturating_mul(HOLLOW_PERCENT);
        let (kind, detail) = if hollowed {
            (
                ScanKind::ImageHollowing,
                format!(
                    "section {} differs from its file in {changed} of {} bytes: the image looks replaced, not patched",
                    s.section, s.total_bytes
                ),
            )
        } else {
            (
                ScanKind::InlineHook,
                format!(
                    "section {} differs from its file in {changed} of {} bytes",
                    s.section, s.total_bytes
                ),
            )
        };
        file(
            &mut report,
            finding(
                kind,
                format!("{}:{}", file_name(&s.module), s.section),
                detail,
            ),
            Some(&s.module),
            allow,
        );
    }

    let order = |a: &ScanFinding, b: &ScanFinding| {
        (a.kind, &a.subject, &a.detail).cmp(&(b.kind, &b.subject, &b.detail))
    };
    report.findings.sort_by(order);
    report.allowed.sort_by(order);
    report
}

/// Take a snapshot with `probe` and analyse it.
pub fn scan(
    probe: &mut dyn ProcessProbe,
    expect: &Expectations,
    allow: &Allowlist,
) -> Result<ScanReport> {
    Ok(analyze(&probe.snapshot()?, expect, allow))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(
        base: u64,
        size: u64,
        protection: Protection,
        kind: RegionKind,
        module: Option<&str>,
    ) -> Region {
        Region {
            base,
            size,
            protection,
            kind,
            module: module.map(str::to_owned),
        }
    }

    fn image(base: u64, module: &str) -> Region {
        region(
            base,
            0x1000,
            Protection::READ_EXECUTE,
            RegionKind::Image,
            Some(module),
        )
    }

    fn analyze_default(snapshot: &Snapshot) -> ScanReport {
        analyze(snapshot, &Expectations::default(), &Allowlist::new())
    }

    // ---- patch shapes ---------------------------------------------------------------------------------------------

    #[test]
    fn the_patch_shapes_are_recognised_from_the_bytes_in_memory() {
        let disk = [
            0x48, 0x89, 0x5C, 0x24, 0x08, 0x57, 0x48, 0x83, 0xEC, 0x20, 0x8B, 0xD9, 0x90, 0x90,
        ];
        let mut cases: Vec<(Vec<u8>, PatchShape)> = vec![
            (
                vec![0xE9, 0x10, 0x20, 0x30, 0x40, 0x90],
                PatchShape::RelativeJump,
            ),
            (vec![0xEB, 0x05, 0x90], PatchShape::ShortJump),
            (
                vec![0xFF, 0x25, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8],
                PatchShape::IndirectJump,
            ),
            (vec![0x68, 1, 2, 3, 4, 0xC3, 0x90], PatchShape::PushReturn),
            (vec![0xCC, 0x89, 0x5C], PatchShape::Breakpoint),
            (vec![0x90, 0x90, 0x90, 0x90], PatchShape::Other),
        ];
        let mut abs = vec![0x48, 0xB8];
        abs.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        abs.extend_from_slice(&[0xFF, 0xE0]);
        cases.push((abs, PatchShape::AbsoluteJump));
        let mut abs_r11 = vec![0x49, 0xBB];
        abs_r11.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        abs_r11.extend_from_slice(&[0x41, 0xFF, 0xE3]);
        cases.push((abs_r11, PatchShape::AbsoluteJump));
        for (bytes, shape) in cases {
            assert_eq!(classify_patch(&bytes, &disk), Some(shape), "{bytes:02X?}");
        }
    }

    #[test]
    fn identical_or_empty_prologues_are_not_a_patch() {
        let a = [0xE9, 1, 2, 3, 4];
        assert_eq!(
            classify_patch(&a, &a),
            None,
            "a function that really starts with a jump"
        );
        assert_eq!(classify_patch(&[], &a), None);
        assert_eq!(classify_patch(&a, &[]), None);
        // Different lengths compare over the shorter one.
        assert_eq!(classify_patch(&a, &a[..3]), None);
        assert_eq!(
            classify_patch(&[0xE9, 1, 9], &[0xE9, 1, 2, 3, 4]),
            Some(PatchShape::RelativeJump)
        );
    }

    #[test]
    fn a_truncated_absolute_jump_is_other_not_a_panic() {
        // `mov rax, imm64` cut short: not the full pattern, so it is just different bytes.
        let disk = [0x90; 14];
        assert_eq!(
            classify_patch(&[0x48, 0xB8, 1, 2, 3], &disk),
            Some(PatchShape::Other)
        );
        assert_eq!(
            classify_patch(&[0x68, 1, 2], &disk),
            Some(PatchShape::Other)
        );
    }

    // ---- RWX regions ----------------------------------------------------------------------------------------------

    #[test]
    fn only_readable_writable_executable_memory_is_an_rwx_finding() {
        let snapshot = Snapshot {
            regions: vec![
                region(
                    0x1000,
                    0x1000,
                    Protection::READ_WRITE,
                    RegionKind::Private,
                    None,
                ),
                region(
                    0x2000,
                    0x1000,
                    Protection::READ_EXECUTE,
                    RegionKind::Private,
                    None,
                ),
                region(
                    0x3000,
                    0x2000,
                    Protection::READ_WRITE_EXECUTE,
                    RegionKind::Private,
                    None,
                ),
            ],
            ..Snapshot::default()
        };
        let report = analyze_default(&snapshot);
        assert_eq!(report.findings.len(), 1, "{report:?}");
        let f = &report.findings[0];
        assert_eq!(f.kind, ScanKind::RwxRegion);
        assert_eq!(f.certainty, Certainty::Suspicious);
        assert_eq!(f.subject, "0x3000 (8192 bytes)");
        assert_eq!(f.kind.code(), Some(WatchdogCode::RwxRegion));
        assert_eq!(report.regions_examined, 3);
    }

    #[test]
    fn rwx_memory_sul_allocated_itself_is_not_reported_but_a_neighbour_is() {
        let expect = Expectations {
            own_rwx: vec![(0x10000, 0x4000)],
            ..Expectations::default()
        };
        let snapshot = Snapshot {
            regions: vec![
                region(
                    0x10000,
                    0x2000,
                    Protection::READ_WRITE_EXECUTE,
                    RegionKind::Private,
                    None,
                ),
                region(
                    0x12000,
                    0x2000,
                    Protection::READ_WRITE_EXECUTE,
                    RegionKind::Private,
                    None,
                ),
                // Starts inside the registered range but runs past its end: not covered.
                region(
                    0x13000,
                    0x2000,
                    Protection::READ_WRITE_EXECUTE,
                    RegionKind::Private,
                    None,
                ),
                region(
                    0x14000,
                    0x1000,
                    Protection::READ_WRITE_EXECUTE,
                    RegionKind::Private,
                    None,
                ),
            ],
            ..Snapshot::default()
        };
        let report = analyze(&snapshot, &expect, &Allowlist::new());
        let subjects: Vec<_> = report.findings.iter().map(|f| f.subject.as_str()).collect();
        assert_eq!(subjects, ["0x13000 (8192 bytes)", "0x14000 (4096 bytes)"]);
    }

    #[test]
    fn an_rwx_image_of_an_allowed_module_is_listed_as_allowed_not_dropped() {
        let mut allow = Allowlist::new();
        allow.allow("C:\\Program Files\\Overlay\\overlay64.dll", "my overlay", 5);
        let snapshot = Snapshot {
            regions: vec![
                region(
                    0x1000,
                    0x1000,
                    Protection::READ_WRITE_EXECUTE,
                    RegionKind::Image,
                    Some("Overlay64.DLL"),
                ),
                // A private region cannot be attributed to a module, so the allowlist does not apply to it.
                region(
                    0x5000,
                    0x1000,
                    Protection::READ_WRITE_EXECUTE,
                    RegionKind::Private,
                    Some("overlay64.dll"),
                ),
            ],
            ..Snapshot::default()
        };
        let report = analyze(&snapshot, &Expectations::default(), &allow);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].subject, "0x5000 (4096 bytes)");
        assert_eq!(report.allowed.len(), 1);
        assert_eq!(
            report.allowed[0].allowed_because.as_deref(),
            Some("my overlay")
        );
    }

    // ---- threads --------------------------------------------------------------------------------------------------

    #[test]
    fn a_thread_in_a_known_module_but_in_memory_that_is_not_executable_is_still_foreign() {
        // A known module is trusted to run code. A thread that starts in its data pages (readable and writable, not
        // executable) is not running that code. Found by a deliberate-bug check, 2026-11-26: the "known module" exemption
        // once applied to any memory of the module.
        let expect = Expectations {
            known_modules: vec!["sul.exe".into()],
            ..Expectations::default()
        };
        let snapshot = Snapshot {
            regions: vec![region(
                0x1000,
                0x1000,
                Protection::READ_WRITE,
                RegionKind::Image,
                Some("sul.exe"),
            )],
            threads: vec![Thread {
                id: 7,
                start_address: 0x1010,
            }],
            ..Snapshot::default()
        };
        let report = analyze(&snapshot, &expect, &Allowlist::new());
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].kind, ScanKind::ForeignThread);
        assert!(
            report.findings[0].detail.contains("not executable"),
            "{}",
            report.findings[0].detail
        );
    }

    #[test]
    fn a_thread_in_a_known_module_is_fine_and_others_are_foreign() {
        let expect = Expectations {
            known_modules: vec!["sul.exe".into(), "ntdll.dll".into()],
            ..Expectations::default()
        };
        let snapshot = Snapshot {
            regions: vec![
                image(0x1000, "SUL.EXE"),
                image(0x2000, "ntdll.dll"),
                image(0x3000, "late.dll"),
                region(
                    0x4000,
                    0x1000,
                    Protection::READ_EXECUTE,
                    RegionKind::Private,
                    None,
                ),
                region(
                    0x5000,
                    0x1000,
                    Protection::READ_EXECUTE,
                    RegionKind::Mapped,
                    None,
                ),
                region(
                    0x6000,
                    0x1000,
                    Protection::READ_WRITE,
                    RegionKind::Private,
                    None,
                ),
            ],
            threads: vec![
                Thread {
                    id: 1,
                    start_address: 0x1010,
                },
                Thread {
                    id: 2,
                    start_address: 0x2010,
                },
                Thread {
                    id: 3,
                    start_address: 0x3010,
                },
                Thread {
                    id: 4,
                    start_address: 0x4010,
                },
                Thread {
                    id: 5,
                    start_address: 0x5010,
                },
                Thread {
                    id: 6,
                    start_address: 0x6010,
                },
                Thread {
                    id: 7,
                    start_address: 0x9000,
                },
            ],
            ..Snapshot::default()
        };
        let report = analyze(&snapshot, &expect, &Allowlist::new());
        let subjects: Vec<_> = report.findings.iter().map(|f| f.subject.clone()).collect();
        assert_eq!(
            subjects,
            [
                "thread 3 at 0x3010",
                "thread 4 at 0x4010",
                "thread 5 at 0x5010",
                "thread 6 at 0x6010",
                "thread 7 at 0x9000"
            ]
        );
        assert!(
            report
                .findings
                .iter()
                .all(|f| f.kind == ScanKind::ForeignThread && f.kind.code().is_none())
        );
        assert!(report.findings[0].detail.contains("late.dll"));
        assert!(report.findings[4].detail.contains("not mapped"));
        assert_eq!(report.threads_examined, 7);
    }

    #[test]
    fn a_thread_in_an_allowed_late_module_is_allowed_with_its_reason() {
        let mut allow = Allowlist::new();
        allow.allow("av_agent.dll", "my antivirus", 1);
        let snapshot = Snapshot {
            regions: vec![image(0x3000, "av_agent.dll")],
            threads: vec![Thread {
                id: 9,
                start_address: 0x3008,
            }],
            ..Snapshot::default()
        };
        let report = analyze(&snapshot, &Expectations::default(), &allow);
        assert!(report.is_clean());
        assert_eq!(report.allowed.len(), 1);
        assert_eq!(
            report.allowed[0].allowed_because.as_deref(),
            Some("my antivirus")
        );
    }

    #[test]
    fn a_thread_that_starts_at_the_very_end_of_a_region_is_not_inside_it() {
        let snapshot = Snapshot {
            regions: vec![image(0x1000, "sul.exe")],
            threads: vec![Thread {
                id: 1,
                start_address: 0x2000,
            }],
            ..Snapshot::default()
        };
        let expect = Expectations {
            known_modules: vec!["sul.exe".into()],
            ..Expectations::default()
        };
        let report = analyze(&snapshot, &expect, &Allowlist::new());
        assert_eq!(report.findings.len(), 1, "0x2000 is one past the region");
    }

    // ---- hooks ----------------------------------------------------------------------------------------------------

    fn prologue(module: &str, function: &str, in_memory: &[u8], target: Option<&str>) -> Prologue {
        Prologue {
            module: module.into(),
            function: function.into(),
            in_memory: in_memory.to_vec(),
            on_disk: vec![0x48, 0x89, 0x5C, 0x24, 0x08, 0x57, 0x48, 0x83, 0xEC, 0x20],
            target_module: target.map(str::to_owned),
        }
    }

    #[test]
    fn a_changed_function_start_is_a_hook_and_an_unchanged_one_is_not() {
        let snapshot = Snapshot {
            prologues: vec![
                prologue(
                    "C:\\Windows\\System32\\user32.dll",
                    "SetWindowsHookExW",
                    &[0xE9, 1, 2, 3, 4, 0x57],
                    None,
                ),
                prologue(
                    "kernel32.dll",
                    "CreateFileW",
                    &[0x48, 0x89, 0x5C, 0x24, 0x08, 0x57, 0x48, 0x83, 0xEC, 0x20],
                    None,
                ),
            ],
            ..Snapshot::default()
        };
        let report = analyze_default(&snapshot);
        assert_eq!(report.findings.len(), 1);
        let f = &report.findings[0];
        assert_eq!(f.kind, ScanKind::InlineHook);
        assert_eq!(
            f.subject, "user32.dll!SetWindowsHookExW",
            "the folder is not kept"
        );
        assert!(f.detail.contains("a relative jump"));
        assert_eq!(f.kind.code(), Some(WatchdogCode::HookDetected));
        assert_eq!(report.prologues_examined, 2);
    }

    #[test]
    fn sul_s_own_hooks_are_expected_and_an_allowed_target_is_allowed() {
        let expect = Expectations {
            own_hooks: vec![("USER32.dll".into(), "SetWindowsHookExW".into())],
            ..Expectations::default()
        };
        let mut allow = Allowlist::new();
        allow.allow("av_hook.dll", "my antivirus", 1);
        let snapshot = Snapshot {
            prologues: vec![
                prologue("user32.dll", "SetWindowsHookExW", &[0xE9, 1, 2, 3, 4], None),
                // Same module, another function: not SUL's.
                prologue("user32.dll", "GetAsyncKeyState", &[0xE9, 1, 2, 3, 4], None),
                prologue(
                    "ntdll.dll",
                    "NtOpenFile",
                    &[0xE9, 1, 2, 3, 4],
                    Some("AV_HOOK.dll"),
                ),
            ],
            ..Snapshot::default()
        };
        let report = analyze(&snapshot, &expect, &allow);
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].subject, "user32.dll!GetAsyncKeyState");
        assert_eq!(report.allowed.len(), 1);
        assert_eq!(report.allowed[0].subject, "ntdll.dll!NtOpenFile");
    }

    // ---- hollowing ------------------------------------------------------------------------------------------------

    fn section(changed: u64, total: u64) -> SectionDiff {
        SectionDiff {
            module: "C:\\x\\target.dll".into(),
            section: ".text".into(),
            total_bytes: total,
            changed_bytes: changed,
        }
    }

    #[test]
    fn a_section_changed_in_a_quarter_of_its_bytes_is_hollowed_and_less_is_patched() {
        let kind = |changed, total| {
            let snapshot = Snapshot {
                sections: vec![section(changed, total)],
                ..Snapshot::default()
            };
            analyze_default(&snapshot).findings.first().map(|f| f.kind)
        };
        assert_eq!(kind(0, 100), None, "no difference");
        assert_eq!(kind(5, 0), None, "an empty section");
        assert_eq!(kind(1, 100), Some(ScanKind::InlineHook));
        assert_eq!(kind(24, 100), Some(ScanKind::InlineHook));
        assert_eq!(kind(25, 100), Some(ScanKind::ImageHollowing));
        assert_eq!(kind(100, 100), Some(ScanKind::ImageHollowing));
        // More changed bytes than the section has is clamped, not a panic or an overflow.
        assert_eq!(kind(u64::MAX, 100), Some(ScanKind::ImageHollowing));
        assert_eq!(kind(u64::MAX, u64::MAX), Some(ScanKind::ImageHollowing));
        assert_eq!(ScanKind::ImageHollowing.code(), None);
    }

    #[test]
    fn a_section_finding_names_the_module_without_its_folder() {
        let snapshot = Snapshot {
            sections: vec![section(100, 100)],
            ..Snapshot::default()
        };
        let report = analyze_default(&snapshot);
        assert_eq!(report.findings[0].subject, "target.dll:.text");
    }

    // ---- allowlist ------------------------------------------------------------------------------------------------

    #[test]
    fn the_allowlist_matches_whole_file_names_only_and_ignores_case_and_folders() {
        let mut allow = Allowlist::new();
        allow.allow("D:\\Tools\\Overlay.DLL", "overlay", 1);
        allow.allow("overlay.dll", "again", 2);
        allow.allow("", "empty", 3);
        assert_eq!(
            allow.entries().len(),
            1,
            "a repeat and an empty name are not added"
        );
        assert!(allow.matches("overlay.dll").is_some());
        assert!(allow.matches("C:/x/OVERLAY.dll").is_some());
        assert!(allow.matches("my_overlay.dll").is_none());
        assert!(allow.matches("overlay.dll.exe").is_none());
        assert!(allow.remove("OVERLAY.DLL"));
        assert!(!allow.remove("overlay.dll"));
        assert!(allow.entries().is_empty());
    }

    // ---- whole analysis -------------------------------------------------------------------------------------------

    #[test]
    fn findings_come_out_in_a_fixed_order_whatever_the_snapshot_order() {
        let regions = vec![
            region(
                0x9000,
                0x1000,
                Protection::READ_WRITE_EXECUTE,
                RegionKind::Private,
                None,
            ),
            region(
                0x1000,
                0x1000,
                Protection::READ_WRITE_EXECUTE,
                RegionKind::Private,
                None,
            ),
        ];
        let mut reversed = regions.clone();
        reversed.reverse();
        let a = analyze_default(&Snapshot {
            regions,
            ..Snapshot::default()
        });
        let b = analyze_default(&Snapshot {
            regions: reversed,
            ..Snapshot::default()
        });
        assert_eq!(a, b);
        assert_eq!(a.findings[0].subject, "0x1000 (4096 bytes)");
    }

    #[test]
    fn an_empty_snapshot_is_clean() {
        let report = analyze_default(&Snapshot::default());
        assert!(report.is_clean() && report.allowed.is_empty());
    }

    struct Fixed(Option<Snapshot>);
    impl ProcessProbe for Fixed {
        fn snapshot(&mut self) -> Result<Snapshot> {
            self.0
                .take()
                .ok_or(sul_core::Error::Unsupported("no process to look at"))
        }
    }

    #[test]
    fn scan_asks_the_probe_and_passes_its_failure_on() {
        let mut probe = Fixed(Some(Snapshot {
            regions: vec![region(
                0x1000,
                0x1000,
                Protection::READ_WRITE_EXECUTE,
                RegionKind::Private,
                None,
            )],
            ..Snapshot::default()
        }));
        let report = scan(&mut probe, &Expectations::default(), &Allowlist::new()).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert!(scan(&mut probe, &Expectations::default(), &Allowlist::new()).is_err());
    }
}
