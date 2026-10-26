//! Recovery journal: records begin/commit for multi-step operations so an interrupted
//! operation (crash, power loss) can be detected and reported on next start.

use crate::{Result, error::io};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Record {
    Begin {
        id: String,
        op: String,
        detail: String,
    },
    Commit {
        id: String,
    },
    Abort {
        id: String,
        reason: String,
    },
}

pub struct Journal {
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub id: String,
    pub op: String,
    pub detail: String,
}

impl Journal {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).map_err(io(p))?;
        }
        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    fn append(&self, r: &Record) -> Result<()> {
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(io(&self.path))?;
        writeln!(f, "{}", serde_json::to_string(r)?).map_err(io(&self.path))?;
        f.sync_all().map_err(io(&self.path))?;
        Ok(())
    }

    pub fn begin(&self, id: &str, op: &str, detail: &str) -> Result<()> {
        self.append(&Record::Begin {
            id: id.into(),
            op: op.into(),
            detail: detail.into(),
        })
    }
    pub fn commit(&self, id: &str) -> Result<()> {
        self.append(&Record::Commit { id: id.into() })
    }
    pub fn abort(&self, id: &str, reason: &str) -> Result<()> {
        self.append(&Record::Abort {
            id: id.into(),
            reason: reason.into(),
        })
    }

    /// Operations that began but never committed or aborted. Unparseable lines (e.g. a torn
    /// final write) are ignored rather than treated as fatal.
    pub fn pending(&self) -> Result<Vec<Pending>> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => {
                return Err(crate::Error::Io {
                    path: self.path.clone(),
                    source: e,
                });
            }
        };
        let mut open: Vec<Pending> = vec![];
        for line in text.lines() {
            match serde_json::from_str::<Record>(line) {
                Ok(Record::Begin { id, op, detail }) => open.push(Pending { id, op, detail }),
                Ok(Record::Commit { id }) | Ok(Record::Abort { id, .. }) => {
                    open.retain(|p| p.id != id)
                }
                Err(_) => {}
            }
        }
        Ok(open)
    }
}
