//! Structured JSONL logging with secret redaction. Secrets must never reach logs.

use crate::{error::io, Result};
use serde_json::{json, Map, Value};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const SENSITIVE: &[&str] = &["password", "passwd", "token", "secret", "authorization", "api_key", "apikey", "cookie", "session"];

pub struct Logger {
    path: PathBuf,
    file: Mutex<std::fs::File>,
}

fn is_sensitive(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    SENSITIVE.iter().any(|s| k.contains(s))
}

pub fn redact(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut out = Map::new();
            for (k, val) in m {
                out.insert(k.clone(), if is_sensitive(k) { json!("[REDACTED]") } else { redact(val) });
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(redact).collect()),
        other => other.clone(),
    }
}

impl Logger {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).map_err(io(p))?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path).map_err(io(path))?;
        Ok(Self { path: path.to_path_buf(), file: Mutex::new(file) })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn log(&self, level: &str, event: &str, fields: Value) -> Result<()> {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let line = json!({ "ts_unix": ts, "level": level, "event": event, "fields": redact(&fields) });
        let mut f = self.file.lock().unwrap();
        writeln!(f, "{line}").map_err(io(&self.path))?;
        f.flush().map_err(io(&self.path))?;
        Ok(())
    }
}
