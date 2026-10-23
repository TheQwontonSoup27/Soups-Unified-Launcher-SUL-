//! Read-only Steam library helpers. SUL never needs a Steam password: Steam itself stays logged in
//! and SUL asks it to launch a game with a `steam://` URL. Nothing here writes to Steam's folders.

use crate::{error::io, Error, Result};
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Vdf {
    Str(String),
    Block(Vec<(String, Vdf)>),
}

impl Vdf {
    pub fn get(&self, key: &str) -> Option<&Vdf> {
        match self {
            Vdf::Block(v) => v.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, x)| x),
            Vdf::Str(_) => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Vdf::Str(s) => Some(s),
            Vdf::Block(_) => None,
        }
    }
    pub fn entries(&self) -> &[(String, Vdf)] {
        match self {
            Vdf::Block(v) => v,
            Vdf::Str(_) => &[],
        }
    }
}

#[derive(Debug, PartialEq)]
enum Tok {
    Str(String),
    Open,
    Close,
}

fn tokenize(text: &str) -> Result<Vec<Tok>> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '{' {
            out.push(Tok::Open);
            i += 1;
        } else if c == '}' {
            out.push(Tok::Close);
            i += 1;
        } else if c == '"' {
            i += 1;
            let mut s = String::new();
            loop {
                match chars.get(i) {
                    None => return Err(Error::Invalid("unterminated string in VDF".into())),
                    Some('"') => {
                        i += 1;
                        break;
                    }
                    Some('\\') => {
                        match chars.get(i + 1) {
                            Some('\\') => s.push('\\'),
                            Some('"') => s.push('"'),
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some(o) => {
                                s.push('\\');
                                s.push(*o);
                            }
                            None => return Err(Error::Invalid("dangling escape in VDF".into())),
                        }
                        i += 2;
                    }
                    Some(ch) => {
                        s.push(*ch);
                        i += 1;
                    }
                }
            }
            out.push(Tok::Str(s));
        } else {
            let mut s = String::new();
            while i < chars.len() && !chars[i].is_whitespace() && !matches!(chars[i], '{' | '}' | '"') {
                s.push(chars[i]);
                i += 1;
            }
            out.push(Tok::Str(s));
        }
    }
    Ok(out)
}

fn parse_entries(toks: &[Tok], pos: &mut usize, depth: usize, top: bool) -> Result<Vec<(String, Vdf)>> {
    if depth > MAX_DEPTH {
        return Err(Error::Invalid("VDF nesting too deep".into()));
    }
    let mut out = Vec::new();
    loop {
        match toks.get(*pos) {
            None => {
                if top {
                    return Ok(out);
                }
                return Err(Error::Invalid("unexpected end of VDF (missing '}')".into()));
            }
            Some(Tok::Close) => {
                if top {
                    return Err(Error::Invalid("unexpected '}' in VDF".into()));
                }
                *pos += 1;
                return Ok(out);
            }
            Some(Tok::Open) => return Err(Error::Invalid("unexpected '{' in VDF".into())),
            Some(Tok::Str(k)) => {
                let key = k.clone();
                *pos += 1;
                match toks.get(*pos) {
                    Some(Tok::Str(v)) => {
                        out.push((key, Vdf::Str(v.clone())));
                        *pos += 1;
                    }
                    Some(Tok::Open) => {
                        *pos += 1;
                        let inner = parse_entries(toks, pos, depth + 1, false)?;
                        out.push((key, Vdf::Block(inner)));
                    }
                    _ => return Err(Error::Invalid(format!("key '{key}' has no value in VDF"))),
                }
            }
        }
    }
}

/// Parse Valve KeyValues text (libraryfolders.vdf, appmanifest_*.acf).
pub fn parse_vdf(text: &str) -> Result<Vdf> {
    let toks = tokenize(text)?;
    let mut pos = 0;
    Ok(Vdf::Block(parse_entries(&toks, &mut pos, 0, true)?))
}

/// Library folder paths listed in libraryfolders.vdf (the `steamapps` parent folders).
pub fn library_folders(text: &str) -> Result<Vec<PathBuf>> {
    let root = parse_vdf(text)?;
    let lf = root.get("libraryfolders").ok_or_else(|| Error::Invalid("no 'libraryfolders' block".into()))?;
    let mut out = Vec::new();
    for (_, v) in lf.entries() {
        if let Some(p) = v.get("path").and_then(Vdf::as_str) {
            out.push(PathBuf::from(p));
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppManifest {
    pub app_id: u32,
    pub name: String,
    pub install_dir: String,
    pub build_id: Option<String>,
    /// 0 = always update, 1 = only on launch, 2 = high priority. Informational only; SUL does not edit it.
    pub auto_update_behavior: Option<u32>,
}

pub fn parse_app_manifest(text: &str) -> Result<AppManifest> {
    let root = parse_vdf(text)?;
    let st = root.get("AppState").ok_or_else(|| Error::Invalid("no 'AppState' block".into()))?;
    let field = |k: &str| st.get(k).and_then(Vdf::as_str);
    let app_id = field("appid")
        .and_then(|s| s.trim().parse::<u32>().ok())
        .ok_or_else(|| Error::Invalid("missing or invalid appid".into()))?;
    let install_dir = field("installdir").ok_or_else(|| Error::Invalid("missing installdir".into()))?.to_string();
    // installdir is a folder name inside steamapps/common; reject anything path-like.
    crate::safepath::validate_relative(&install_dir)?;
    if install_dir.contains('/') || install_dir.contains('\\') {
        return Err(Error::UnsafePath(format!("installdir must be a single folder name: {install_dir}")));
    }
    Ok(AppManifest {
        app_id,
        name: field("name").unwrap_or("").to_string(),
        install_dir,
        build_id: field("buildid").map(str::to_string),
        auto_update_behavior: field("AutoUpdateBehavior").and_then(|s| s.trim().parse().ok()),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledGame {
    pub manifest: AppManifest,
    pub install_path: PathBuf,
}

/// Scan one `steamapps` folder (read-only). Unreadable or malformed manifests are reported, not fatal.
pub fn scan_steamapps(steamapps: &Path) -> Result<(Vec<InstalledGame>, Vec<String>)> {
    let mut games = Vec::new();
    let mut warnings = Vec::new();
    for entry in std::fs::read_dir(steamapps).map_err(io(steamapps))? {
        let entry = entry.map_err(io(steamapps))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
            continue;
        }
        let text = match std::fs::read_to_string(entry.path()) {
            Ok(t) => t,
            Err(e) => {
                warnings.push(format!("{name}: {e}"));
                continue;
            }
        };
        match parse_app_manifest(&text) {
            Ok(m) => {
                let install_path = steamapps.join("common").join(&m.install_dir);
                games.push(InstalledGame { manifest: m, install_path });
            }
            Err(e) => warnings.push(format!("{name}: {e}")),
        }
    }
    games.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    Ok((games, warnings))
}

/// URL that asks the logged-in Steam client to start a game. No credentials involved.
pub fn launch_url(app_id: u32) -> String {
    format!("steam://rungameid/{app_id}")
}

/// Likely Steam install folders on Windows (no registry access yet; user can also pick the folder).
pub fn default_steam_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Ok(base) = std::env::var(var) {
            v.push(PathBuf::from(base).join("Steam"));
        }
    }
    v.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
    v.dedup();
    v
}

/// True if `candidate` lies inside any Steam library's `steamapps` folder. A managed legacy build
/// must NOT live there: Steam updates and verifies games inside its libraries.
pub fn is_inside_steam_library(candidate: &Path, library_roots: &[PathBuf]) -> bool {
    let cand = candidate.canonicalize().unwrap_or_else(|_| candidate.to_path_buf());
    library_roots.iter().any(|root| {
        let steamapps = root.join("steamapps");
        let sa = steamapps.canonicalize().unwrap_or(steamapps);
        cand.starts_with(&sa)
    })
}
