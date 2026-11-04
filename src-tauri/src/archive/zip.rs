//! Native zip backend. The central directory gives random access, so one entry is cheap.

use std::fs::File;
use std::io::{BufReader, Cursor, Read, Seek, Write};
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use super::{safe_join, sanitize_entry_path, ArchiveReader, Entry, Listing, ProgressFn};
use crate::error::{AppError, AppResult, ArchiveFailure};
use crate::util::Cancel;

enum Source {
    File(PathBuf),
    /// A zip stored as an entry of another zip, read into memory when opened.
    Nested {
        outer: PathBuf,
        entry: String,
    },
}

pub struct ZipReader {
    source: Source,
}

trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}

impl ZipReader {
    pub fn file(path: PathBuf) -> Self {
        Self {
            source: Source::File(path),
        }
    }

    pub fn nested(outer: PathBuf, entry: String) -> Self {
        Self {
            source: Source::Nested { outer, entry },
        }
    }

    fn open(&self, cancel: &Cancel) -> AppResult<ZipArchive<Box<dyn ReadSeek>>> {
        let reader: Box<dyn ReadSeek> = match &self.source {
            Source::File(p) => Box::new(BufReader::new(File::open(p)?)),
            Source::Nested { outer, entry } => {
                let outer = ZipReader::file(outer.clone());
                let bytes = outer.read_entry(entry, super::MAX_NESTED_ZIP_BYTES, cancel)?;
                Box::new(Cursor::new(bytes))
            }
        };
        Ok(ZipArchive::new(reader)?)
    }
}

/// Copy at most `limit` bytes, failing if there is more. Checks for cancellation between chunks.
fn copy_limited<R: Read, W: Write>(
    r: &mut R,
    w: &mut W,
    limit: u64,
    cancel: &Cancel,
) -> AppResult<u64> {
    let mut buf = vec![0u8; 256 * 1024];
    let mut total: u64 = 0;
    loop {
        cancel.check()?;
        let n = r.read(&mut buf)?;
        if n == 0 {
            return Ok(total);
        }
        total += n as u64;
        if total > limit {
            return Err(AppError::Archive(ArchiveFailure::TooLarge(total)));
        }
        w.write_all(&buf[..n])?;
    }
}

impl ArchiveReader for ZipReader {
    fn list(&self) -> AppResult<Listing> {
        let mut archive = self.open(&Cancel::new())?;
        let mut listing = Listing::default();
        for i in 0..archive.len() {
            let f = archive.by_index_raw(i)?;
            let raw = f.name()?.to_string();
            let encrypted = f.encrypted();
            listing.encrypted |= encrypted;
            listing.entries.push(Entry {
                path: sanitize_entry_path(&raw),
                size: f.size(),
                is_dir: f.is_dir(),
                encrypted,
                is_link: f.is_symlink(),
            });
        }
        Ok(listing)
    }

    fn read_entry(&self, entry: &str, limit: u64, cancel: &Cancel) -> AppResult<Vec<u8>> {
        let mut archive = self.open(cancel)?;
        let idx = find_index(&mut archive, entry)?;
        let mut f = archive.by_index(idx)?;
        if f.size() > limit {
            return Err(AppError::Archive(ArchiveFailure::TooLarge(f.size())));
        }
        let mut out = Vec::with_capacity(f.size() as usize);
        copy_limited(&mut f, &mut out, limit, cancel)?;
        Ok(out)
    }

    fn extract_entries(
        &self,
        entries: &[String],
        dest: &Path,
        progress: ProgressFn<'_>,
        cancel: &Cancel,
    ) -> AppResult<Vec<(String, PathBuf)>> {
        let mut archive = self.open(cancel)?;
        let mut done = Vec::new();
        for (i, entry) in entries.iter().enumerate() {
            cancel.check()?;
            let Some(target) = safe_join(dest, entry) else {
                continue;
            };
            let idx = find_index(&mut archive, entry)?;
            let mut f = archive.by_index(idx)?;
            if f.is_symlink() || f.is_dir() {
                continue;
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = File::create(&target)?;
            if let Err(e) = copy_limited(&mut f, &mut out, u64::MAX, cancel) {
                drop(out);
                let _ = std::fs::remove_file(&target);
                return Err(e);
            }
            done.push((entry.clone(), target));
            progress((i + 1) as f32 / entries.len().max(1) as f32 * 100.0);
        }
        Ok(done)
    }
}

/// Find an entry by its sanitised path. Names in the index are sanitised, so compare that way
/// rather than relying on the stored name matching byte for byte.
fn find_index<R: Read + Seek>(archive: &mut ZipArchive<R>, entry: &str) -> AppResult<usize> {
    if let Some(i) = archive.index_for_name(entry) {
        return Ok(i);
    }
    for i in 0..archive.len() {
        let f = archive.by_index_raw(i)?;
        if f.name()
            .ok()
            .and_then(|n| sanitize_entry_path(&n))
            .as_deref()
            == Some(entry)
        {
            return Ok(i);
        }
    }
    Err(AppError::NotFound(format!("{entry} is not in the archive")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;

    pub fn make_zip(path: &Path, files: &[(&str, &[u8])]) {
        let f = File::create(path).expect("create");
        let mut w = zip::ZipWriter::new(f);
        for (name, data) in files {
            w.start_file(*name, SimpleFileOptions::default())
                .expect("start");
            w.write_all(data).expect("write");
        }
        w.finish().expect("finish");
    }

    #[test]
    fn lists_reads_and_extracts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let zp = dir.path().join("pack.zip");
        make_zip(
            &zp,
            &[("a/cube.stl", b"solid cube"), ("../evil.stl", b"bad")],
        );
        let r = ZipReader::file(zp);
        let l = r.list().expect("list");
        assert_eq!(l.entries.len(), 2);
        assert_eq!(l.entries[0].path.as_deref(), Some("a/cube.stl"));
        assert_eq!(l.entries[1].path, None);
        let bytes = r
            .read_entry("a/cube.stl", 1024, &Cancel::new())
            .expect("read");
        assert_eq!(bytes, b"solid cube");
        assert!(matches!(
            r.read_entry("a/cube.stl", 3, &Cancel::new()),
            Err(AppError::Archive(ArchiveFailure::TooLarge(_)))
        ));
        let out = dir.path().join("out");
        let done = r
            .extract_entries(
                &["a/cube.stl".into(), "../evil.stl".into()],
                &out,
                &mut |_| {},
                &Cancel::new(),
            )
            .expect("extract");
        assert_eq!(done.len(), 1);
        assert!(out.join("a").join("cube.stl").exists());
        assert!(!dir.path().join("evil.stl").exists());
    }

    #[test]
    fn reads_nested_zip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let inner = dir.path().join("inner.zip");
        make_zip(&inner, &[("x.stl", b"inner mesh")]);
        let inner_bytes = std::fs::read(&inner).expect("read inner");
        let outer = dir.path().join("outer.zip");
        make_zip(&outer, &[("parts/inner.zip", &inner_bytes)]);
        let r = ZipReader::nested(outer, "parts/inner.zip".into());
        assert_eq!(r.list().expect("list").entries.len(), 1);
        assert_eq!(
            r.read_entry("x.stl", 1024, &Cancel::new()).expect("read"),
            b"inner mesh"
        );
    }
}
