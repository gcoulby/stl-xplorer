//! Tidying unpacked folders: checking a folder against an archive of the same files, and
//! zipping a folder up. Nothing here deletes anything; callers only remove a folder after
//! `compare` says the archive holds every file in it.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::archive::Listing;
use crate::error::{AppError, AppResult};
use crate::util::Cancel;

/// The files in a folder, relative and `/` separated, with their sizes.
pub fn folder_files(folder: &Path) -> AppResult<Vec<(String, PathBuf, u64)>> {
    let mut out = Vec::new();
    for entry in WalkDir::new(folder).follow_links(false).sort_by_file_name() {
        let entry =
            entry.map_err(|e| AppError::Other(format!("Could not read the folder: {e}")))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(folder)
            .map_err(|_| AppError::Other("path outside the folder".into()))?;
        let rel = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        out.push((rel, entry.path().to_path_buf(), size));
    }
    Ok(out)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Comparison {
    pub files: u64,
    pub matched: u64,
    pub bytes: u64,
    /// A few files the archive lacks or holds at a different size.
    pub mismatched: Vec<String>,
}

impl Comparison {
    /// Every file in the folder is in the archive at the same size.
    pub fn complete(&self) -> bool {
        self.files > 0 && self.matched == self.files
    }
}

/// Check every file in `folder` against an archive listing. Archives often wrap their contents
/// in one top-level folder, so an entry also counts if it matches after that first component.
pub fn compare(files: &[(String, PathBuf, u64)], listing: &Listing) -> Comparison {
    let mut index: HashMap<String, u64> = HashMap::new();
    for e in &listing.entries {
        let Some(p) = &e.path else { continue };
        if e.is_dir {
            continue;
        }
        let lower = p.to_lowercase();
        if let Some((_, rest)) = lower.split_once('/') {
            index.entry(rest.to_string()).or_insert(e.size);
        }
        index.insert(lower, e.size);
    }
    let mut c = Comparison::default();
    for (rel, _, size) in files {
        c.files += 1;
        c.bytes += size;
        match index.get(&rel.to_lowercase()) {
            Some(s) if s == size => c.matched += 1,
            _ => {
                if c.mismatched.len() < 5 {
                    c.mismatched.push(rel.clone());
                }
            }
        }
    }
    c
}

/// Zip `folder` into `target`, with the folder's name as the top-level directory. Written to
/// a `.partial` file first (which the indexer ignores) and renamed only once complete.
pub fn write_zip(
    folder: &Path,
    target: &Path,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &Cancel,
) -> AppResult<()> {
    let files = folder_files(folder)?;
    if files.is_empty() {
        return Err(AppError::Invalid(format!(
            "{} has no files to zip",
            folder.display()
        )));
    }
    let top = folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "files".into());
    let total: u64 = files.iter().map(|f| f.2).sum();
    let mut partial = target.as_os_str().to_owned();
    partial.push(".partial");
    let partial = PathBuf::from(partial);

    let result = (|| -> AppResult<()> {
        let out = BufWriter::new(File::create(&partial)?);
        let mut zw = zip::ZipWriter::new(out);
        let mut done: u64 = 0;
        let mut buf = vec![0u8; 1024 * 1024];
        for (rel, path, size) in &files {
            cancel.check()?;
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .large_file(*size >= u32::MAX as u64);
            zw.start_file(format!("{top}/{rel}"), opts)?;
            let mut f = File::open(path)?;
            loop {
                let n = f.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                zw.write_all(&buf[..n])?;
                done += n as u64;
                progress(done, total);
            }
        }
        zw.finish()?.flush()?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&partial);
        return Err(e);
    }
    std::fs::rename(&partial, target)?;
    Ok(())
}

/// Strip an archive's extension: "Pack.zip" and "Pack.7z" both give "Pack".
pub fn archive_stem(name: &str) -> &str {
    name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::zip::ZipReader;
    use crate::archive::ArchiveReader;

    fn make_folder(dir: &Path) -> PathBuf {
        let f = dir.join("Pack");
        std::fs::create_dir_all(f.join("sub")).expect("mkdir");
        std::fs::write(f.join("a.stl"), b"solid a").expect("write");
        std::fs::write(f.join("sub/b.png"), vec![7u8; 5000]).expect("write");
        f
    }

    #[test]
    fn zips_a_folder_and_the_result_matches_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let folder = make_folder(dir.path());
        let target = dir.path().join("Pack.zip");
        let mut last = (0, 0);
        write_zip(&folder, &target, &mut |d, t| last = (d, t), &Cancel::new()).expect("zip");
        assert!(target.is_file());
        assert!(!dir.path().join("Pack.zip.partial").exists());
        assert_eq!(last, (5007, 5007));

        let listing = ZipReader::file(target).list().expect("list");
        let c = compare(&folder_files(&folder).expect("files"), &listing);
        assert!(c.complete(), "{c:?}");
        assert_eq!(c.files, 2);
    }

    #[test]
    fn a_folder_with_an_extra_or_changed_file_does_not_match() {
        let dir = tempfile::tempdir().expect("tempdir");
        let folder = make_folder(dir.path());
        let target = dir.path().join("Pack.zip");
        write_zip(&folder, &target, &mut |_, _| {}, &Cancel::new()).expect("zip");
        std::fs::write(folder.join("notes.txt"), b"mine").expect("write");
        std::fs::write(folder.join("a.stl"), b"solid a, edited").expect("write");
        let listing = ZipReader::file(target).list().expect("list");
        let c = compare(&folder_files(&folder).expect("files"), &listing);
        assert!(!c.complete());
        assert_eq!(c.matched, 1);
        assert_eq!(c.mismatched, vec!["a.stl", "notes.txt"]);
    }

    #[test]
    fn archives_without_a_top_folder_match_too() {
        let dir = tempfile::tempdir().expect("tempdir");
        let folder = make_folder(dir.path());
        let listing = Listing {
            entries: vec![
                crate::archive::Entry {
                    path: Some("A.STL".into()),
                    size: 7,
                    is_dir: false,
                    encrypted: false,
                    is_link: false,
                    mtime: 0,
                },
                crate::archive::Entry {
                    path: Some("sub/b.png".into()),
                    size: 5000,
                    is_dir: false,
                    encrypted: false,
                    is_link: false,
                    mtime: 0,
                },
            ],
            ..Default::default()
        };
        assert!(compare(&folder_files(&folder).expect("files"), &listing).complete());
    }

    #[test]
    fn empty_folders_never_count_as_matching() {
        assert!(!Comparison::default().complete());
        assert_eq!(archive_stem("Big Pack.7z"), "Big Pack");
    }
}
