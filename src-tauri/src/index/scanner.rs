//! Walks a library folder and brings the database up to date. Incremental: anything whose
//! size and mtime are unchanged is skipped, and archives are only re-listed when the
//! signature of their parts changes.

use std::collections::{HashMap, HashSet};
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use walkdir::WalkDir;

use crate::archive::parts::{group_archives, sniff_kind, ArchiveSet};
use crate::archive::sevenzip::SevenZip;
use crate::archive::{open_reader, ArchiveSource, Listing, MAX_NESTED_ZIP_BYTES};
use crate::db::queries;
use crate::error::{AppError, AppResult, ArchiveFailure};
use crate::types::ScanProgress;
use crate::util::{
    ext_of, is_in_progress_download, is_indexed_name, mtime_secs, now_secs, path_str, Cancel,
};

/// Archive parts newer than this are assumed to still be downloading.
pub const QUIET_SECS: i64 = 10;

pub struct LibraryRef {
    pub id: i64,
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Default)]
pub struct ScanOutcome {
    pub changed: u64,
    pub models: u64,
    pub errors: u64,
    /// Something was skipped because it looked like a download in progress.
    pub deferred: bool,
    /// Archives whose contents changed, so cached extractions must go.
    pub changed_archives: Vec<i64>,
}

struct ExistingContainer {
    id: i64,
    signature: Option<String>,
    missing: bool,
}

struct ExistingModel {
    id: i64,
    container_id: i64,
    size: i64,
    mtime: i64,
    missing: bool,
}

struct Scan<'a> {
    lib: &'a LibraryRef,
    sz: &'a Arc<SevenZip>,
    cancel: &'a Cancel,
    now: i64,
    quiet_secs: i64,
    containers: HashMap<String, ExistingContainer>,
    loose: HashMap<String, ExistingModel>,
    seen_containers: HashSet<i64>,
    seen_models: HashSet<i64>,
    changed_models: Vec<i64>,
    out: ScanOutcome,
}

pub fn scan_library(
    conn: &mut Connection,
    lib: &LibraryRef,
    sz: &Arc<SevenZip>,
    progress: &dyn Fn(ScanProgress),
    cancel: &Cancel,
) -> AppResult<ScanOutcome> {
    scan_library_with(conn, lib, sz, progress, cancel, QUIET_SECS)
}

pub fn scan_library_with(
    conn: &mut Connection,
    lib: &LibraryRef,
    sz: &Arc<SevenZip>,
    progress: &dyn Fn(ScanProgress),
    cancel: &Cancel,
    quiet_secs: i64,
) -> AppResult<ScanOutcome> {
    if !lib.path.is_dir() {
        // An unplugged drive must not mark the whole library as missing.
        return Err(AppError::NotFound(format!(
            "Library folder not found: {}",
            lib.path.display()
        )));
    }
    let report = |phase: &str, done: u64, total: u64, current: Option<String>| {
        progress(ScanProgress {
            library_id: lib.id,
            library_name: lib.name.clone(),
            phase: phase.into(),
            done,
            total,
            current,
        })
    };

    let mut scan = Scan {
        lib,
        sz,
        cancel,
        now: now_secs(),
        quiet_secs,
        containers: HashMap::new(),
        loose: HashMap::new(),
        seen_containers: HashSet::new(),
        seen_models: HashSet::new(),
        changed_models: Vec::new(),
        out: ScanOutcome::default(),
    };
    scan.load_existing(conn)?;

    report("discovering", 0, 0, None);
    let (dirs, files) = discover(&lib.path, cancel, &mut scan.out.errors, &|n| {
        report("discovering", n, 0, None)
    })?;

    let total = dirs.len() as u64;
    let mut dir_ids: HashMap<PathBuf, i64> = HashMap::new();
    for (i, dir) in dirs.iter().enumerate() {
        cancel.check()?;
        if i % 25 == 0 {
            report("indexing", i as u64, total, Some(path_str(dir)));
        }
        let parent = dir.parent().and_then(|p| dir_ids.get(p)).copied();
        let is_root = dir == &lib.path;
        let tx = conn.transaction()?;
        let id = scan.upsert_folder(&tx, dir, if is_root { None } else { parent })?;
        dir_ids.insert(dir.clone(), id);
        if let Some(entries) = files.get(dir) {
            scan.index_files(&tx, id, entries)?;
        }
        tx.commit()?;
    }

    cancel.check()?;
    report("finishing", total, total, None);
    let tx = conn.transaction()?;
    scan.mark_missing(&tx)?;
    queries::recount_library(&tx, lib.id)?;
    let mut changed = std::mem::take(&mut scan.changed_models);
    changed.sort_unstable();
    changed.dedup();
    let mut retagged = Vec::new();
    if crate::autotag::enabled_on_index(&tx)? {
        retagged = crate::autotag::apply(&tx, Some(&changed), None)?;
        // New folders may have made a word common enough to become a tag of its own.
        if !changed.is_empty() && crate::autotag::suggestions_enabled(&tx)? {
            queries::recount_library(&tx, lib.id)?;
            retagged.extend(crate::autotag::create_from_suggestions(&tx)?);
        }
    }
    let mut fts_ids = changed.clone();
    fts_ids.extend(retagged);
    fts_ids.sort_unstable();
    fts_ids.dedup();
    queries::refresh_fts(&tx, &fts_ids)?;
    tx.execute(
        "UPDATE libraries SET last_scanned_at = ?2 WHERE id = ?1",
        params![lib.id, scan.now],
    )?;
    scan.out.models = tx.query_row(
        &format!(
            "SELECT COUNT(*) FROM models WHERE library_id = ?1 AND missing = 0 AND ext IN {}",
            crate::util::MESH_EXTS_SQL
        ),
        [lib.id],
        |r| r.get::<_, i64>(0),
    )? as u64;
    tx.commit()?;
    scan.out.changed = changed.len() as u64;
    Ok(scan.out)
}

type Discovered = (Vec<PathBuf>, HashMap<PathBuf, Vec<(PathBuf, Metadata)>>);

fn discover(
    root: &Path,
    cancel: &Cancel,
    errors: &mut u64,
    tick: &dyn Fn(u64),
) -> AppResult<Discovered> {
    let mut dirs = Vec::new();
    let mut files: HashMap<PathBuf, Vec<(PathBuf, Metadata)>> = HashMap::new();
    let walker = WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.'));
    let mut n: u64 = 0;
    for entry in walker {
        n += 1;
        if n % 1000 == 0 {
            cancel.check()?;
            tick(n);
        }
        let entry = match entry {
            Ok(e) => e,
            Err(_) => {
                *errors += 1;
                continue;
            }
        };
        let ft = entry.file_type();
        if ft.is_dir() {
            dirs.push(entry.path().to_path_buf());
        } else if ft.is_file() {
            let name = entry.file_name().to_string_lossy();
            if is_in_progress_download(&name) {
                continue;
            }
            let Some(parent) = entry.path().parent() else {
                continue;
            };
            match entry.metadata() {
                Ok(m) => files
                    .entry(parent.to_path_buf())
                    .or_default()
                    .push((entry.path().to_path_buf(), m)),
                Err(_) => *errors += 1,
            }
        }
    }
    Ok((dirs, files))
}

impl Scan<'_> {
    fn load_existing(&mut self, conn: &Connection) -> AppResult<()> {
        let mut stmt = conn
            .prepare("SELECT id, path, signature, missing FROM containers WHERE library_id = ?1")?;
        let rows = stmt.query_map([self.lib.id], |r| {
            Ok((
                r.get::<_, String>(1)?,
                ExistingContainer {
                    id: r.get(0)?,
                    signature: r.get(2)?,
                    missing: r.get(3)?,
                },
            ))
        })?;
        for row in rows {
            let (path, c) = row?;
            self.containers.insert(path, c);
        }
        let mut stmt = conn.prepare(
            "SELECT m.id, m.rel_path, m.container_id, m.size, m.mtime, m.missing
             FROM models m JOIN containers c ON c.id = m.container_id
             WHERE m.library_id = ?1 AND c.kind = 'folder'",
        )?;
        let rows = stmt.query_map([self.lib.id], |r| {
            Ok((
                r.get::<_, String>(1)?,
                ExistingModel {
                    id: r.get(0)?,
                    container_id: r.get(2)?,
                    size: r.get(3)?,
                    mtime: r.get(4)?,
                    missing: r.get(5)?,
                },
            ))
        })?;
        for row in rows {
            let (path, m) = row?;
            self.loose.insert(path, m);
        }
        Ok(())
    }

    fn upsert_folder(
        &mut self,
        tx: &Transaction,
        dir: &Path,
        parent: Option<i64>,
    ) -> AppResult<i64> {
        let path = path_str(dir);
        let name = if parent.is_none() {
            self.lib.name.clone()
        } else {
            dir.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.clone())
        };
        let id: i64 = tx.query_row(
            "INSERT INTO containers (library_id, parent_id, kind, path, name)
             VALUES (?1, ?2, 'folder', ?3, ?4)
             ON CONFLICT (library_id, path) DO UPDATE SET
               parent_id = excluded.parent_id, name = excluded.name, missing = 0
             RETURNING id",
            params![self.lib.id, parent, path, name],
            |r| r.get(0),
        )?;
        self.seen_containers.insert(id);
        Ok(id)
    }

    fn index_files(
        &mut self,
        tx: &Transaction,
        folder_id: i64,
        entries: &[(PathBuf, Metadata)],
    ) -> AppResult<()> {
        let meta: HashMap<&Path, &Metadata> =
            entries.iter().map(|(p, m)| (p.as_path(), m)).collect();
        let paths: Vec<PathBuf> = entries.iter().map(|(p, _)| p.clone()).collect();
        let (sets, rest) = group_archives(&paths);

        for f in rest {
            let name = f
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !is_indexed_name(&name) {
                continue;
            }
            let Some(m) = meta.get(f.as_path()) else {
                continue;
            };
            self.index_loose(tx, folder_id, &f, &name, m)?;
        }
        for set in sets {
            self.cancel.check()?;
            self.index_set(tx, folder_id, &set, &meta)?;
        }
        Ok(())
    }

    fn index_loose(
        &mut self,
        tx: &Transaction,
        folder_id: i64,
        path: &Path,
        name: &str,
        meta: &Metadata,
    ) -> AppResult<()> {
        let rel = path_str(path);
        let size = meta.len() as i64;
        let mtime = mtime_secs(meta);
        if let Some(ex) = self.loose.get(&rel) {
            self.seen_models.insert(ex.id);
            if ex.size == size && ex.mtime == mtime && !ex.missing && ex.container_id == folder_id {
                return Ok(());
            }
            tx.prepare_cached(
                "UPDATE models SET size = ?2, mtime = ?3, missing = 0, container_id = ?4, name = ?5
                 WHERE id = ?1",
            )?
            .execute(params![ex.id, size, mtime, folder_id, name])?;
            self.changed_models.push(ex.id);
            return Ok(());
        }
        let id: i64 = tx
            .prepare_cached(
                "INSERT INTO models (container_id, library_id, rel_path, name, ext, size, mtime)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (container_id, rel_path) DO UPDATE SET
               size = excluded.size, mtime = excluded.mtime, missing = 0
             RETURNING id",
            )?
            .query_row(
                params![folder_id, self.lib.id, rel, name, ext_of(name), size, mtime],
                |r| r.get(0),
            )?;
        self.seen_models.insert(id);
        self.changed_models.push(id);
        Ok(())
    }

    fn index_set(
        &mut self,
        tx: &Transaction,
        folder_id: i64,
        set: &ArchiveSet,
        meta: &HashMap<&Path, &Metadata>,
    ) -> AppResult<()> {
        let mut size: i64 = 0;
        let mut mtime: i64 = 0;
        let mut sig_parts = Vec::new();
        for p in &set.parts {
            let (s, m) = meta
                .get(p.as_path())
                .map(|m| (m.len() as i64, mtime_secs(m)))
                .unwrap_or((0, 0));
            size += s;
            mtime = mtime.max(m);
            sig_parts.push(format!(
                "{}:{s}:{m}",
                p.file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default()
            ));
        }
        let signature = format!("{}|missing:{:?}", sig_parts.join("|"), set.missing);
        let path = path_str(&set.entry);
        let existing = self.containers.get(&path);

        if self.now - mtime < self.quiet_secs {
            self.out.deferred = true;
            if let Some(ex) = existing {
                let id = ex.id;
                self.keep_archive(tx, id)?;
            }
            return Ok(());
        }
        if let Some(ex) = existing {
            if ex.signature.as_deref() == Some(signature.as_str()) && !ex.missing {
                let id = ex.id;
                self.keep_archive(tx, id)?;
                return Ok(());
            }
        }

        let kind = match set.kind {
            Some(k) => k,
            None => match set.parts.first().and_then(|p| sniff_kind(p)) {
                Some(k) => k,
                None => return Ok(()), // numbered files that are not an archive
            },
        };
        let part_paths: Option<String> = if set.multi_part {
            Some(serde_json::to_string(
                &set.parts.iter().map(|p| path_str(p)).collect::<Vec<_>>(),
            )?)
        } else {
            None
        };

        let src = ArchiveSource {
            kind: kind.to_string(),
            path: set.entry.clone(),
            multi_part: set.multi_part,
            nested: None,
        };
        let listing: Result<Listing, ArchiveFailure> = if let Some(d) = set.missing_detail() {
            Err(ArchiveFailure::MissingParts(d))
        } else if !set.entry_present {
            Err(ArchiveFailure::MissingParts("Missing part 1".into()))
        } else {
            match open_reader(&src, self.sz).and_then(|r| r.list()) {
                Ok(l) => Ok(l),
                Err(AppError::Archive(f)) => Err(f),
                Err(e) => Err(ArchiveFailure::Corrupt(e.to_string())),
            }
        };

        let (status, detail, solid, encrypted) = match &listing {
            Ok(l) => ("ok", None, l.solid, l.encrypted),
            Err(ArchiveFailure::MissingParts(d)) => {
                ("missing_parts", Some(d.clone()), false, false)
            }
            Err(ArchiveFailure::Encrypted) => ("ok", None, false, true),
            Err(ArchiveFailure::ToolMissing) => (
                "unsupported",
                Some("7-Zip was not found. Install it or check Settings → Archives.".to_string()),
                false,
                false,
            ),
            Err(ArchiveFailure::Unsupported(d)) => ("unsupported", Some(d.clone()), false, false),
            Err(f) => (
                "corrupt",
                Some(AppError::Archive(f.clone()).to_string()),
                false,
                false,
            ),
        };
        if status != "ok" {
            self.out.errors += 1;
        }

        let id: i64 = tx.query_row(
            "INSERT INTO containers (library_id, parent_id, kind, path, part_paths, name, size, mtime,
                                     signature, solid, encrypted, status, status_detail)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT (library_id, path) DO UPDATE SET
               parent_id = excluded.parent_id, kind = excluded.kind, part_paths = excluded.part_paths,
               name = excluded.name, size = excluded.size, mtime = excluded.mtime,
               signature = excluded.signature, solid = excluded.solid, encrypted = excluded.encrypted,
               status = excluded.status, status_detail = excluded.status_detail, missing = 0
             RETURNING id",
            params![
                self.lib.id, folder_id, kind, path, part_paths, set.name, size, mtime, signature,
                solid, encrypted, status, detail
            ],
            |r| r.get(0),
        )?;
        self.seen_containers.insert(id);
        self.out.changed_archives.push(id);

        let Ok(listing) = listing else {
            // Keep the old rows (and their tags) until the archive is readable again.
            self.keep_children(tx, id)?;
            return Ok(());
        };
        self.sync_entries(tx, id, &listing)?;

        if kind == "zip" && !set.multi_part {
            self.index_nested_zips(tx, id, &set.entry, &path, &signature, &listing)?;
        } else {
            self.keep_children(tx, id)?;
        }
        Ok(())
    }

    /// Bring the models of one archive container in line with its listing.
    fn sync_entries(
        &mut self,
        tx: &Transaction,
        container_id: i64,
        listing: &Listing,
    ) -> AppResult<()> {
        let mut old: HashMap<String, i64> = HashMap::new();
        {
            let mut stmt =
                tx.prepare_cached("SELECT rel_path, id FROM models WHERE container_id = ?1")?;
            for row in
                stmt.query_map([container_id], |r| Ok((r.get::<_, String>(0)?, r.get(1)?)))?
            {
                let (p, id) = row?;
                old.insert(p, id);
            }
        }
        let mut upsert = tx.prepare_cached(
            "INSERT INTO models (container_id, library_id, rel_path, name, ext, size, mtime, encrypted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (container_id, rel_path) DO UPDATE SET
               name = excluded.name, ext = excluded.ext, size = excluded.size,
               mtime = excluded.mtime, encrypted = excluded.encrypted, missing = 0
             RETURNING id",
        )?;
        for e in &listing.entries {
            let Some(rel) = &e.path else { continue };
            if e.is_dir || e.is_link {
                continue;
            }
            let name = rel.rsplit('/').next().unwrap_or(rel);
            if !is_indexed_name(name) {
                continue;
            }
            let id: i64 = upsert.query_row(
                params![
                    container_id,
                    self.lib.id,
                    rel,
                    name,
                    ext_of(name),
                    e.size as i64,
                    e.mtime,
                    e.encrypted
                ],
                |r| r.get(0),
            )?;
            old.remove(rel);
            self.seen_models.insert(id);
            self.changed_models.push(id);
        }
        let mut gone =
            tx.prepare_cached("UPDATE models SET missing = 1 WHERE id = ?1 AND missing = 0")?;
        for id in old.into_values() {
            if gone.execute([id])? > 0 {
                self.changed_models.push(id);
            }
        }
        Ok(())
    }

    fn index_nested_zips(
        &mut self,
        tx: &Transaction,
        parent_id: i64,
        outer: &Path,
        outer_path: &str,
        outer_sig: &str,
        listing: &Listing,
    ) -> AppResult<()> {
        for e in &listing.entries {
            let Some(rel) = &e.path else { continue };
            if e.is_dir || e.is_link || ext_of(rel) != "zip" || e.size > MAX_NESTED_ZIP_BYTES {
                continue;
            }
            self.cancel.check()?;
            let src = ArchiveSource {
                kind: "zip".into(),
                path: outer.to_path_buf(),
                multi_part: false,
                nested: Some((outer.to_path_buf(), rel.clone())),
            };
            let inner = open_reader(&src, self.sz).and_then(|r| r.list());
            let (status, detail, encrypted) = match &inner {
                Ok(l) => ("ok", None, l.encrypted),
                Err(AppError::Archive(ArchiveFailure::Encrypted)) => ("ok", None, true),
                Err(e) => ("corrupt", Some(e.to_string()), false),
            };
            let name = rel.rsplit('/').next().unwrap_or(rel).to_string();
            let id: i64 = tx.query_row(
                "INSERT INTO containers (library_id, parent_id, kind, path, entry_path, name, size, mtime,
                                         signature, encrypted, status, status_detail)
                 VALUES (?1, ?2, 'zip', ?3, ?4, ?5, ?6, 0, ?7, ?8, ?9, ?10)
                 ON CONFLICT (library_id, path) DO UPDATE SET
                   parent_id = excluded.parent_id, size = excluded.size, signature = excluded.signature,
                   encrypted = excluded.encrypted, status = excluded.status,
                   status_detail = excluded.status_detail, missing = 0
                 RETURNING id",
                params![
                    self.lib.id,
                    parent_id,
                    format!("{outer_path}::{rel}"),
                    rel,
                    name,
                    e.size as i64,
                    format!("{outer_sig}::{rel}"),
                    encrypted,
                    status,
                    detail
                ],
                |r| r.get(0),
            )?;
            self.seen_containers.insert(id);
            self.out.changed_archives.push(id);
            if let Ok(l) = inner {
                self.sync_entries(tx, id, &l)?;
            }
        }
        Ok(())
    }

    /// An archive we did not re-list: keep it and everything nested in it.
    fn keep_archive(&mut self, tx: &Transaction, id: i64) -> AppResult<()> {
        self.seen_containers.insert(id);
        self.keep_children(tx, id)
    }

    fn keep_children(&mut self, tx: &Transaction, id: i64) -> AppResult<()> {
        let mut stmt = tx.prepare_cached("SELECT id FROM containers WHERE parent_id = ?1")?;
        let kids: Vec<i64> = stmt
            .query_map([id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        self.seen_containers.extend(kids);
        Ok(())
    }

    fn mark_missing(&mut self, tx: &Transaction) -> AppResult<()> {
        let gone: Vec<i64> = self
            .containers
            .values()
            .filter(|c| !c.missing && !self.seen_containers.contains(&c.id))
            .map(|c| c.id)
            .collect();
        for id in gone {
            tx.execute("UPDATE containers SET missing = 1 WHERE id = ?1", [id])?;
            let ids = queries::model_ids_in_container(tx, id)?;
            tx.execute(
                "UPDATE models SET missing = 1 WHERE container_id = ?1 AND missing = 0",
                [id],
            )?;
            self.changed_models.extend(ids);
            self.out.changed_archives.push(id);
        }
        let lost: Vec<i64> = self
            .loose
            .values()
            .filter(|m| !m.missing && !self.seen_models.contains(&m.id))
            .map(|m| m.id)
            .collect();
        for id in lost {
            tx.execute("UPDATE models SET missing = 1 WHERE id = ?1", [id])?;
            self.changed_models.push(id);
        }
        Ok(())
    }
}

/// Library id and folder for an id, for callers that only have the id.
pub fn library_ref(conn: &Connection, id: i64) -> AppResult<LibraryRef> {
    conn.query_row(
        "SELECT id, name, path FROM libraries WHERE id = ?1",
        [id],
        |r| {
            Ok(LibraryRef {
                id: r.get(0)?,
                name: r.get(1)?,
                path: PathBuf::from(r.get::<_, String>(2)?),
            })
        },
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("library {id}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn setup(dir: &Path) -> (Connection, LibraryRef) {
        let conn = crate::db::test_conn();
        conn.execute(
            "INSERT INTO libraries (name, path) VALUES ('Test', ?1)",
            [path_str(dir)],
        )
        .expect("insert library");
        let lib = library_ref(&conn, 1).expect("library");
        (conn, lib)
    }

    fn scan(conn: &mut Connection, lib: &LibraryRef) -> ScanOutcome {
        let sz = Arc::new(SevenZip::with_exe(None));
        scan_library_with(conn, lib, &sz, &|_| {}, &Cancel::new(), 0).expect("scan")
    }

    fn write_zip(path: &Path, files: &[(&str, &[u8])]) {
        let f = std::fs::File::create(path).expect("create");
        let mut w = zip::ZipWriter::new(f);
        for (n, d) in files {
            w.start_file(*n, zip::write::SimpleFileOptions::default())
                .expect("start");
            w.write_all(d).expect("write");
        }
        w.finish().expect("finish");
    }

    fn count(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |r| r.get(0)).expect("count")
    }

    #[test]
    fn indexes_folders_files_and_zips() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("sub")).expect("mkdir");
        std::fs::write(dir.path().join("sub/cube.stl"), b"solid").expect("write");
        std::fs::write(dir.path().join("notes.txt"), b"x").expect("write");
        std::fs::write(dir.path().join("half.zip.crdownload"), b"x").expect("write");
        write_zip(
            &dir.path().join("pack.zip"),
            &[
                ("a/one.stl", b"1"),
                ("two.3mf", b"2"),
                ("x.txt", b"3"),
                ("img/render.jpg", b"4"),
            ],
        );
        std::fs::write(dir.path().join("guide.pdf"), b"%PDF").expect("write");

        let (mut conn, lib) = setup(dir.path());
        let out = scan(&mut conn, &lib);
        assert_eq!(out.models, 3);
        assert_eq!(
            count(&conn, "SELECT COUNT(*) FROM containers WHERE kind = 'zip'"),
            1
        );
        let root_count = count(
            &conn,
            "SELECT model_count FROM containers WHERE parent_id IS NULL",
        );
        assert_eq!(root_count, 3, "images and PDFs are not models");
        let root_files = count(
            &conn,
            "SELECT file_count FROM containers WHERE parent_id IS NULL",
        );
        assert_eq!(root_files, 5, "but they are indexed");
        // search rows were written
        assert_eq!(
            count(
                &conn,
                "SELECT COUNT(*) FROM models_fts WHERE models_fts MATCH 'one'"
            ),
            1
        );
    }

    #[test]
    fn rescans_are_incremental_and_missing_is_soft() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.stl"), b"solid a").expect("write");
        std::fs::write(dir.path().join("b.stl"), b"solid b").expect("write");
        let (mut conn, lib) = setup(dir.path());
        assert_eq!(scan(&mut conn, &lib).changed, 2);
        assert_eq!(
            scan(&mut conn, &lib).changed,
            0,
            "unchanged files are skipped"
        );

        conn.execute("UPDATE models SET rot_x = 90 WHERE name = 'a.stl'", [])
            .expect("rot");
        std::fs::remove_file(dir.path().join("a.stl")).expect("rm");
        let out = scan(&mut conn, &lib);
        assert_eq!(out.changed, 1);
        assert_eq!(out.models, 1);
        assert_eq!(
            count(&conn, "SELECT missing FROM models WHERE name = 'a.stl'"),
            1
        );

        // Coming back restores the same row, orientation intact.
        std::fs::write(dir.path().join("a.stl"), b"solid a").expect("write");
        scan(&mut conn, &lib);
        assert_eq!(
            count(&conn, "SELECT missing FROM models WHERE name = 'a.stl'"),
            0
        );
        assert_eq!(
            count(
                &conn,
                "SELECT CAST(rot_x AS INTEGER) FROM models WHERE name = 'a.stl'"
            ),
            90
        );
    }

    #[test]
    fn changed_zip_is_relisted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let zp = dir.path().join("p.zip");
        write_zip(&zp, &[("one.stl", b"1")]);
        let (mut conn, lib) = setup(dir.path());
        scan(&mut conn, &lib);
        std::thread::sleep(std::time::Duration::from_millis(1100));
        write_zip(&zp, &[("one.stl", b"1"), ("two.stl", b"22")]);
        let out = scan(&mut conn, &lib);
        assert_eq!(out.models, 2);
        assert_eq!(out.changed_archives.len(), 1);
    }

    #[test]
    fn missing_parts_are_flagged_not_hidden() {
        let dir = tempfile::tempdir().expect("tempdir");
        for n in [1, 2, 4] {
            std::fs::write(dir.path().join(format!("Big.part{n}.rar")), b"Rar!").expect("write");
        }
        let (mut conn, lib) = setup(dir.path());
        let out = scan(&mut conn, &lib);
        assert_eq!(out.errors, 1);
        let (status, detail, parts): (String, String, i64) = conn
            .query_row(
                "SELECT status, status_detail, json_array_length(part_paths) FROM containers WHERE kind = 'rar'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("row");
        assert_eq!(status, "missing_parts");
        assert_eq!(detail, "Missing part 3 of 4");
        assert_eq!(parts, 3);
        assert_eq!(
            count(
                &conn,
                "SELECT issue_count FROM containers WHERE parent_id IS NULL"
            ),
            1
        );
        // Exactly one container for the set, not one per part.
        assert_eq!(
            count(&conn, "SELECT COUNT(*) FROM containers WHERE kind = 'rar'"),
            1
        );
    }

    #[test]
    fn fresh_parts_wait_for_quiet_period() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_zip(&dir.path().join("new.zip"), &[("one.stl", b"1")]);
        let (mut conn, lib) = setup(dir.path());
        let sz = Arc::new(SevenZip::with_exe(None));
        let out =
            scan_library_with(&mut conn, &lib, &sz, &|_| {}, &Cancel::new(), 3600).expect("scan");
        assert!(out.deferred);
        assert_eq!(out.models, 0);
    }

    #[test]
    fn nested_zip_one_level() {
        let dir = tempfile::tempdir().expect("tempdir");
        let inner = dir.path().join("inner.tmpzip");
        write_zip(&inner, &[("deep.stl", b"d")]);
        let bytes = std::fs::read(&inner).expect("read");
        std::fs::remove_file(&inner).expect("rm");
        write_zip(
            &dir.path().join("outer.zip"),
            &[("bits/inner.zip", &bytes), ("top.stl", b"t")],
        );
        let (mut conn, lib) = setup(dir.path());
        let out = scan(&mut conn, &lib);
        assert_eq!(out.models, 2);
        assert_eq!(
            count(
                &conn,
                "SELECT COUNT(*) FROM containers WHERE entry_path = 'bits/inner.zip'"
            ),
            1
        );
    }

    #[test]
    fn missing_library_folder_is_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (mut conn, mut lib) = setup(dir.path());
        lib.path = dir.path().join("nope");
        let sz = Arc::new(SevenZip::with_exe(None));
        assert!(scan_library(&mut conn, &lib, &sz, &|_| {}, &Cancel::new()).is_err());
    }
}
