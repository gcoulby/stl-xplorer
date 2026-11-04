use std::fs::Metadata;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn mtime_secs(meta: &Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// Lower-case extension without the dot.
pub fn ext_of(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

pub const MESH_EXTS: [&str; 3] = ["stl", "3mf", "obj"];

pub fn is_mesh_name(name: &str) -> bool {
    MESH_EXTS.contains(&ext_of(name).as_str())
}

/// Browser and download-manager temp files. `.part` alone is an in-progress download, while
/// `name.part1.rar` has the extension `rar`, so it is not caught here.
pub fn is_in_progress_download(name: &str) -> bool {
    matches!(
        ext_of(name).as_str(),
        "crdownload" | "part" | "partial" | "tmp" | "download" | "opdownload"
    )
}

pub fn stable_hash<T: Hash>(v: &T) -> String {
    // DefaultHasher is fine here: the value only names a cache folder, and a collision just
    // means one extra extraction.
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Insert ` (n)` after `stem_len` bytes of `name`.
pub fn numbered_name(name: &str, stem_len: usize, n: u32) -> String {
    let (stem, rest) = name.split_at(stem_len.min(name.len()));
    format!("{stem} ({n}){rest}")
}

/// Length of the part of a file name before its extension.
pub fn stem_len(name: &str) -> usize {
    match name.rfind('.') {
        Some(0) | None => name.len(),
        Some(i) => i,
    }
}

/// First free path in `dir` for `name`, appending ` (1)`, ` (2)` ... before the extension.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let sl = stem_len(name);
    (1..)
        .map(|n| dir.join(numbered_name(name, sl, n)))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

#[derive(Clone, Default, Debug)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
    pub fn check(&self) -> crate::error::AppResult<()> {
        if self.is_cancelled() {
            Err(crate::error::AppError::Cancelled)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbered_names() {
        assert_eq!(numbered_name("a.stl", stem_len("a.stl"), 2), "a (2).stl");
        assert_eq!(numbered_name("noext", stem_len("noext"), 1), "noext (1)");
        assert_eq!(
            numbered_name(".hidden", stem_len(".hidden"), 1),
            ".hidden (1)"
        );
    }

    #[test]
    fn download_temp_files() {
        assert!(is_in_progress_download("pack.zip.crdownload"));
        assert!(is_in_progress_download("pack.7z.part"));
        assert!(!is_in_progress_download("pack.part1.rar"));
        assert!(!is_in_progress_download("pack.part01.rar"));
    }

    #[test]
    fn unique_path_skips_existing() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.stl"), b"x").expect("write");
        std::fs::write(dir.path().join("a (1).stl"), b"x").expect("write");
        assert_eq!(
            unique_path(dir.path(), "a.stl"),
            dir.path().join("a (2).stl")
        );
        assert_eq!(unique_path(dir.path(), "b.stl"), dir.path().join("b.stl"));
    }
}
