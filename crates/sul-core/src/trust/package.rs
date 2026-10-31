//! Package scanning: look at what a mod package contains before anything is unpacked, and say what looks wrong.
//!
//! It scans a folder, or a ZIP file's central directory (the list of names and sizes at the end of the file). It never
//! decompresses or runs anything, and never writes. The rules are about names, structure and file types:
//!
//! * **Confirmed** findings are conclusive about the name or structure, so unpacking the package as it is would be wrong
//!   or impossible: a path that climbs out of its folder, an absolute path, a name Windows cannot keep (reserved device
//!   names, `:` streams, trailing dots), two entries that collide, a symbolic link, control characters.
//! * **Suspicious** findings are signals that can be false alarms: program and script files (many legitimate mods ship a
//!   DLL), a program disguised by a double extension, nested archives, encrypted entries, a very high compression ratio.
//!
//! What it does not do: detect malware, check signatures or provenance, or look inside files. For a ZIP it reads the central
//! directory only (as most tools do) and does not compare it with the local headers; the sizes are the ones the archive
//! declares, which an archive can lie about, so a real unpacker must enforce its own limits while it writes. ZIP64 and
//! multi-disk archives are refused rather than guessed at.

use super::{Certainty, Finding, NOT_A_GUARANTEE, ProgramKind, extension, program_kind};
use crate::{Error, Result, error::io};
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// One item in a package, as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageEntry {
    /// The path exactly as the package stores it (either slash style).
    pub path: String,
    pub size: u64,
    /// The compressed size, where the format has one.
    pub compressed_size: Option<u64>,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub encrypted: bool,
}

impl PackageEntry {
    pub fn file(path: &str, size: u64) -> Self {
        Self {
            path: path.to_owned(),
            size,
            compressed_size: None,
            is_dir: false,
            is_symlink: false,
            encrypted: false,
        }
    }
}

/// Thresholds. The defaults are conservative guesses, not measured values.
#[derive(Debug, Clone)]
pub struct Limits {
    /// Refuse (an error, not a finding) a package with more entries than this.
    pub max_entries: usize,
    /// Refuse a ZIP whose central directory is larger than this many bytes.
    pub max_central_directory: u64,
    /// More entries than this is a signal.
    pub many_entries: usize,
    /// A total declared size above this is a signal.
    pub huge_total: u64,
    /// A file whose declared size is at least this many times its compressed size is a signal ...
    pub ratio: u64,
    /// ... when it is also at least this big.
    pub ratio_min_size: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: 500_000,
            max_central_directory: 128 * 1024 * 1024,
            many_entries: 20_000,
            huge_total: 8 * 1024 * 1024 * 1024,
            ratio: 100,
            ratio_min_size: 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageScan {
    pub entries: usize,
    /// Sum of the declared (or on-disk) sizes of the files.
    pub total_size: u64,
    pub confirmed: Vec<Finding>,
    pub suspicious: Vec<Finding>,
}

impl PackageScan {
    pub fn has_findings(&self) -> bool {
        !self.confirmed.is_empty() || !self.suspicious.is_empty()
    }

    /// The sentence to show. With no findings it carries the disclaimer; it never says "safe".
    pub fn statement(&self) -> String {
        if !self.has_findings() {
            return format!("No findings. {NOT_A_GUARANTEE}");
        }
        format!(
            "{} confirmed and {} suspicious. A confirmed finding is conclusive about a name or structure; a suspicious one needs a look and can be a false alarm.",
            self.confirmed.len(),
            self.suspicious.len()
        )
    }
}

// ---- the rules (pure) ----

const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9", "CONIN$",
    "CONOUT$",
];

/// Extensions of files people expect to be inert, used to spot `report.pdf.exe`.
const DOCUMENT_LIKE: &[&str] = &[
    "txt", "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "rtf", "csv", "jpg", "jpeg", "png",
    "gif", "bmp", "mp3", "mp4", "avi", "wav", "ogg", "json", "xml", "ini", "cfg", "dds", "zip",
    "rar", "7z",
];

const ARCHIVES: &[&str] = &["zip", "rar", "7z", "cab", "tar", "gz", "tgz", "bz2", "xz"];

fn is_bidi_or_format_control(c: char) -> bool {
    matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}' | '\u{061C}')
}

fn has_drive_prefix(path: &str) -> bool {
    let b = path.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// Apply the rules to a list of entries. Pure: nothing is read from disk.
pub fn scan_entries(entries: &[PackageEntry], limits: &Limits) -> PackageScan {
    let mut confirmed: Vec<Finding> = vec![];
    let mut suspicious: Vec<Finding> = vec![];
    let mut total_size = 0u64;

    for e in entries {
        let path = e.path.as_str();
        if !e.is_dir {
            total_size = total_size.saturating_add(e.size);
        }
        if path.is_empty() {
            confirmed.push(Finding::confirmed(
                "empty_name",
                path,
                "An entry has no name.",
            ));
            continue;
        }
        if path
            .chars()
            .any(|c| c.is_control() || is_bidi_or_format_control(c))
        {
            confirmed.push(Finding::confirmed(
                "control_character",
                path,
                "The name contains a control or text-direction character, which can make a name look like something it is not.",
            ));
        }
        if path.starts_with('/') || path.starts_with('\\') || has_drive_prefix(path) {
            confirmed.push(Finding::confirmed(
                "absolute_path",
                path,
                "The path is absolute: unpacking it would write outside the target folder.",
            ));
        }
        let norm = path.replace('\\', "/");
        let components: Vec<&str> = norm
            .split('/')
            .filter(|c| !c.is_empty() && *c != ".")
            .collect();
        if components.contains(&"..") {
            confirmed.push(Finding::confirmed(
                "path_traversal",
                path,
                "The path climbs out of its folder with \"..\".",
            ));
        }
        for (i, comp) in components.iter().enumerate() {
            if *comp == ".." {
                continue;
            }
            let stem = comp
                .split('.')
                .next()
                .unwrap_or("")
                .trim_end_matches(' ')
                .to_ascii_uppercase();
            if RESERVED.contains(&stem.as_str()) {
                confirmed.push(Finding::confirmed("reserved_name", path, format!("\"{comp}\" is a reserved Windows device name and cannot be a file or folder name.")));
            }
            if comp.ends_with('.') || comp.ends_with(' ') {
                confirmed.push(Finding::confirmed(
                    "trailing_dot_or_space",
                    path,
                    format!("\"{comp}\" ends in a dot or space, which Windows silently removes."),
                ));
            }
            if comp
                .chars()
                .any(|c| matches!(c, '<' | '>' | '"' | '|' | '?' | '*'))
            {
                confirmed.push(Finding::confirmed(
                    "invalid_character",
                    path,
                    format!("\"{comp}\" contains a character Windows does not allow in names."),
                ));
            }
            if comp.contains(':') && !(i == 0 && has_drive_prefix(comp)) {
                confirmed.push(Finding::confirmed(
                    "alternate_data_stream",
                    path,
                    format!(
                        "\"{comp}\" contains \":\", which Windows reads as a hidden data stream."
                    ),
                ));
            }
        }
        if e.is_symlink {
            confirmed.push(Finding::confirmed("symlink_entry", path, "The entry is a symbolic link, which can point outside the folder it is unpacked into. SUL does not follow or create them."));
        }
        if e.is_dir {
            continue;
        }
        match program_kind(path) {
            Some(ProgramKind::NativeCode) => suspicious.push(Finding::suspicious("native_code", path, "A program or library file. Many legitimate mods ship one; check where the package came from.")),
            Some(ProgramKind::Script) => suspicious.push(Finding::suspicious("script_file", path, "A script or registry file that runs or changes settings when opened.")),
            Some(ProgramKind::Shortcut) => suspicious.push(Finding::suspicious("shortcut_file", path, "A shortcut, which can point to any program.")),
            Some(ProgramKind::Java) => suspicious.push(Finding::suspicious("java_archive", path, "A Java archive, which runs code when launched.")),
            None => {}
        }
        if let Some(name) = components.last() {
            let parts: Vec<&str> = name.split('.').collect();
            if parts.len() >= 3 && program_kind(name).is_some() {
                let previous = parts[parts.len() - 2].to_ascii_lowercase();
                if DOCUMENT_LIKE.contains(&previous.as_str()) {
                    suspicious.push(Finding::suspicious(
                        "double_extension",
                        path,
                        format!(
                            "\"{name}\" is a program that is named like a \".{previous}\" file."
                        ),
                    ));
                }
            }
        }
        if extension(path).is_some_and(|x| ARCHIVES.contains(&x.as_str())) {
            suspicious.push(Finding::suspicious(
                "nested_archive",
                path,
                "An archive inside the package. SUL does not look inside it.",
            ));
        }
        if e.encrypted {
            suspicious.push(Finding::suspicious(
                "encrypted_entry",
                path,
                "The entry is encrypted, so its contents cannot be inspected.",
            ));
        }
        if let Some(c) = e.compressed_size
            && e.size >= limits.ratio_min_size
            && e.size / c.max(1) >= limits.ratio
        {
            suspicious.push(Finding::suspicious(
                "high_compression_ratio",
                path,
                format!("It claims to unpack to {} bytes from {} compressed, which is how decompression bombs look. The sizes are the archive's own claim.", e.size, c),
            ));
        }
    }

    // Collisions: entries that would end up as the same file once unpacked on Windows.
    let mut groups: BTreeMap<String, Vec<&PackageEntry>> = BTreeMap::new();
    for e in entries.iter().filter(|e| !e.path.is_empty()) {
        let key = e
            .path
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_lowercase();
        groups.entry(key).or_default().push(e);
    }
    for group in groups.values().filter(|g| g.len() > 1) {
        let first = group[0];
        let exact = group.iter().all(|e| {
            e.path.replace('\\', "/").trim_end_matches('/')
                == first.path.replace('\\', "/").trim_end_matches('/')
        });
        if exact {
            confirmed.push(Finding::confirmed("duplicate_entry", &first.path, format!("The path appears {} times; which copy wins depends on the tool that unpacks it.", group.len())));
        } else {
            let names: Vec<&str> = group.iter().map(|e| e.path.as_str()).collect();
            confirmed.push(Finding::confirmed(
                "case_collision",
                &first.path,
                format!(
                    "These names differ only in letter case and are the same file on Windows: {}.",
                    names.join(", ")
                ),
            ));
        }
    }

    if entries.len() > limits.many_entries {
        suspicious.push(Finding::suspicious(
            "many_entries",
            "",
            format!("The package has {} entries.", entries.len()),
        ));
    }
    if total_size > limits.huge_total {
        suspicious.push(Finding::suspicious(
            "very_large",
            "",
            format!("The package declares {total_size} bytes in total."),
        ));
    }
    debug_assert!(
        confirmed
            .iter()
            .all(|f| f.certainty == Certainty::Confirmed)
    );
    debug_assert!(
        suspicious
            .iter()
            .all(|f| f.certainty == Certainty::Suspicious)
    );
    PackageScan {
        entries: entries.len(),
        total_size,
        confirmed,
        suspicious,
    }
}

// ---- a folder ----

/// Scan an unpacked package folder. Reads names and sizes only; symbolic links are reported and never followed.
pub fn scan_directory(root: &Path, limits: &Limits) -> Result<PackageScan> {
    let meta = std::fs::symlink_metadata(root).map_err(io(root))?;
    if !meta.is_dir() {
        return Err(Error::Invalid(format!(
            "{} is not a folder",
            root.display()
        )));
    }
    let mut entries = vec![];
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for item in std::fs::read_dir(&dir).map_err(io(&dir))? {
            let item = item.map_err(io(&dir))?;
            let path = item.path();
            let ft = item.file_type().map_err(io(&path))?;
            let rel = path
                .strip_prefix(root)
                .map_err(|_| Error::Invalid("entry outside the scanned folder".into()))?
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            if entries.len() >= limits.max_entries {
                return Err(Error::Invalid(format!(
                    "more than {} entries: refusing to scan",
                    limits.max_entries
                )));
            }
            if ft.is_symlink() {
                entries.push(PackageEntry {
                    path: rel,
                    size: 0,
                    compressed_size: None,
                    is_dir: false,
                    is_symlink: true,
                    encrypted: false,
                });
            } else if ft.is_dir() {
                entries.push(PackageEntry {
                    path: rel,
                    size: 0,
                    compressed_size: None,
                    is_dir: true,
                    is_symlink: false,
                    encrypted: false,
                });
                stack.push(path);
            } else {
                let size = item.metadata().map_err(io(&path))?.len();
                entries.push(PackageEntry::file(&rel, size));
            }
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(scan_entries(&entries, limits))
}

// ---- a ZIP file ----

fn bad(msg: &str) -> Error {
    Error::Invalid(format!("not a readable ZIP archive: {msg}"))
}

/// Bounds-checked reader: every read that would run past the end is an error, never a panic.
struct Bytes<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Bytes<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.buf.len())
            .ok_or_else(|| bad("it is cut short"))?;
        let out = &self.buf[self.pos..end];
        self.pos = end;
        Ok(out)
    }
    fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

const EOCD_SIG: [u8; 4] = *b"PK\x05\x06";
const ZIP64_LOCATOR_SIG: [u8; 4] = *b"PK\x06\x07";
const CENTRAL_SIG: u32 = 0x0201_4b50;

/// The sizes in a ZIP64 extra field (id 1), which replace the 32-bit fields that hold 0xFFFFFFFF.
fn zip64_sizes(
    extra: &[u8],
    need_uncompressed: bool,
    need_compressed: bool,
) -> Result<(Option<u64>, Option<u64>)> {
    let mut r = Bytes { buf: extra, pos: 0 };
    while r.pos < extra.len() {
        let id = r.u16()?;
        let len = usize::from(r.u16()?);
        let body = r.take(len)?;
        if id == 1 {
            let mut b = Bytes { buf: body, pos: 0 };
            let read8 = |b: &mut Bytes| -> Result<u64> {
                let s = b.take(8)?;
                Ok(u64::from_le_bytes([
                    s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
                ]))
            };
            let u = if need_uncompressed {
                Some(read8(&mut b)?)
            } else {
                None
            };
            let c = if need_compressed {
                Some(read8(&mut b)?)
            } else {
                None
            };
            return Ok((u, c));
        }
    }
    Err(bad(
        "a size is marked as ZIP64 but the extra field is missing",
    ))
}

/// Scan a ZIP file's central directory. Reads the end of the file and the central directory only; nothing is decompressed.
pub fn scan_zip<R: Read + Seek>(source: &mut R, limits: &Limits) -> Result<PackageScan> {
    let len = source.seek(SeekFrom::End(0)).map_err(io("<zip>"))?;
    if len < 22 {
        return Err(bad("it is too short"));
    }
    let tail_len = len.min(22 + 65_535);
    source
        .seek(SeekFrom::Start(len - tail_len))
        .map_err(io("<zip>"))?;
    let mut tail = vec![0u8; usize::try_from(tail_len).map_err(|_| bad("it is too large"))?];
    source.read_exact(&mut tail).map_err(io("<zip>"))?;

    // The end-of-archive record: a signature whose comment length lands exactly on the end of the file. Trailing bytes after
    // it, or a second such record (one planted inside the comment), are refused, because tools disagree about which counts.
    let mut candidates = (0..=tail.len() - 22).rev().filter(|&pos| {
        tail[pos..pos + 4] == EOCD_SIG
            && pos + 22 + usize::from(u16::from_le_bytes([tail[pos + 20], tail[pos + 21]]))
                == tail.len()
    });
    let pos = candidates
        .next()
        .ok_or_else(|| bad("no end-of-archive record at the end of the file"))?;
    if candidates.next().is_some() {
        return Err(bad(
            "it has more than one end-of-archive record, and tools disagree about which one counts",
        ));
    }
    let eocd_file_pos = len - tail_len + pos as u64;

    let mut r = Bytes {
        buf: &tail[pos + 4..pos + 22],
        pos: 0,
    };
    let (disk, cd_disk, _on_disk, total, cd_size, cd_offset) =
        (r.u16()?, r.u16()?, r.u16()?, r.u16()?, r.u32()?, r.u32()?);
    if disk != 0 || cd_disk != 0 {
        return Err(Error::Unsupported(
            "multi-disk ZIP archives are refused, not guessed at",
        ));
    }
    let zip64_locator = pos >= 20 && tail[pos - 20..pos - 16] == ZIP64_LOCATOR_SIG;
    if zip64_locator && (total == 0xFFFF || cd_size == u32::MAX || cd_offset == u32::MAX) {
        return Err(Error::Unsupported(
            "ZIP64 archives are refused, not guessed at",
        ));
    }
    let (total, cd_size, cd_offset) =
        (usize::from(total), u64::from(cd_size), u64::from(cd_offset));
    if total > limits.max_entries {
        return Err(Error::Invalid(format!(
            "more than {} entries: refusing to scan",
            limits.max_entries
        )));
    }
    if cd_size > limits.max_central_directory {
        return Err(bad("its central directory is larger than the limit"));
    }
    if cd_offset
        .checked_add(cd_size)
        .is_none_or(|end| end > eocd_file_pos)
    {
        return Err(bad("the central directory is outside the file"));
    }
    if total as u64 * 46 > cd_size {
        return Err(bad(
            "it claims more entries than its central directory can hold",
        ));
    }
    source
        .seek(SeekFrom::Start(cd_offset))
        .map_err(io("<zip>"))?;
    let mut cd = vec![0u8; usize::try_from(cd_size).map_err(|_| bad("it is too large"))?];
    source.read_exact(&mut cd).map_err(io("<zip>"))?;

    let mut b = Bytes { buf: &cd, pos: 0 };
    let mut entries = Vec::with_capacity(total);
    for _ in 0..total {
        if b.u32()? != CENTRAL_SIG {
            return Err(bad("an entry in the central directory is damaged"));
        }
        let version_made_by = b.u16()?;
        let _needed = b.u16()?;
        let flags = b.u16()?;
        let _method = b.u16()?;
        let _time = b.u32()?;
        let _crc = b.u32()?;
        let comp32 = b.u32()?;
        let size32 = b.u32()?;
        let name_len = usize::from(b.u16()?);
        let extra_len = usize::from(b.u16()?);
        let comment_len = usize::from(b.u16()?);
        let _disk = b.u16()?;
        let _internal = b.u16()?;
        let external = b.u32()?;
        let _offset = b.u32()?;
        let name = String::from_utf8_lossy(b.take(name_len)?).into_owned();
        let extra = b.take(extra_len)?;
        b.take(comment_len)?;

        let (mut size, mut comp) = (u64::from(size32), u64::from(comp32));
        if size32 == u32::MAX || comp32 == u32::MAX {
            let (u, c) = zip64_sizes(extra, size32 == u32::MAX, comp32 == u32::MAX)?;
            size = u.unwrap_or(size);
            comp = c.unwrap_or(comp);
        }
        // High byte of "version made by" is the host system: 3 = Unix, where the top half of the external attributes is the file mode.
        let unix_mode = if version_made_by >> 8 == 3 {
            external >> 16
        } else {
            0
        };
        let is_symlink = unix_mode & 0o170000 == 0o120000;
        let is_dir = name.ends_with('/')
            || name.ends_with('\\')
            || external & 0x10 != 0
            || unix_mode & 0o170000 == 0o040000;
        entries.push(PackageEntry {
            path: name,
            size,
            compressed_size: Some(comp),
            is_dir,
            is_symlink,
            encrypted: flags & 1 != 0,
        });
    }
    if b.pos != cd.len() {
        return Err(bad("its central directory is longer than its entries"));
    }
    Ok(scan_entries(&entries, limits))
}

/// Open `path` and scan it as a ZIP file.
pub fn scan_zip_file(path: &Path, limits: &Limits) -> Result<PackageScan> {
    let mut f = std::fs::File::open(path).map_err(io(path))?;
    scan_zip(&mut f, limits)
}

/// Scan a folder or a ZIP file, whichever `path` is.
pub fn scan_path(path: &Path, limits: &Limits) -> Result<PackageScan> {
    if std::fs::symlink_metadata(path).map_err(io(path))?.is_dir() {
        scan_directory(path, limits)
    } else {
        scan_zip_file(path, limits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn scan(paths: &[&str]) -> PackageScan {
        let entries: Vec<_> = paths.iter().map(|p| PackageEntry::file(p, 10)).collect();
        scan_entries(&entries, &Limits::default())
    }
    fn codes(f: &[Finding]) -> Vec<&'static str> {
        f.iter().map(|x| x.code).collect()
    }
    fn only(paths: &[&str]) -> (Vec<&'static str>, Vec<&'static str>) {
        let s = scan(paths);
        (codes(&s.confirmed), codes(&s.suspicious))
    }

    // ---- the rules ----

    #[test]
    fn an_ordinary_package_has_no_findings_and_the_statement_is_not_a_promise() {
        let s = scan(&[
            "readme.txt",
            "data/model.fbx",
            "data/tex/a.dds",
            "config.json",
        ]);
        assert!(!s.has_findings());
        assert_eq!(s.entries, 4);
        assert_eq!(s.total_size, 40);
        let text = s.statement();
        assert!(text.starts_with("No findings."));
        assert!(text.contains("not a guarantee"));
        assert!(!text.to_lowercase().contains("is safe"));
    }

    #[test]
    fn paths_that_leave_the_target_folder_are_confirmed() {
        for p in [
            "../evil.txt",
            "a/../../evil.txt",
            "a\\..\\..\\evil",
            "a/b/../../../c",
        ] {
            assert_eq!(only(&[p]).0, ["path_traversal"], "{p}");
        }
        for p in [
            "/etc/passwd",
            "\\windows\\system32\\x.dll",
            "C:\\Windows\\x.txt",
            "c:/x",
            "\\\\server\\share\\x",
        ] {
            assert!(only(&[p]).0.contains(&"absolute_path"), "{p}");
        }
        assert!(
            only(&["a/./b.txt", "sub/../x"]).0.len() <= 1,
            "'.' alone is harmless, and an inner '..' that stays inside is still reported once"
        );
    }

    #[test]
    fn a_dot_dot_that_stays_inside_is_still_reported_because_tools_differ() {
        assert_eq!(only(&["a/../b.txt"]).0, ["path_traversal"]);
    }

    #[test]
    fn names_windows_cannot_keep_are_confirmed() {
        assert_eq!(only(&["CON"]).0, ["reserved_name"]);
        assert_eq!(only(&["data/nul.txt"]).0, ["reserved_name"]);
        assert_eq!(
            only(&["com1.log", "LPT9"]).0,
            ["reserved_name", "reserved_name"]
        );
        assert_eq!(
            only(&["AUX .txt"]).0,
            ["reserved_name"],
            "trailing spaces before the dot do not help"
        );
        assert!(
            only(&["console.txt", "connect.cfg", "com10.txt", "aux2"])
                .0
                .is_empty(),
            "near misses are fine"
        );
        assert_eq!(only(&["a/b."]).0, ["trailing_dot_or_space"]);
        assert_eq!(only(&["a /b"]).0, ["trailing_dot_or_space"]);
        assert_eq!(only(&["a<b.txt"]).0, ["invalid_character"]);
        for c in ['>', '"', '|', '?', '*'] {
            assert_eq!(only(&[&format!("x{c}y")]).0, ["invalid_character"], "{c}");
        }
        assert_eq!(only(&["file.txt:hidden"]).0, ["alternate_data_stream"]);
        assert_eq!(only(&["a/b:c/d"]).0, ["alternate_data_stream"]);
        assert_eq!(
            only(&["C:\\x"]).0,
            ["absolute_path"],
            "a drive prefix is one finding, not also a data stream"
        );
    }

    #[test]
    fn control_and_text_direction_characters_are_confirmed() {
        assert_eq!(only(&["a\u{0}b"]).0, ["control_character"]);
        assert_eq!(only(&["a\nb"]).0, ["control_character"]);
        assert_eq!(
            only(&["photo\u{202E}gpj.exe"]).0,
            ["control_character"],
            "right-to-left override"
        );
        assert!(
            only(&["naïve — 日本語 ✓.txt"]).0.is_empty(),
            "ordinary non-ASCII names are fine"
        );
    }

    #[test]
    fn an_empty_name_and_a_symbolic_link_are_confirmed() {
        assert_eq!(only(&[""]).0, ["empty_name"]);
        let link = PackageEntry {
            path: "link".into(),
            size: 0,
            compressed_size: None,
            is_dir: false,
            is_symlink: true,
            encrypted: false,
        };
        assert_eq!(
            codes(&scan_entries(&[link], &Limits::default()).confirmed),
            ["symlink_entry"]
        );
    }

    #[test]
    fn duplicates_and_case_collisions_are_confirmed_once_per_group() {
        let s = scan(&["a.txt", "b.txt", "a.txt", "a.txt"]);
        assert_eq!(codes(&s.confirmed), ["duplicate_entry"]);
        assert!(s.confirmed[0].detail.contains("3 times"));
        let s = scan(&["Data/A.bin", "data/a.bin"]);
        assert_eq!(codes(&s.confirmed), ["case_collision"]);
        assert!(
            s.confirmed[0].detail.contains("Data/A.bin")
                && s.confirmed[0].detail.contains("data/a.bin")
        );
        assert_eq!(
            codes(&scan(&["a/b", "a\\b"]).confirmed),
            ["duplicate_entry"],
            "both slash styles are the same path"
        );
        assert!(scan(&["a.txt", "b.txt", "dir/a.txt"]).confirmed.is_empty());
    }

    #[test]
    fn program_files_are_signals_never_confirmed_and_say_they_can_be_legitimate() {
        let (c, s) = only(&[
            "mod.dll",
            "run.exe",
            "setup.bat",
            "x.ps1",
            "go.lnk",
            "tool.jar",
            "data.bin",
        ]);
        assert!(c.is_empty(), "a program file is never a confirmed finding");
        assert_eq!(
            s,
            [
                "native_code",
                "native_code",
                "script_file",
                "script_file",
                "shortcut_file",
                "java_archive"
            ]
        );
        assert!(
            scan(&["mod.dll"]).suspicious[0]
                .detail
                .contains("legitimate")
        );
    }

    #[test]
    fn a_program_that_imitates_a_document_is_a_signal() {
        let s = scan(&[
            "report.pdf.exe",
            "holiday.jpg.scr",
            "notes.txt.bat",
            "a.b.exe",
            "a.tar.gz",
        ]);
        assert_eq!(
            s.suspicious
                .iter()
                .filter(|f| f.code == "double_extension")
                .map(|f| f.path.as_str())
                .collect::<Vec<_>>(),
            ["report.pdf.exe", "holiday.jpg.scr", "notes.txt.bat"]
        );
        assert!(s.confirmed.is_empty());
    }

    #[test]
    fn nested_archives_and_encrypted_entries_are_signals() {
        assert_eq!(
            only(&["inner.zip", "x/y.7z"]).1,
            ["nested_archive", "nested_archive"]
        );
        let enc = PackageEntry {
            encrypted: true,
            ..PackageEntry::file("secret.dat", 5)
        };
        assert_eq!(
            codes(&scan_entries(&[enc], &Limits::default()).suspicious),
            ["encrypted_entry"]
        );
    }

    #[test]
    fn a_decompression_bomb_shape_is_a_signal_and_a_big_ordinary_file_is_not() {
        let l = Limits::default();
        let bomb = PackageEntry {
            compressed_size: Some(1000),
            ..PackageEntry::file("zeros.bin", 10 * 1024 * 1024 * 1024)
        };
        assert_eq!(
            codes(&scan_entries(&[bomb], &l).suspicious),
            ["high_compression_ratio", "very_large"]
        );
        let ok = PackageEntry {
            compressed_size: Some(300 * 1024 * 1024),
            ..PackageEntry::file("video.bin", 400 * 1024 * 1024)
        };
        assert!(!scan_entries(&[ok], &l).has_findings());
        let small = PackageEntry {
            compressed_size: Some(1),
            ..PackageEntry::file("tiny.txt", 50_000)
        };
        assert!(
            !scan_entries(&[small], &l).has_findings(),
            "a tiny file that compresses well is not a bomb"
        );
        let zero_comp = PackageEntry {
            compressed_size: Some(0),
            ..PackageEntry::file("x.bin", 5 * 1024 * 1024)
        };
        assert_eq!(
            codes(&scan_entries(&[zero_comp], &l).suspicious),
            ["high_compression_ratio"],
            "zero compressed bytes must not divide by zero"
        );
    }

    #[test]
    fn the_compression_ratio_and_minimum_size_limits_are_exact() {
        let l = Limits::default();
        let mib = 1024 * 1024;
        let at = |size: u64, comp: u64| {
            codes(
                &scan_entries(
                    &[PackageEntry {
                        compressed_size: Some(comp),
                        ..PackageEntry::file("data.bin", size)
                    }],
                    &l,
                )
                .suspicious,
            )
        };
        // Ratio: exactly the limit is flagged, one byte of compressed size more is not.
        assert_eq!(at(100 * mib, mib), ["high_compression_ratio"]);
        assert!(at(100 * mib, mib + 1).is_empty());
        // Minimum size: exactly the minimum is checked, one byte under is not.
        assert_eq!(at(mib, 1), ["high_compression_ratio"]);
        assert!(at(mib - 1, 1).is_empty());
    }

    #[test]
    fn very_many_entries_is_a_signal_and_far_too_many_is_refused_by_the_zip_reader() {
        let l = Limits {
            many_entries: 3,
            ..Limits::default()
        };
        let entries: Vec<_> = (0..4)
            .map(|i| PackageEntry::file(&format!("f{i}"), 1))
            .collect();
        let s = scan_entries(&entries, &l);
        assert_eq!(codes(&s.suspicious), ["many_entries"]);
        assert_eq!(s.suspicious[0].path, "");
    }

    #[test]
    fn directories_are_checked_for_names_but_not_for_program_types() {
        let d = PackageEntry {
            is_dir: true,
            ..PackageEntry::file("tools.exe", 0)
        };
        assert!(!scan_entries(&[d], &Limits::default()).has_findings());
        let bad_dir = PackageEntry {
            is_dir: true,
            ..PackageEntry::file("../x/", 0)
        };
        assert_eq!(
            codes(&scan_entries(&[bad_dir], &Limits::default()).confirmed),
            ["path_traversal"]
        );
    }

    #[test]
    fn confirmed_and_suspicious_are_never_mixed_up() {
        let s = scan(&["../a.dll", "CON", "b.exe", "c.pdf.exe", "d.zip"]);
        assert!(
            s.confirmed
                .iter()
                .all(|f| f.certainty == Certainty::Confirmed)
        );
        assert!(
            s.suspicious
                .iter()
                .all(|f| f.certainty == Certainty::Suspicious)
        );
        assert!(s.statement().starts_with("2 confirmed and "));
    }

    // ---- a folder ----

    #[test]
    fn a_folder_is_scanned_by_name_and_size_and_never_changed() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("pkg");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("readme.txt"), b"hello").unwrap();
        std::fs::write(root.join("sub/plugin.dll"), b"MZ").unwrap();
        std::fs::write(root.join("sub/fake.pdf.exe"), b"MZ").unwrap();
        let s = scan_directory(&root, &Limits::default()).unwrap();
        assert_eq!(
            s.entries, 4,
            "readme.txt, the folder sub, and the two files in it"
        );
        assert_eq!(s.total_size, 5 + 2 + 2, "the folder adds no size");
        assert!(s.confirmed.is_empty());
        // Entries are judged in path order: sub/fake.pdf.exe (a program, and one that imitates a document), then sub/plugin.dll.
        assert_eq!(
            codes(&s.suspicious),
            ["native_code", "double_extension", "native_code"]
        );
        assert_eq!(std::fs::read(root.join("readme.txt")).unwrap(), b"hello");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
    }

    #[test]
    fn scanning_something_that_is_not_a_folder_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("file.txt");
        std::fs::write(&f, b"x").unwrap();
        assert!(matches!(
            scan_directory(&f, &Limits::default()),
            Err(Error::Invalid(_))
        ));
        assert!(scan_directory(&tmp.path().join("missing"), &Limits::default()).is_err());
    }

    #[test]
    fn a_folder_with_too_many_entries_is_refused_not_scanned_halfway() {
        let tmp = tempfile::tempdir().unwrap();
        for i in 0..5 {
            std::fs::write(tmp.path().join(format!("f{i}")), b"x").unwrap();
        }
        let l = Limits {
            max_entries: 3,
            ..Limits::default()
        };
        assert!(
            matches!(scan_directory(tmp.path(), &l), Err(Error::Invalid(m)) if m.contains("refusing"))
        );
    }

    /// Runs on Windows only (CI): it needs a real folder junction.
    #[cfg(windows)]
    #[test]
    fn a_link_in_a_folder_is_reported_and_not_followed() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("pkg");
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.dll"), b"MZ").unwrap();
        super::super::make_junction(&root.join("link"), &outside);
        let s = scan_directory(&root, &Limits::default()).unwrap();
        assert_eq!(s.entries, 1, "the link's target was not entered");
        assert_eq!(codes(&s.confirmed), ["symlink_entry"]);
        assert!(s.suspicious.is_empty());
    }

    // ---- a ZIP file ----

    /// Builds ZIP files by hand, so tests can make exactly the shapes (and the damage) they need.
    #[derive(Default)]
    struct Zip {
        entries: Vec<(String, u32, u32, u16, u32, u16)>, // name, comp, size, flags, external attrs, version made by
        comment: Vec<u8>,
    }
    impl Zip {
        fn new() -> Self {
            Self::default()
        }
        fn file(mut self, name: &str, size: u32) -> Self {
            self.entries.push((name.into(), size, size, 0, 0, 20));
            self
        }
        fn raw(
            mut self,
            name: &str,
            comp: u32,
            size: u32,
            flags: u16,
            external: u32,
            made_by: u16,
        ) -> Self {
            self.entries
                .push((name.into(), comp, size, flags, external, made_by));
            self
        }
        fn build(&self) -> Vec<u8> {
            let mut out = vec![];
            let mut cd = vec![];
            for (name, comp, size, flags, external, made_by) in &self.entries {
                let offset = out.len() as u32;
                out.extend_from_slice(&0x0403_4b50u32.to_le_bytes()); // local header (not read by the scanner)
                out.extend_from_slice(&[
                    20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                ]);
                out.extend_from_slice(&(name.len() as u16).to_le_bytes());
                out.extend_from_slice(&0u16.to_le_bytes());
                out.extend_from_slice(name.as_bytes());
                cd.extend_from_slice(&CENTRAL_SIG.to_le_bytes());
                cd.extend_from_slice(&made_by.to_le_bytes());
                cd.extend_from_slice(&20u16.to_le_bytes());
                cd.extend_from_slice(&flags.to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes()); // stored
                cd.extend_from_slice(&0u32.to_le_bytes()); // time
                cd.extend_from_slice(&0u32.to_le_bytes()); // crc
                cd.extend_from_slice(&comp.to_le_bytes());
                cd.extend_from_slice(&size.to_le_bytes());
                cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes()); // extra
                cd.extend_from_slice(&0u16.to_le_bytes()); // comment
                cd.extend_from_slice(&0u16.to_le_bytes()); // disk
                cd.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
                cd.extend_from_slice(&external.to_le_bytes());
                cd.extend_from_slice(&offset.to_le_bytes());
                cd.extend_from_slice(name.as_bytes());
            }
            let cd_offset = out.len() as u32;
            out.extend_from_slice(&cd);
            out.extend_from_slice(&EOCD_SIG);
            out.extend_from_slice(&[0, 0, 0, 0]);
            out.extend_from_slice(&(self.entries.len() as u16).to_le_bytes());
            out.extend_from_slice(&(self.entries.len() as u16).to_le_bytes());
            out.extend_from_slice(&(cd.len() as u32).to_le_bytes());
            out.extend_from_slice(&cd_offset.to_le_bytes());
            out.extend_from_slice(&(self.comment.len() as u16).to_le_bytes());
            out.extend_from_slice(&self.comment);
            out
        }
    }

    fn scan_bytes(bytes: &[u8]) -> Result<PackageScan> {
        scan_zip(&mut Cursor::new(bytes), &Limits::default())
    }

    #[test]
    fn a_zip_is_scanned_from_its_central_directory() {
        let bytes = Zip::new()
            .file("readme.txt", 5)
            .file("plugins/mod.dll", 700)
            .file("../up.txt", 1)
            .build();
        let s = scan_bytes(&bytes).unwrap();
        assert_eq!(s.entries, 3);
        assert_eq!(s.total_size, 706);
        assert_eq!(codes(&s.confirmed), ["path_traversal"]);
        assert_eq!(codes(&s.suspicious), ["native_code"]);
    }

    #[test]
    fn an_empty_zip_and_a_zip_with_a_comment_are_read() {
        assert_eq!(scan_bytes(&Zip::new().build()).unwrap().entries, 0);
        let mut z = Zip::new().file("a.txt", 1);
        z.comment = b"made by a tool".to_vec();
        assert_eq!(scan_bytes(&z.build()).unwrap().entries, 1);
        let mut tricky = Zip::new().file("a.txt", 1);
        tricky.comment = EOCD_SIG.iter().copied().chain([0u8; 18]).collect(); // a fake record inside the comment
        let err = scan_bytes(&tricky.build()).unwrap_err().to_string();
        assert!(
            err.contains("more than one end-of-archive record"),
            "a record planted in the comment makes the file ambiguous: {err}"
        );
    }

    #[test]
    fn symbolic_links_directories_and_encryption_are_read_from_the_attributes() {
        let unix = 3u16 << 8 | 20;
        let bytes = Zip::new()
            .raw("link", 5, 5, 0, 0o120777 << 16, unix)
            .raw("dir/", 0, 0, 0, 0, 20)
            .raw("dosdir", 0, 0, 0, 0x10, 20)
            .raw("secret.dat", 5, 5, 1, 0, 20)
            .raw("plain.txt", 5, 5, 0, 0o100644 << 16, unix)
            .build();
        let s = scan_bytes(&bytes).unwrap();
        assert_eq!(codes(&s.confirmed), ["symlink_entry"]);
        assert_eq!(codes(&s.suspicious), ["encrypted_entry"]);
        assert_eq!(s.total_size, 5 + 5 + 5, "directories add no size");
    }

    #[test]
    fn declared_sizes_that_look_like_a_bomb_are_a_signal() {
        let bytes = Zip::new()
            .raw("zeros.bin", 100, 50 * 1024 * 1024, 0, 0, 20)
            .build();
        assert_eq!(
            codes(&scan_bytes(&bytes).unwrap().suspicious),
            ["high_compression_ratio"]
        );
    }

    #[test]
    fn zip64_and_multi_disk_archives_are_refused() {
        let mut z = Zip::new().file("a.txt", 1).build();
        // Mark the central directory offset as 0xFFFFFFFF and put a ZIP64 locator in front of the end record.
        let eocd = z.len() - 22;
        z[eocd + 16..eocd + 20].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut with_locator = z[..eocd].to_vec();
        with_locator.extend_from_slice(&ZIP64_LOCATOR_SIG);
        with_locator.extend_from_slice(&[0u8; 16]);
        with_locator.extend_from_slice(&z[eocd..]);
        assert!(
            matches!(scan_bytes(&with_locator), Err(Error::Unsupported(m)) if m.contains("ZIP64"))
        );
        let mut multi = Zip::new().file("a.txt", 1).build();
        let e = multi.len() - 22;
        multi[e + 4] = 1;
        assert!(
            matches!(scan_bytes(&multi), Err(Error::Unsupported(m)) if m.contains("multi-disk"))
        );
    }

    #[test]
    fn a_zip64_size_in_the_extra_field_is_read_and_a_missing_one_is_an_error() {
        // 6 GiB file: 32-bit fields hold 0xFFFFFFFF and the real sizes are in the ZIP64 extra field.
        let mut extra = vec![];
        extra.extend_from_slice(&1u16.to_le_bytes());
        extra.extend_from_slice(&16u16.to_le_bytes());
        extra.extend_from_slice(&(6u64 << 30).to_le_bytes());
        extra.extend_from_slice(&1000u64.to_le_bytes());
        assert_eq!(
            zip64_sizes(&extra, true, true).unwrap(),
            (Some(6u64 << 30), Some(1000))
        );
        assert_eq!(
            zip64_sizes(&extra, true, false).unwrap(),
            (Some(6u64 << 30), None)
        );
        assert!(zip64_sizes(&[], true, true).is_err());
        assert!(
            zip64_sizes(&[1, 0, 16, 0, 1, 2, 3], true, true).is_err(),
            "a field cut short is an error, not a guess"
        );
        assert!(
            zip64_sizes(&[9, 9, 200, 0], true, true).is_err(),
            "a length past the end is an error"
        );
    }

    #[test]
    fn things_that_are_not_zips_are_errors_with_a_reason() {
        for bytes in [
            &b""[..],
            b"PK",
            b"just some text that is long enough to have a tail ........",
            &[0u8; 100],
        ] {
            assert!(
                matches!(scan_bytes(bytes), Err(Error::Invalid(m)) if m.contains("not a readable ZIP")),
                "{} bytes",
                bytes.len()
            );
        }
        let mut trailing = Zip::new().file("a.txt", 1).build();
        trailing.extend_from_slice(b"extra bytes after the archive");
        assert!(
            scan_bytes(&trailing).is_err(),
            "bytes after the end record are refused, because tools disagree about them"
        );
    }

    #[test]
    fn a_central_directory_that_lies_about_itself_is_refused() {
        let good = Zip::new().file("a.txt", 1).file("b.txt", 2).build();
        let e = good.len() - 22;
        let patch = |at: usize, v: u32, width: usize| {
            let mut b = good.clone();
            b[e + at..e + at + width].copy_from_slice(&v.to_le_bytes()[..width]);
            b
        };
        assert!(
            scan_bytes(&patch(10, 3, 2)).is_err(),
            "claims one more entry than exists"
        );
        assert!(
            scan_bytes(&patch(10, 1, 2)).is_err(),
            "claims fewer entries than the directory holds"
        );
        assert!(
            scan_bytes(&patch(12, 5, 4)).is_err(),
            "claims a smaller directory than its entries"
        );
        assert!(
            scan_bytes(&patch(12, 4_000_000_000, 4)).is_err(),
            "claims a directory bigger than the file"
        );
        assert!(
            scan_bytes(&patch(16, 4_000_000_000, 4)).is_err(),
            "points past the end of the file"
        );
        assert!(
            scan_bytes(&patch(10, 65_000, 2)).is_err(),
            "claims more entries than fit"
        );
    }

    #[test]
    fn a_huge_directory_claim_is_refused_before_anything_is_allocated() {
        let good = Zip::new().file("a.txt", 1).build();
        let e = good.len() - 22;
        let mut b = good.clone();
        b[e + 12..e + 16].copy_from_slice(&u32::MAX.to_le_bytes());
        let limits = Limits {
            max_central_directory: 1024,
            ..Limits::default()
        };
        assert!(
            matches!(scan_zip(&mut Cursor::new(&b), &limits), Err(Error::Invalid(m)) if m.contains("larger than the limit"))
        );
        let many = Limits {
            max_entries: 0,
            ..Limits::default()
        };
        assert!(
            matches!(scan_zip(&mut Cursor::new(&good), &many), Err(Error::Invalid(m)) if m.contains("refusing"))
        );
    }

    #[test]
    fn no_truncation_and_no_single_damaged_byte_can_make_the_reader_panic() {
        let bytes = Zip::new()
            .file("readme.txt", 5)
            .file("dir/", 0)
            .file("plugins/mod.dll", 700)
            .raw("link", 1, 1, 0, 0o120777 << 16, 3 << 8)
            .build();
        for n in 0..=bytes.len() {
            let _ = scan_bytes(&bytes[..n]);
        }
        for i in 0..bytes.len() {
            for v in [0x00u8, 0xFF, 0x7F, 0x80, 0x50] {
                let mut b = bytes.clone();
                b[i] = v;
                let _ = scan_bytes(&b);
            }
            for bit in 0..8 {
                let mut b = bytes.clone();
                b[i] ^= 1 << bit;
                let _ = scan_bytes(&b);
            }
        }
    }

    #[test]
    fn random_damage_to_many_shapes_never_panics_and_never_allocates_more_than_the_limits_allow() {
        let base = Zip::new()
            .file("a.txt", 3)
            .file("b/c.dll", 9)
            .file("d/", 0)
            .build();
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let limits = Limits {
            max_central_directory: 4096,
            max_entries: 64,
            ..Limits::default()
        };
        for _ in 0..5000 {
            let mut b = base.clone();
            for _ in 0..(next() % 6 + 1) {
                let i = (next() % b.len() as u64) as usize;
                b[i] = (next() & 0xFF) as u8;
            }
            if next() % 4 == 0 {
                let cut = (next() % b.len() as u64) as usize;
                b.truncate(cut);
            }
            let _ = scan_zip(&mut Cursor::new(&b), &limits);
        }
    }

    #[test]
    fn a_zip_file_and_a_folder_can_be_scanned_by_path() {
        let tmp = tempfile::tempdir().unwrap();
        let zip = tmp.path().join("mod.zip");
        std::fs::write(&zip, Zip::new().file("a.txt", 1).file("x.exe", 2).build()).unwrap();
        assert_eq!(scan_path(&zip, &Limits::default()).unwrap().entries, 2);
        assert_eq!(
            scan_zip_file(&zip, &Limits::default())
                .unwrap()
                .suspicious
                .len(),
            1
        );
        let dir = tmp.path().join("folder");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), b"x").unwrap();
        assert_eq!(scan_path(&dir, &Limits::default()).unwrap().entries, 1);
        assert!(scan_path(&tmp.path().join("nothing.zip"), &Limits::default()).is_err());
        std::fs::write(
            tmp.path().join("notzip.zip"),
            b"hello world, not a zip file at all",
        )
        .unwrap();
        assert!(scan_path(&tmp.path().join("notzip.zip"), &Limits::default()).is_err());
        assert!(zip.exists(), "scanning leaves the file alone");
    }
}
