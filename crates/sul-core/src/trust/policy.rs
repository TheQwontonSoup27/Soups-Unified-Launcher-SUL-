//! When may SUL act on a file without asking? The owner's rule (2026-10-31), written as one pure function.
//!
//! * **Tier 1, SUL's own files** (`Tier::Internal`): a file whose content is on a *known-harmful* list may be moved
//!   to quarantine automatically. Anything else that is wrong with it is *unknown*, so a person decides.
//! * **Tier 2, a game's or anti-cheat vendor's files** (`Tier::External`, anything inside a protected original install):
//!   SUL never moves, edits or signs them, whatever they contain. The most it does is report and point at the vendor's
//!   own verification tool (for Steam games, "Verify integrity of game files").
//! * **Anywhere else** (`Tier::Unclassified`): not SUL's to move without asking, so never automatic.
//!
//! "Known harmful" is evidence, not a guess: the SHA-256 of the file's current content is on a list the caller supplies.
//! No such list exists in SUL yet, so [`KnownHarmful::empty`] is the only list there is today and **nothing is automatic**.
//! A change event, a hash that differs from the baseline, or a suspicious signal is *unknown*, not known-harmful: updates,
//! installers and antivirus all change files, and a changed file is not proof of anything (see [`super::Certainty`]).
//!
//! This module decides; it does not touch a file. The caller still goes through [`super::quarantine::Quarantine`], which
//! journals the move, keeps it reversible and refuses original installs on its own.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::Certainty;
use crate::error::io;
use crate::fsops::ProtectedRoots;
use crate::safepath::resolve_lenient;
use crate::{Error, Result};

/// Which verification model a file falls under (spec V5.1, section 12.7: the Two-Tier Verification Boundary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// SUL's own binaries, sidecars and configuration.
    Internal,
    /// A game, a game's anti-cheat or any other vendor's files: read-only to SUL.
    External,
    /// Neither: not decided by a root SUL knows about.
    Unclassified,
}

/// Decides the [`Tier`] of a path from two sets of folders. A path inside both is **External**: when in doubt, SUL does not touch it.
#[derive(Debug, Clone)]
pub struct Tiering {
    internal: Vec<PathBuf>,
    external: ProtectedRoots,
}

impl Tiering {
    /// `internal_roots` are folders that hold SUL's own files (they must exist); `external` is the set of protected original installs.
    pub fn new(internal_roots: &[&Path], external: ProtectedRoots) -> Result<Self> {
        let mut internal = Vec::with_capacity(internal_roots.len());
        for root in internal_roots {
            internal.push(root.canonicalize().map_err(io(root))?);
        }
        Ok(Self { internal, external })
    }

    /// The path need not exist. Symlinks and `..` are resolved first, so neither can be used to move a file into the wrong tier.
    pub fn classify(&self, path: &Path) -> Tier {
        if self.external.check_writable(path).is_err() {
            return Tier::External;
        }
        let resolved = resolve_lenient(path);
        if self.internal.iter().any(|root| resolved.starts_with(root)) {
            Tier::Internal
        } else {
            Tier::Unclassified
        }
    }
}

/// SHA-256 hashes (lowercase hex) of file contents known to be harmful. Empty by default.
#[derive(Debug, Default, Clone)]
pub struct KnownHarmful {
    hashes: HashSet<String>,
}

impl KnownHarmful {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Every entry must be exactly 64 hexadecimal characters; one bad entry rejects the whole list, so a truncated or
    /// mistyped list can never match something it should not.
    pub fn from_sha256_hex<I, S>(hashes: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut set = HashSet::new();
        for (i, h) in hashes.into_iter().enumerate() {
            let h = h.as_ref();
            if h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(Error::Invalid(format!(
                    "entry {} of the known-harmful list is not a 64-character hexadecimal SHA-256",
                    i + 1
                )));
            }
            set.insert(h.to_ascii_lowercase());
        }
        Ok(Self { hashes: set })
    }

    pub fn contains(&self, sha256_hex: &str) -> bool {
        self.hashes.contains(&sha256_hex.to_ascii_lowercase())
    }

    pub fn len(&self) -> usize {
        self.hashes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.hashes.is_empty()
    }
}

/// What a check saw about one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub tier: Tier,
    pub certainty: Certainty,
    /// SHA-256 (hex) of the file's *current* content, if it could be read. A file that is missing or unreadable cannot match a list.
    pub sha256: Option<String>,
}

/// What SUL may do about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Put it in the list of findings. Nothing is asked and nothing is changed.
    ReportOnly,
    /// Show it to the owner and wait. Nothing is moved or changed until they say so.
    AskOwner,
    /// SUL's own file, known-harmful content: it may be quarantined now (journaled and reversible, see [`super::quarantine`]).
    QuarantineAutomatically,
    /// A vendor's file: SUL does not touch it. Offer the vendor's own verification (for example Steam's "Verify integrity of game files").
    SuggestNativeVerification,
}

impl Action {
    /// True only for the one action that changes a file before a person has said yes.
    pub fn acts_without_asking(self) -> bool {
        self == Action::QuarantineAutomatically
    }
}

/// An [`Action`] and the sentence that explains it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Decision {
    pub action: Action,
    pub reason: &'static str,
}

/// The rule. See the module documentation. Only one combination returns [`Action::QuarantineAutomatically`]:
/// `Tier::Internal` **and** a content hash that is on the known-harmful list.
pub fn decide(obs: &Observation, known: &KnownHarmful) -> Decision {
    let is_known = obs.sha256.as_deref().is_some_and(|h| known.contains(h));
    match obs.tier {
        Tier::External => match obs.certainty {
            Certainty::Confirmed => Decision {
                action: Action::SuggestNativeVerification,
                reason: "This belongs to a game or its vendor. SUL never moves or edits it; use the vendor's own file verification.",
            },
            Certainty::Suspicious => Decision {
                action: Action::ReportOnly,
                reason: "A signal about a vendor's file. SUL only reports it and never touches the file.",
            },
        },
        Tier::Internal if is_known => Decision {
            action: Action::QuarantineAutomatically,
            reason: "SUL's own file, and its content is on the known-harmful list.",
        },
        Tier::Internal | Tier::Unclassified => match (obs.certainty, is_known) {
            (_, true) => Decision {
                action: Action::AskOwner,
                reason: "Known-harmful content, but the file is not one of SUL's own, so it is not moved without asking.",
            },
            (Certainty::Confirmed, false) => Decision {
                action: Action::AskOwner,
                reason: "The file is not as expected, and nothing says it is harmful. A person decides.",
            },
            (Certainty::Suspicious, false) => Decision {
                action: Action::ReportOnly,
                reason: "A signal that needs a person to look. It may be a false alarm.",
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::hash_file;
    use crate::trust::quarantine::Quarantine;

    const BAD: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const OTHER: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

    fn obs(tier: Tier, certainty: Certainty, sha: Option<&str>) -> Observation {
        Observation {
            tier,
            certainty,
            sha256: sha.map(str::to_owned),
        }
    }

    fn list() -> KnownHarmful {
        KnownHarmful::from_sha256_hex([BAD]).unwrap()
    }

    #[test]
    fn known_harmful_content_in_sul_own_file_is_the_only_automatic_case() {
        for tier in [Tier::Internal, Tier::External, Tier::Unclassified] {
            for certainty in [Certainty::Confirmed, Certainty::Suspicious] {
                for sha in [None, Some(BAD), Some(OTHER)] {
                    for known in [KnownHarmful::empty(), list()] {
                        let d = decide(&obs(tier, certainty, sha), &known);
                        let expected =
                            tier == Tier::Internal && sha == Some(BAD) && !known.is_empty();
                        assert_eq!(
                            d.action.acts_without_asking(),
                            expected,
                            "{tier:?} {certainty:?} {sha:?} list={}",
                            known.len()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn with_no_list_nothing_is_ever_automatic() {
        let empty = KnownHarmful::empty();
        for tier in [Tier::Internal, Tier::External, Tier::Unclassified] {
            for certainty in [Certainty::Confirmed, Certainty::Suspicious] {
                for sha in [None, Some(BAD)] {
                    assert!(
                        !decide(&obs(tier, certainty, sha), &empty)
                            .action
                            .acts_without_asking()
                    );
                }
            }
        }
    }

    #[test]
    fn unknown_alterations_to_sul_own_files_are_manual() {
        let known = list();
        let confirmed = decide(
            &obs(Tier::Internal, Certainty::Confirmed, Some(OTHER)),
            &known,
        );
        assert_eq!(
            confirmed.action,
            Action::AskOwner,
            "a changed file nobody has called harmful: a person decides"
        );
        let unreadable = decide(&obs(Tier::Internal, Certainty::Confirmed, None), &known);
        assert_eq!(
            unreadable.action,
            Action::AskOwner,
            "a file that cannot be read cannot match a list"
        );
        let signal = decide(
            &obs(Tier::Internal, Certainty::Suspicious, Some(OTHER)),
            &known,
        );
        assert_eq!(
            signal.action,
            Action::ReportOnly,
            "a signal is reported, not acted on"
        );
    }

    #[test]
    fn a_vendor_file_is_never_touched_even_when_its_content_is_on_the_list() {
        let known = list();
        let d = decide(
            &obs(Tier::External, Certainty::Confirmed, Some(BAD)),
            &known,
        );
        assert_eq!(d.action, Action::SuggestNativeVerification);
        let s = decide(
            &obs(Tier::External, Certainty::Suspicious, Some(BAD)),
            &known,
        );
        assert_eq!(s.action, Action::ReportOnly);
    }

    #[test]
    fn a_file_outside_both_areas_is_asked_about_never_moved() {
        let known = list();
        let hit = decide(
            &obs(Tier::Unclassified, Certainty::Suspicious, Some(BAD)),
            &known,
        );
        assert_eq!(
            hit.action,
            Action::AskOwner,
            "known-harmful, but not SUL's file"
        );
        let miss = decide(
            &obs(Tier::Unclassified, Certainty::Confirmed, Some(OTHER)),
            &known,
        );
        assert_eq!(miss.action, Action::AskOwner);
    }

    #[test]
    fn the_list_accepts_only_exact_sha256_and_ignores_case() {
        assert!(
            KnownHarmful::from_sha256_hex(Vec::<&str>::new())
                .unwrap()
                .is_empty()
        );
        let upper = KnownHarmful::from_sha256_hex([BAD.to_ascii_uppercase()]).unwrap();
        assert!(upper.contains(BAD) && upper.contains(&BAD.to_ascii_uppercase()));
        assert!(!upper.contains(OTHER));
        for bad in [
            "",
            "abc",
            &BAD[..63],
            &format!("{BAD}0"),
            &format!("{}g", &BAD[..63]),
            &format!(" {}", &BAD[..63]),
        ] {
            let err = KnownHarmful::from_sha256_hex([BAD, bad])
                .unwrap_err()
                .to_string();
            assert!(err.contains("entry 2"), "{bad:?}: {err}");
            assert!(
                !err.contains(bad) || bad.is_empty(),
                "the message must not echo the entry"
            );
        }
    }

    #[test]
    fn tiers_follow_the_folders_and_the_external_one_wins() {
        let tmp = tempfile::tempdir().unwrap();
        let own = tmp.path().join("sul");
        let game = tmp.path().join("game");
        let nested = game.join("sul-inside-the-game");
        for d in [&own, &game, &nested] {
            std::fs::create_dir_all(d).unwrap();
        }
        let mut external = ProtectedRoots::new();
        external.add(&game).unwrap();
        let tiering = Tiering::new(&[own.as_path(), nested.as_path()], external).unwrap();

        assert_eq!(
            tiering.classify(&own.join("sul.exe")),
            Tier::Internal,
            "a file that does not exist yet is still classified"
        );
        assert_eq!(
            tiering.classify(&game.join("helldivers2.exe")),
            Tier::External
        );
        assert_eq!(
            tiering.classify(&nested.join("sul-helper.exe")),
            Tier::External,
            "a folder that is both SUL's and the game's counts as the game's"
        );
        assert_eq!(
            tiering.classify(&tmp.path().join("elsewhere.dll")),
            Tier::Unclassified
        );
        assert_eq!(
            tiering.classify(&own.join("..").join("game").join("a.dll")),
            Tier::External,
            "`..` cannot be used to look like SUL's own file"
        );
        assert_eq!(
            tiering.classify(&own.join("..").join("elsewhere.dll")),
            Tier::Unclassified
        );
        // A sibling that merely starts with the same letters is not inside the folder.
        assert_eq!(
            tiering.classify(&tmp.path().join("sul-other").join("a.dll")),
            Tier::Unclassified
        );
        assert!(
            Tiering::new(
                &[tmp.path().join("missing").as_path()],
                ProtectedRoots::new()
            )
            .is_err()
        );
    }

    #[test]
    fn the_rule_and_the_quarantine_agree_end_to_end() {
        let tmp = tempfile::tempdir().unwrap();
        let own = tmp.path().join("sul");
        let game = tmp.path().join("game");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::create_dir_all(&game).unwrap();
        let mut external = ProtectedRoots::new();
        external.add(&game).unwrap();
        let tiering = Tiering::new(&[own.as_path()], external.clone()).unwrap();
        let q = Quarantine::open(&tmp.path().join("quarantine"), external).unwrap();

        let ours = own.join("sidecar.exe");
        let theirs = game.join("anticheat.dll");
        std::fs::write(&ours, b"tampered sidecar").unwrap();
        std::fs::write(&theirs, b"tampered anticheat").unwrap();
        let ours_hash = hash_file(&ours).unwrap().0;
        let theirs_hash = hash_file(&theirs).unwrap().0;
        let known = KnownHarmful::from_sha256_hex([&ours_hash, &theirs_hash]).unwrap();

        // SUL's own file with known-harmful content: automatic, and the quarantine can undo it.
        let d = decide(
            &obs(
                tiering.classify(&ours),
                Certainty::Confirmed,
                Some(&ours_hash),
            ),
            &known,
        );
        assert_eq!(d.action, Action::QuarantineAutomatically);
        let rec = q
            .quarantine_file(&ours, "known-harmful content (automatic)")
            .unwrap();
        assert!(!ours.exists());
        q.restore(&rec.id).unwrap();
        assert_eq!(std::fs::read(&ours).unwrap(), b"tampered sidecar");

        // The same list entry on a game's file: no automatic action, and the quarantine refuses it on its own as well.
        let d = decide(
            &obs(
                tiering.classify(&theirs),
                Certainty::Confirmed,
                Some(&theirs_hash),
            ),
            &known,
        );
        assert_eq!(d.action, Action::SuggestNativeVerification);
        assert!(q.quarantine_file(&theirs, "must be refused").is_err());
        assert_eq!(std::fs::read(&theirs).unwrap(), b"tampered anticheat");
    }

    #[test]
    fn every_decision_has_a_reason_that_never_says_safe() {
        let known = list();
        for tier in [Tier::Internal, Tier::External, Tier::Unclassified] {
            for certainty in [Certainty::Confirmed, Certainty::Suspicious] {
                for sha in [None, Some(BAD), Some(OTHER)] {
                    let r = decide(&obs(tier, certainty, sha), &known)
                        .reason
                        .to_ascii_lowercase();
                    assert!(r.len() > 20);
                    for word in ["safe", "clean", "trusted", "verified"] {
                        assert!(!r.contains(word), "{r}");
                    }
                }
            }
        }
    }
}
