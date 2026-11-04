pub mod cache;
pub mod parts;
pub mod sevenzip;
pub mod zip;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::{AppError, AppResult};
use crate::util::Cancel;

/// Largest single entry we will hold in memory for a preview. Guards against zip bombs.
pub const MAX_PREVIEW_BYTES: u64 = 500 * 1024 * 1024;
/// Largest nested zip we will open in memory during indexing.
pub const MAX_NESTED_ZIP_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Entry {
    /// Sanitised, `/` separated. `None` when the entry path is unsafe.
    pub path: Option<String>,
    pub size: u64,
    pub is_dir: bool,
    pub encrypted: bool,
    pub is_link: bool,
    /// Seconds since 1970 as stored in the archive (usually local time), 0 when unknown.
    pub mtime: i64,
}

/// Seconds since 1970 for a calendar date and time, treated as UTC.
pub fn civil_to_unix(y: i64, mo: i64, d: i64, h: i64, mi: i64, s: i64) -> i64 {
    // Howard Hinnant's days-from-civil.
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    days * 86_400 + h * 3600 + mi * 60 + s
}

/// 7-Zip's `Modified = 2021-06-06 12:34:56.1234567`.
pub fn parse_7z_time(s: &str) -> i64 {
    let nums: Vec<i64> = s
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .take(6)
        .filter_map(|p| p.parse().ok())
        .collect();
    match nums.as_slice() {
        [y, mo, d, h, mi, sec, ..] if *y >= 1980 => civil_to_unix(*y, *mo, *d, *h, *mi, *sec),
        _ => 0,
    }
}

#[derive(Debug, Clone, Default)]
pub struct Listing {
    pub entries: Vec<Entry>,
    pub solid: bool,
    pub encrypted: bool,
}

pub type ProgressFn<'a> = &'a mut dyn FnMut(f32);

pub trait ArchiveReader: Send {
    /// Headers only. Never decompresses file data.
    fn list(&self) -> AppResult<Listing>;
    /// Decompress one entry into memory, giving up past `limit` bytes.
    fn read_entry(&self, entry: &str, limit: u64, cancel: &Cancel) -> AppResult<Vec<u8>>;
    /// Extract the given (sanitised) entries under `dest`, keeping their archive paths.
    /// Returns the entries that were written and where.
    fn extract_entries(
        &self,
        entries: &[String],
        dest: &Path,
        progress: ProgressFn<'_>,
        cancel: &Cancel,
    ) -> AppResult<Vec<(String, PathBuf)>>;
}

/// What the rest of the app needs to know to open a container's archive.
#[derive(Debug, Clone)]
pub struct ArchiveSource {
    pub kind: String,
    /// Entry point on disk (first part for split sets, `.zip` for spanned zips).
    pub path: PathBuf,
    pub multi_part: bool,
    /// Set for a zip that lives inside another zip.
    pub nested: Option<(PathBuf, String)>,
}

pub fn open_reader(
    src: &ArchiveSource,
    sz: &Arc<sevenzip::SevenZip>,
) -> AppResult<Box<dyn ArchiveReader>> {
    if let Some((outer, entry)) = &src.nested {
        return Ok(Box::new(zip::ZipReader::nested(
            outer.clone(),
            entry.clone(),
        )));
    }
    if src.kind == "zip" && !src.multi_part {
        return Ok(Box::new(zip::ZipReader::file(src.path.clone())));
    }
    if !sz.available() {
        return Err(AppError::Archive(crate::error::ArchiveFailure::ToolMissing));
    }
    Ok(Box::new(sevenzip::SevenZipReader::new(
        sz.clone(),
        src.path.clone(),
        src.multi_part,
    )))
}

/// Normalise an archive entry path, or reject it. Absolute paths, drive prefixes, `..` and
/// NUL bytes are refused outright rather than "fixed", so nothing can land outside `dest`.
pub fn sanitize_entry_path(raw: &str) -> Option<String> {
    if raw.contains('\0') {
        return None;
    }
    let p = raw.replace('\\', "/");
    if p.starts_with('/') {
        return None;
    }
    let bytes = p.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' {
        return None;
    }
    let mut parts = Vec::new();
    for comp in p.split('/') {
        match comp {
            "" | "." => continue,
            ".." => return None,
            c if c.contains(':') => return None,
            c => parts.push(c),
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("/"))
}

/// Join a sanitised entry path onto `dest`, double checking the result stays inside.
pub fn safe_join(dest: &Path, rel: &str) -> Option<PathBuf> {
    let clean = sanitize_entry_path(rel)?;
    let mut out = dest.to_path_buf();
    for part in clean.split('/') {
        out.push(part);
    }
    if out.starts_with(dest) {
        Some(out)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_times() {
        assert_eq!(civil_to_unix(1970, 1, 1, 0, 0, 0), 0);
        assert_eq!(parse_7z_time("2021-06-06 12:34:56.1234567"), 1_622_982_896);
        assert_eq!(parse_7z_time(""), 0);
    }

    #[test]
    fn sanitises_normal_paths() {
        assert_eq!(sanitize_entry_path("a/b.stl").as_deref(), Some("a/b.stl"));
        assert_eq!(
            sanitize_entry_path("a\\b\\c.stl").as_deref(),
            Some("a/b/c.stl")
        );
        assert_eq!(
            sanitize_entry_path("./a//b.stl").as_deref(),
            Some("a/b.stl")
        );
        assert_eq!(sanitize_entry_path("dir/").as_deref(), Some("dir"));
        assert_eq!(
            sanitize_entry_path("über/drache.stl").as_deref(),
            Some("über/drache.stl")
        );
    }

    #[test]
    fn rejects_traversal_and_absolute() {
        for bad in [
            "../evil.stl",
            "a/../../evil.stl",
            "a\\..\\evil.stl",
            "/etc/passwd",
            "\\windows\\x.stl",
            "C:\\x.stl",
            "c:/x.stl",
            "a/b:stream",
            "",
            "./",
            "a\0b",
        ] {
            assert_eq!(sanitize_entry_path(bad), None, "{bad:?} should be rejected");
        }
    }

    #[test]
    fn safe_join_stays_inside() {
        let dest = Path::new("/cache/x");
        assert_eq!(
            safe_join(dest, "a/b.stl"),
            Some(PathBuf::from("/cache/x/a/b.stl"))
        );
        assert_eq!(safe_join(dest, "../b.stl"), None);
    }
}
