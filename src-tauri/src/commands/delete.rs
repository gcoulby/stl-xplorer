//! Deleting files and packs from disk. Everything goes to the Recycle Bin (or the Bin on
//! macOS) rather than being erased, so a slip can be undone from the operating system.

use std::path::PathBuf;

use rusqlite::Connection;
use tauri::AppHandle;

use super::blocking;
use crate::db::queries;
use crate::error::{AppError, AppResult};
use crate::types::DeleteReport;

#[derive(Debug, PartialEq)]
pub enum Target {
    Model(i64),
    Container(i64),
}

#[derive(Debug)]
pub struct Planned {
    pub target: Target,
    pub label: String,
    pub library_id: i64,
    /// Every file or folder that goes. All parts, for a split archive.
    pub paths: Vec<PathBuf>,
}

/// Work out what each selected item means on disk, refusing the things that cannot be deleted
/// on their own.
pub fn plan(
    conn: &Connection,
    model_ids: &[i64],
    container_ids: &[i64],
) -> AppResult<(Vec<Planned>, Vec<String>)> {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    for &id in model_ids {
        let m = queries::get_model(conn, id)?;
        if m.in_archive {
            errors.push(format!(
                "{} is inside an archive. Delete the whole archive instead.",
                m.name
            ));
            continue;
        }
        out.push(Planned {
            target: Target::Model(id),
            label: m.name,
            library_id: m.library_id,
            paths: vec![PathBuf::from(m.rel_path)],
        });
    }
    for &id in container_ids {
        let c = queries::get_container(conn, id)?;
        if c.parent_id.is_none() {
            errors.push(format!(
                "{} is a library. Remove it in Settings → Libraries instead.",
                c.name
            ));
            continue;
        }
        if c.entry_path.is_some() {
            errors.push(format!(
                "{} is inside another archive. Delete the outer archive instead.",
                c.name
            ));
            continue;
        }
        let part_paths: Option<String> = conn.query_row(
            "SELECT part_paths FROM containers WHERE id = ?1",
            [id],
            |r| r.get(0),
        )?;
        let paths = match part_paths.and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok()) {
            Some(parts) if !parts.is_empty() => parts.into_iter().map(PathBuf::from).collect(),
            _ => vec![PathBuf::from(&c.path)],
        };
        out.push(Planned {
            target: Target::Container(id),
            label: c.name,
            library_id: c.library_id,
            paths,
        });
    }
    Ok((out, errors))
}

/// Flag what was deleted as missing, the same way a rescan would, and return the affected
/// models and archives so search rows and caches can be refreshed.
pub fn mark_deleted(conn: &Connection, target: &Target) -> AppResult<(Vec<i64>, Vec<i64>)> {
    match target {
        Target::Model(id) => {
            conn.execute("UPDATE models SET missing = 1 WHERE id = ?1", [id])?;
            Ok((vec![*id], Vec::new()))
        }
        Target::Container(id) => {
            let mut stmt = conn.prepare(
                "WITH RECURSIVE sub(id) AS (
                   SELECT ?1 UNION ALL
                   SELECT c.id FROM containers c JOIN sub ON c.parent_id = sub.id
                 ) SELECT id FROM sub",
            )?;
            let ids: Vec<i64> = stmt
                .query_map([id], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            let mut models = Vec::new();
            for cid in &ids {
                conn.execute("UPDATE containers SET missing = 1 WHERE id = ?1", [cid])?;
                models.extend(queries::model_ids_in_container(conn, *cid)?);
                conn.execute(
                    "UPDATE models SET missing = 1 WHERE container_id = ?1",
                    [cid],
                )?;
            }
            Ok((models, ids))
        }
    }
}

#[tauri::command]
pub async fn delete_items(
    app: AppHandle,
    model_ids: Vec<i64>,
    container_ids: Vec<i64>,
) -> AppResult<DeleteReport> {
    blocking(app, move |_, st| {
        let (planned, mut errors) = plan(&*st.db.lock()?, &model_ids, &container_ids)?;
        let mut deleted = Vec::new();
        let mut done = Vec::new();
        for p in planned {
            // Parts already gone are fine; the rest go to the bin together.
            let existing: Vec<&PathBuf> = p.paths.iter().filter(|x| x.exists()).collect();
            if !existing.is_empty() {
                if let Err(e) = trash::delete_all(&existing) {
                    errors.push(format!("Could not delete {}: {e}", p.label));
                    continue;
                }
            }
            deleted.push(p.label.clone());
            done.push(p);
        }
        if !done.is_empty() {
            let mut conn = st.db.lock()?;
            let tx = conn.transaction()?;
            let mut models = Vec::new();
            let mut archives = Vec::new();
            let mut libs: Vec<i64> = Vec::new();
            for p in &done {
                let (m, c) = mark_deleted(&tx, &p.target)?;
                models.extend(m);
                archives.extend(c);
                if !libs.contains(&p.library_id) {
                    libs.push(p.library_id);
                }
            }
            for lib in libs {
                queries::recount_library(&tx, lib)?;
            }
            queries::refresh_fts(&tx, &models)?;
            tx.commit()?;
            st.invalidate_archives(&archives);
        }
        if deleted.is_empty() && !errors.is_empty() && errors.len() == 1 {
            return Err(AppError::Invalid(errors.remove(0)));
        }
        Ok(DeleteReport { deleted, errors })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed() -> Connection {
        let conn = crate::db::test_conn();
        conn.execute_batch(
            r#"INSERT INTO libraries (id, name, path) VALUES (1, 'L', '/l');
             INSERT INTO containers (id, library_id, parent_id, kind, path, part_paths, entry_path, name) VALUES
               (1, 1, NULL, 'folder', '/l', NULL, NULL, 'L'),
               (2, 1, 1, 'folder', '/l/Pack', NULL, NULL, 'Pack'),
               (3, 1, 2, 'rar', '/l/Pack/Big.part1.rar', '["/l/Pack/Big.part1.rar","/l/Pack/Big.part2.rar"]', NULL, 'Big.rar'),
               (4, 1, 2, 'zip', '/l/Pack/p.zip', NULL, NULL, 'p.zip'),
               (5, 1, 4, 'zip', '/l/Pack/p.zip::in.zip', NULL, 'in.zip', 'in.zip');
             INSERT INTO models (id, container_id, library_id, rel_path, name, ext) VALUES
               (1, 2, 1, '/l/Pack/loose.stl', 'loose.stl', 'stl'),
               (2, 3, 1, 'a.stl', 'a.stl', 'stl'),
               (3, 5, 1, 'deep.stl', 'deep.stl', 'stl');"#,
        )
        .expect("seed");
        conn
    }

    #[test]
    fn plans_whole_sets_and_refuses_what_cannot_go_alone() {
        let conn = seed();
        let (plan, errors) = plan(&conn, &[1, 2], &[3, 5, 1]).expect("plan");
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].paths, vec![PathBuf::from("/l/Pack/loose.stl")]);
        assert_eq!(
            plan[1].paths,
            vec![
                PathBuf::from("/l/Pack/Big.part1.rar"),
                PathBuf::from("/l/Pack/Big.part2.rar")
            ],
            "every part of a split archive goes together"
        );
        // A file inside an archive, a nested archive and a library root are all refused.
        assert_eq!(errors.len(), 3);
    }

    #[test]
    fn deleting_a_folder_flags_everything_below_it() {
        let conn = seed();
        let (models, containers) = mark_deleted(&conn, &Target::Container(2)).expect("mark");
        assert_eq!(containers.len(), 4);
        assert_eq!(models.len(), 3);
        let live: i64 = conn
            .query_row("SELECT COUNT(*) FROM models WHERE missing = 0", [], |r| {
                r.get(0)
            })
            .expect("count");
        assert_eq!(live, 0);
    }
}
