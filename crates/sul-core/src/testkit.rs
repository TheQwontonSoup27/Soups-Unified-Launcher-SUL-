//! Shared test fixtures: a library on a temporary disk with a made-up game installed in a protected
//! "original" folder, two library roots, a journal and a build registry. Tests only.

#[cfg(test)]
pub(crate) use kit::*;

#[cfg(test)]
mod kit {
    use std::path::{Path, PathBuf};

    use crate::builds::{BuildRegistry, Ctx, ImportRequest};
    use crate::fsops::ProtectedRoots;
    use crate::journal::Journal;
    use crate::manifest::Variant;
    use crate::roots::fake::FakeDisk;
    use crate::roots::{LibraryRoots, RootKind};

    pub const GAME: &str = "madeupgame";

    pub struct World {
        pub _dir: tempfile::TempDir,
        pub base: PathBuf,
        pub protected: ProtectedRoots,
        pub journal: Journal,
        pub roots: LibraryRoots,
        pub disk: FakeDisk,
        pub reg_file: PathBuf,
        /// The "original install": protected, never written.
        pub install: PathBuf,
    }

    pub fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    impl World {
        pub fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let base = dir.path().canonicalize().unwrap();
            for d in ["ssd", "hdd"] {
                std::fs::create_dir_all(base.join(d)).unwrap();
            }
            let install = base.join("original_install");
            put(&install.join("game.exe"), "exe");
            put(&install.join("madeup.txt"), "1.0.0\n");
            put(&install.join("data/a.bin"), "AAAA");
            put(&install.join("data/b.bin"), "BBBB");
            put(&install.join("data/anticheat/guard.dll"), "GUARD");
            put(&install.join("mods/readme.txt"), "put mods here");

            let mut protected = ProtectedRoots::new();
            protected.add(&install).unwrap();
            let mut roots = LibraryRoots::load(&base.join("state/roots.json")).unwrap();
            for id in ["ssd", "hdd"] {
                roots
                    .add(&protected, id, id, &base.join(id), RootKind::Builds)
                    .unwrap();
            }
            Self {
                journal: Journal::open(&base.join("state/journal.jsonl")).unwrap(),
                reg_file: base.join("state/builds.json"),
                disk: FakeDisk::with(u64::MAX / 2),
                _dir: dir,
                base,
                protected,
                roots,
                install,
            }
        }

        pub fn ctx(&self) -> Ctx<'_> {
            Ctx {
                roots: &self.roots,
                protected: &self.protected,
                journal: &self.journal,
                disk: &self.disk,
            }
        }

        pub fn registry(&self) -> BuildRegistry {
            BuildRegistry::open(&self.reg_file).unwrap()
        }

        pub fn import(&self, reg: &mut BuildRegistry, variant: Variant) -> String {
            reg.import_install(
                &self.ctx(),
                &ImportRequest {
                    game_id: GAME.into(),
                    display_name: "Made-up Game".into(),
                    variant,
                    base_version: "1.0.0".into(),
                    source_description: "my own install".into(),
                    source_manifest_id: None,
                    src: self.install.clone(),
                    root_id: "ssd".into(),
                },
            )
            .unwrap()
        }
    }
}
