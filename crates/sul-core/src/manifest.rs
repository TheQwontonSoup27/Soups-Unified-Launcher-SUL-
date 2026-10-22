//! Build manifests: inventory + SHA-256 of a build directory, and read-only verification.
//! Scanning and verifying never modify the directory.

use crate::{error::io, safepath, Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Variant {
    Original,
    Optimized,
    Legacy,
    Experimental,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Unverified,
    Ready,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Forward-slash relative path.
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildManifest {
    pub schema_version: u32,
    pub game_id: String,
    pub display_name: String,
    pub variant: Variant,
    pub base_version: String,
    /// Where the build came from (user-supplied description / manifest id). Free text, no secrets.
    pub source: String,
    pub files: Vec<FileEntry>,
    pub created_unix: u64,
    pub state: State,
    pub notes: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyReport {
    pub missing: Vec<String>,
    pub modified: Vec<String>,
    pub extra: Vec<String>,
}

impl VerifyReport {
    pub fn is_clean(&self) -> bool {
        self.missing.is_empty() && self.modified.is_empty() && self.extra.is_empty()
    }
}

pub fn hash_file(path: &Path) -> Result<(String, u64)> {
    let mut f = File::open(path).map_err(io(path))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut total = 0u64;
    loop {
        let n = f.read(&mut buf).map_err(io(path))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        total += n as u64;
    }
    Ok((hex::encode(h.finalize()), total))
}

/// Walk `root` and return sorted entries. Symlinks are rejected: managed builds must be plain
/// files so a build can't smuggle in content from elsewhere.
fn walk(root: &Path) -> Result<BTreeMap<String, std::path::PathBuf>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).map_err(io(&dir))? {
            let entry = entry.map_err(io(&dir))?;
            let p = entry.path();
            let ft = entry.file_type().map_err(io(&p))?;
            if ft.is_symlink() {
                return Err(Error::UnsafePath(format!("symlink in managed build: {}", p.display())));
            }
            if ft.is_dir() {
                stack.push(p);
            } else if ft.is_file() {
                let rel = p.strip_prefix(root).unwrap().components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>().join("/");
                out.insert(rel, p);
            }
        }
    }
    Ok(out)
}

impl BuildManifest {
    pub fn scan(root: &Path, game_id: &str, display_name: &str, variant: Variant, base_version: &str, source: &str) -> Result<Self> {
        let mut files = Vec::new();
        for (rel, p) in walk(root)? {
            let (sha256, size) = hash_file(&p)?;
            files.push(FileEntry { path: rel, sha256, size });
        }
        let created_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            game_id: game_id.into(),
            display_name: display_name.into(),
            variant,
            base_version: base_version.into(),
            source: source.into(),
            files,
            created_unix,
            state: State::Unverified,
            notes: String::new(),
        })
    }

    /// Compare a directory against this manifest. Read-only.
    pub fn verify(&self, root: &Path) -> Result<VerifyReport> {
        let mut report = VerifyReport::default();
        let on_disk = walk(root)?;
        for f in &self.files {
            safepath::validate_relative(&f.path)?; // manifests from outside are untrusted
            match on_disk.get(&f.path) {
                None => report.missing.push(f.path.clone()),
                Some(p) => {
                    let (h, size) = hash_file(p)?;
                    if h != f.sha256 || size != f.size {
                        report.modified.push(f.path.clone());
                    }
                }
            }
        }
        let known: std::collections::HashSet<&str> = self.files.iter().map(|f| f.path.as_str()).collect();
        for k in on_disk.keys() {
            if !known.contains(k.as_str()) {
                report.extra.push(k.clone());
            }
        }
        Ok(report)
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(s: &str) -> Result<Self> {
        let m: Self = serde_json::from_str(s)?;
        if m.schema_version > SCHEMA_VERSION {
            return Err(Error::Invalid(format!("manifest schema {} is newer than supported {}", m.schema_version, SCHEMA_VERSION)));
        }
        Ok(m)
    }
}
