//! Finding and removing unpacked copies of archives, and zipping folders up.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use tauri::{AppHandle, Emitter, Manager};

use super::blocking;
use super::delete::{execute, Planned, Target};
use crate::archive::zip::ZipReader;
use crate::archive::{open_reader, ArchiveReader};
use crate::db::queries;
use crate::error::{AppError, AppResult};
use crate::packs::{archive_stem, compare, folder_files, write_zip, Comparison};
use crate::state::AppState;
use crate::types::{ContainerInfo, PackProgress, UnpackedCopy, ZipReport};
use crate::util::Cancel;

/// Folders that sit next to an archive with the same name, optionally only below `scope`.
fn candidates(conn: &Connection, scope: Option<i64>) -> AppResult<Vec<(i64, i64)>> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE sub(id) AS (
           SELECT ?1 WHERE ?1 IS NOT NULL
           UNION ALL
           SELECT c.id FROM containers c JOIN sub ON c.parent_id = sub.id
         )
         SELECT f.id, f.name, a.id, a.name
         FROM containers f
         JOIN containers a ON a.parent_id = f.parent_id AND a.kind <> 'folder'
           AND a.entry_path IS NULL AND a.missing = 0 AND a.status = 'ok' AND a.encrypted = 0
         JOIN libraries l ON l.id = f.library_id
         WHERE f.kind = 'folder' AND f.missing = 0 AND f.parent_id IS NOT NULL AND l.enabled = 1
           AND (?1 IS NULL OR f.id IN (SELECT id FROM sub))",
    )?;
    let rows = stmt.query_map([scope], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (fid, fname, aid, aname) = row?;
        if fname.to_lowercase() == archive_stem(&aname).to_lowercase() {
            out.push((fid, aid));
        }
    }
    Ok(out)
}

fn compare_with_archive(
    st: &AppState,
    folder: &ContainerInfo,
    archive: ContainerInfo,
) -> AppResult<Comparison> {
    let meta = {
        let conn = st.db.lock()?;
        crate::preview::archive_meta(&conn, archive)?
    };
    let listing = open_reader(&meta.source, &st.sz)?.list()?;
    Ok(compare(&folder_files(Path::new(&folder.path))?, &listing))
}

#[tauri::command]
pub async fn find_unpacked(app: AppHandle, scope: Option<i64>) -> AppResult<Vec<UnpackedCopy>> {
    blocking(app, move |_, st| {
        let pairs = {
            let conn = st.db.lock()?;
            let mut v = Vec::new();
            for (f, a) in candidates(&conn, scope)? {
                v.push((
                    queries::get_container(&conn, f)?,
                    queries::get_container(&conn, a)?,
                ));
            }
            v
        };
        let mut out = Vec::new();
        for (folder, archive) in pairs {
            if !Path::new(&folder.path).is_dir() {
                continue;
            }
            // One unreadable archive must not stop the rest of the search.
            let Ok(c) = compare_with_archive(st, &folder, archive.clone()) else {
                continue;
            };
            if c.files == 0 {
                continue;
            }
            out.push(UnpackedCopy {
                folder,
                archive,
                files: c.files,
                matched: c.matched,
                bytes: c.bytes,
                mismatched: c.mismatched,
            });
        }
        out.sort_by(|a, b| b.bytes.cmp(&a.bytes));
        Ok(out)
    })
    .await
}

/// Delete unpacked copies, checking each one against its archive again first.
#[tauri::command]
pub async fn delete_unpacked(
    app: AppHandle,
    folder_ids: Vec<i64>,
) -> AppResult<crate::types::DeleteReport> {
    blocking(app, move |_, st| {
        let mut planned = Vec::new();
        let mut errors = Vec::new();
        for id in folder_ids {
            let (folder, archive) = {
                let conn = st.db.lock()?;
                let pair = candidates(&conn, Some(id))?
                    .into_iter()
                    .find(|(f, _)| *f == id);
                match pair {
                    Some((f, a)) => (
                        queries::get_container(&conn, f)?,
                        queries::get_container(&conn, a)?,
                    ),
                    None => {
                        errors.push(format!("Folder {id} no longer has a matching archive"));
                        continue;
                    }
                }
            };
            match compare_with_archive(st, &folder, archive.clone()) {
                Ok(c) if c.complete() => planned.push(Planned {
                    target: Target::Container(folder.id),
                    label: folder.name.clone(),
                    library_id: folder.library_id,
                    paths: vec![PathBuf::from(&folder.path)],
                }),
                Ok(_) => errors.push(format!(
                    "{} has files that are not in {}. Left as it is.",
                    folder.name, archive.name
                )),
                Err(e) => errors.push(format!("Could not check {}: {e}", archive.name)),
            }
        }
        let mut report = execute(st, planned)?;
        errors.append(&mut report.errors);
        report.errors = errors;
        Ok(report)
    })
    .await
}

/// Zip each folder into `<name>.zip` beside it. An existing zip of the same name is reused if
/// it holds every file; otherwise the folder is left alone. Folders are only deleted once
/// their zip has been checked against them.
#[tauri::command]
pub async fn zip_folders(
    app: AppHandle,
    container_ids: Vec<i64>,
    delete_after: bool,
) -> AppResult<ZipReport> {
    blocking(app, move |app, st| {
        let mut report = ZipReport::default();
        let mut planned = Vec::new();
        let mut libs: Vec<i64> = Vec::new();
        for id in container_ids {
            let folder = queries::get_container(&*st.db.lock()?, id)?;
            if folder.kind != "folder" || folder.parent_id.is_none() {
                report
                    .errors
                    .push(format!("{} is not a folder that can be zipped", folder.name));
                continue;
            }
            let dir = PathBuf::from(&folder.path);
            let Some(parent) = dir.parent() else {
                continue;
            };
            let target = parent.join(format!("{}.zip", folder.name));
            let files = match folder_files(&dir) {
                Ok(f) => f,
                Err(e) => {
                    report.errors.push(format!("{}: {e}", folder.name));
                    continue;
                }
            };

            if target.exists() {
                let same = ZipReader::file(target.clone())
                    .list()
                    .map(|l| compare(&files, &l).complete())
                    .unwrap_or(false);
                if !same {
                    report.errors.push(format!(
                        "{}.zip already exists and does not match the folder. Both were left alone.",
                        folder.name
                    ));
                    continue;
                }
                report.already.push(folder.name.clone());
            } else {
                let label = folder.name.clone();
                let mut last = 0u64;
                let mut emit = |done: u64, total: u64| {
                    // About a hundred updates per folder is plenty.
                    if done == total || done - last >= (total / 100).max(1 << 20) {
                        last = done;
                        let _ = app.emit(
                            "pack-progress",
                            PackProgress {
                                label: label.clone(),
                                done,
                                total,
                            },
                        );
                    }
                };
                if let Err(e) = write_zip(&dir, &target, &mut emit, &Cancel::new()) {
                    report
                        .errors
                        .push(format!("Could not zip {}: {e}", folder.name));
                    continue;
                }
                // Read the new zip back before trusting it with the only copy.
                let ok = ZipReader::file(target.clone())
                    .list()
                    .map(|l| compare(&files, &l).complete())
                    .unwrap_or(false);
                if !ok {
                    report.errors.push(format!(
                        "{}.zip did not check out against the folder, so the folder was kept.",
                        folder.name
                    ));
                    continue;
                }
                report.zipped.push(folder.name.clone());
            }
            if !libs.contains(&folder.library_id) {
                libs.push(folder.library_id);
            }
            if delete_after {
                planned.push(Planned {
                    target: Target::Container(folder.id),
                    label: folder.name.clone(),
                    library_id: folder.library_id,
                    paths: vec![dir],
                });
            }
        }
        let _ = app.emit(
            "pack-progress",
            PackProgress {
                label: String::new(),
                done: 1,
                total: 1,
            },
        );
        if !planned.is_empty() {
            let mut d = execute(st, planned)?;
            report.deleted = d.deleted;
            report.errors.append(&mut d.errors);
        }
        // Pick up the new zips (they wait out the quiet period like any new archive).
        for lib in libs {
            app.state::<AppState>().scans.request(app, lib);
        }
        if report.zipped.is_empty() && report.already.is_empty() && report.errors.len() == 1 {
            return Err(AppError::Invalid(report.errors.remove(0)));
        }
        Ok(report)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_folders_with_same_named_archives() {
        let conn = crate::db::test_conn();
        conn.execute_batch(
            "INSERT INTO libraries (id, name, path) VALUES (1, 'L', '/l');
             INSERT INTO containers (id, library_id, parent_id, kind, path, name, status) VALUES
               (1, 1, NULL, 'folder', '/l', 'L', 'ok'),
               (2, 1, 1, 'folder', '/l/Ork Boyz', 'Ork Boyz', 'ok'),
               (3, 1, 1, 'zip', '/l/ork boyz.zip', 'ork boyz.zip', 'ok'),
               (4, 1, 1, 'folder', '/l/Elves', 'Elves', 'ok'),
               (5, 1, 1, 'rar', '/l/Trees.part1.rar', 'Trees.rar', 'missing_parts'),
               (6, 1, 1, 'folder', '/l/Trees', 'Trees', 'ok');",
        )
        .expect("seed");
        assert_eq!(candidates(&conn, None).expect("pairs"), vec![(2, 3)]);
        assert_eq!(candidates(&conn, Some(4)).expect("pairs"), vec![]);
        assert_eq!(candidates(&conn, Some(1)).expect("pairs"), vec![(2, 3)]);
    }
}
