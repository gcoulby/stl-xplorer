use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use tauri::AppHandle;

use super::blocking;
use crate::db::queries;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::types::VaultReport;
use crate::util::{numbered_name, stem_len};
use crate::vault::{move_all, plan_targets};

struct Vault {
    library_id: i64,
    root_id: i64,
    path: PathBuf,
}

fn vault(conn: &Connection) -> AppResult<Vault> {
    let (library_id, path, name): (i64, String, String) = conn.query_row(
        "SELECT id, path, name FROM libraries WHERE is_vault = 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    std::fs::create_dir_all(&path)?;
    let root_id = conn.query_row(
        "INSERT INTO containers (library_id, parent_id, kind, path, name)
         VALUES (?1, NULL, 'folder', ?2, ?3)
         ON CONFLICT (library_id, path) DO UPDATE SET missing = 0
         RETURNING id",
        params![library_id, path, name],
        |r| r.get(0),
    )?;
    Ok(Vault {
        library_id,
        root_id,
        path: PathBuf::from(path),
    })
}

/// The outermost archive a container sits in, so a nested zip saves its parent file.
fn top_archive(conn: &Connection, mut id: i64) -> AppResult<i64> {
    for _ in 0..64 {
        let c = queries::get_container(conn, id)?;
        match c.parent_id {
            Some(p) if c.entry_path.is_some() => id = p,
            _ => return Ok(id),
        }
    }
    Ok(id)
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn save_file(conn: &mut Connection, v: &Vault, model_id: i64) -> AppResult<String> {
    let m = queries::get_model(conn, model_id)?;
    let src = PathBuf::from(&m.rel_path);
    let name = file_name(&src);
    let (targets, _) = plan_targets(std::slice::from_ref(&src), &v.path, stem_len(&name))?;
    let dst = targets[0].clone();
    move_all(&[(src.clone(), dst.clone())])?;
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE models SET container_id = ?2, library_id = ?3, rel_path = ?4, name = ?5,
           vaulted = 1, original_path = COALESCE(original_path, ?6)
         WHERE id = ?1",
        params![
            model_id,
            v.root_id,
            v.library_id,
            dst.to_string_lossy(),
            file_name(&dst),
            m.rel_path
        ],
    )?;
    queries::refresh_fts(&tx, &[model_id])?;
    tx.commit()?;
    Ok(name)
}

fn save_archive(conn: &mut Connection, v: &Vault, container_id: i64) -> AppResult<String> {
    let c = queries::get_container(conn, container_id)?;
    let part_paths: Option<String> = conn.query_row(
        "SELECT part_paths FROM containers WHERE id = ?1",
        [container_id],
        |r| r.get(0),
    )?;
    let parts: Vec<PathBuf> = match &part_paths {
        Some(json) => serde_json::from_str::<Vec<String>>(json)?
            .into_iter()
            .map(PathBuf::from)
            .collect(),
        None => vec![PathBuf::from(&c.path)],
    };
    if c.status == "missing_parts" {
        return Err(AppError::Invalid(format!(
            "{} has missing parts. Wait until every part has downloaded.",
            c.name
        )));
    }
    // Every part shares the set's stem (`Pack` in `Pack.part2.rar`), so a collision suffix
    // lands in the same place on each one and the set still opens.
    let lower = c.name.to_lowercase();
    let stem = lower
        .strip_suffix(&format!(".{}", c.kind))
        .map(str::len)
        .unwrap_or(c.name.len());
    let consistent = parts.iter().all(|p| {
        file_name(p)
            .to_lowercase()
            .starts_with(&lower[..stem.min(lower.len())])
    });
    let stem = if consistent || parts.len() == 1 {
        if parts.len() == 1 {
            stem_len(&file_name(&parts[0]))
        } else {
            stem
        }
    } else {
        return Err(AppError::Invalid(format!(
            "The parts of {} do not share a name, so they cannot be moved safely",
            c.name
        )));
    };
    let (targets, n) = plan_targets(&parts, &v.path, stem)?;
    let pairs: Vec<(PathBuf, PathBuf)> =
        parts.iter().cloned().zip(targets.iter().cloned()).collect();
    move_all(&pairs)?;

    let new_entry = pairs
        .iter()
        .find(|(s, _)| s.to_string_lossy() == c.path)
        .map(|(_, d)| d.clone())
        .unwrap_or_else(|| targets[0].clone());
    let new_parts = part_paths
        .as_ref()
        .map(|_| {
            serde_json::to_string(
                &targets
                    .iter()
                    .map(|t| t.to_string_lossy().into_owned())
                    .collect::<Vec<_>>(),
            )
        })
        .transpose()?;
    let new_name = match n {
        Some(n) if parts.len() > 1 => numbered_name(&c.name, stem, n),
        Some(_) => file_name(&new_entry),
        None => c.name.clone(),
    };
    let new_path = new_entry.to_string_lossy().into_owned();

    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE containers SET library_id = ?2, parent_id = ?3, path = ?4, part_paths = ?5, name = ?6,
           vaulted = 1, original_path = COALESCE(original_path, ?7)
         WHERE id = ?1",
        params![container_id, v.library_id, v.root_id, new_path, new_parts, new_name, c.path],
    )?;
    tx.execute(
        "UPDATE containers SET library_id = ?2, vaulted = 1, path = ?3 || '::' || entry_path
         WHERE parent_id = ?1 AND entry_path IS NOT NULL",
        params![container_id, v.library_id, new_path],
    )?;
    tx.execute(
        "UPDATE models SET library_id = ?2, vaulted = 1
         WHERE container_id = ?1 OR container_id IN (SELECT id FROM containers WHERE parent_id = ?1)",
        params![container_id, v.library_id],
    )?;
    let mut ids = queries::model_ids_in_container(&tx, container_id)?;
    let kids: Vec<i64> = {
        let mut stmt = tx.prepare("SELECT id FROM containers WHERE parent_id = ?1")?;
        let rows = stmt.query_map([container_id], |r| r.get(0))?;
        rows.collect::<Result<_, _>>()?
    };
    for k in kids {
        ids.extend(queries::model_ids_in_container(&tx, k)?);
    }
    queries::refresh_fts(&tx, &ids)?;
    tx.commit()?;
    Ok(c.name)
}

pub fn save_impl(
    st: &AppState,
    model_ids: &[i64],
    container_ids: &[i64],
) -> AppResult<VaultReport> {
    let mut conn = st.db.lock()?;
    let v = vault(&conn)?;
    let mut files = BTreeSet::new();
    let mut archives = BTreeSet::new();
    let mut errors = Vec::new();
    for &id in model_ids {
        let m = queries::get_model(&conn, id)?;
        if m.library_id == v.library_id {
            continue;
        }
        let c = queries::get_container(&conn, m.container_id)?;
        if c.kind == "folder" {
            files.insert(id);
        } else {
            archives.insert(top_archive(&conn, c.id)?);
        }
    }
    for &id in container_ids {
        let c = queries::get_container(&conn, id)?;
        if c.library_id == v.library_id {
            continue;
        }
        if c.kind == "folder" {
            errors.push(format!(
                "{}: folders cannot be saved to the vault yet",
                c.name
            ));
        } else {
            archives.insert(top_archive(&conn, id)?);
        }
    }
    let mut libs: BTreeSet<i64> = BTreeSet::new();
    let mut moved = Vec::new();
    for id in files {
        let lib = queries::get_model(&conn, id)?.library_id;
        match save_file(&mut conn, &v, id) {
            Ok(n) => {
                libs.insert(lib);
                moved.push(n);
            }
            Err(e) => errors.push(e.to_string()),
        }
    }
    for id in archives {
        let lib = queries::get_container(&conn, id)?.library_id;
        match save_archive(&mut conn, &v, id) {
            Ok(n) => {
                libs.insert(lib);
                moved.push(n);
            }
            Err(e) => errors.push(e.to_string()),
        }
    }
    libs.insert(v.library_id);
    for l in libs {
        queries::recount_library(&conn, l)?;
    }
    Ok(VaultReport { moved, errors })
}

#[tauri::command]
pub async fn vault_save(
    app: AppHandle,
    model_ids: Vec<i64>,
    container_ids: Vec<i64>,
) -> AppResult<VaultReport> {
    blocking(app, move |_, st| save_impl(st, &model_ids, &container_ids)).await
}
