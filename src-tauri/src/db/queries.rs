use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::{AppError, AppResult};
use crate::types::{ContainerInfo, Crumb, Library, ModelInfo, Rotation};
use crate::util::MESH_EXTS_SQL;

fn parse_ids(s: Option<String>) -> Vec<i64> {
    s.map(|s| s.split(',').filter_map(|p| p.parse().ok()).collect())
        .unwrap_or_default()
}

fn rotation(x: Option<f64>, y: Option<f64>, z: Option<f64>) -> Option<Rotation> {
    match (x, y, z) {
        (None, None, None) => None,
        (x, y, z) => Some(Rotation {
            x: x.unwrap_or(0.0),
            y: y.unwrap_or(0.0),
            z: z.unwrap_or(0.0),
        }),
    }
}

pub const CONTAINER_COLS: &str =
    "c.id, c.library_id, c.parent_id, c.kind, c.name, c.path, c.entry_path,
  COALESCE(json_array_length(c.part_paths), 1), c.size, c.mtime, c.is_pack, c.solid, c.encrypted,
  c.status, c.status_detail, c.missing, c.vaulted, c.original_path, c.model_count, c.issue_count,
  CASE WHEN c.kind = 'folder' THEN
    EXISTS (SELECT 1 FROM containers k WHERE k.parent_id = c.id AND k.missing = 0
            AND (k.file_count > 0 OR k.issue_count > 0))
  ELSE
    EXISTS (SELECT 1 FROM models m WHERE m.container_id = c.id AND m.missing = 0
            AND instr(m.rel_path, '/') > 0)
    OR EXISTS (SELECT 1 FROM containers k WHERE k.parent_id = c.id AND k.missing = 0)
  END,
  (SELECT group_concat(tag_id) FROM container_tags WHERE container_id = c.id),
  c.rot_x, c.rot_y, c.rot_z, c.file_count";

pub fn container_from_row(r: &Row) -> rusqlite::Result<ContainerInfo> {
    Ok(ContainerInfo {
        id: r.get(0)?,
        library_id: r.get(1)?,
        parent_id: r.get(2)?,
        kind: r.get(3)?,
        name: r.get(4)?,
        path: r.get(5)?,
        entry_path: r.get(6)?,
        part_count: r.get(7)?,
        size: r.get(8)?,
        mtime: r.get(9)?,
        is_pack: r.get(10)?,
        solid: r.get(11)?,
        encrypted: r.get(12)?,
        status: r.get(13)?,
        status_detail: r.get(14)?,
        missing: r.get(15)?,
        vaulted: r.get(16)?,
        original_path: r.get(17)?,
        model_count: r.get(18)?,
        issue_count: r.get(19)?,
        has_children: r.get(20)?,
        tag_ids: parse_ids(r.get(21)?),
        rotation: rotation(r.get(22)?, r.get(23)?, r.get(24)?),
        file_count: r.get(25)?,
    })
}

pub const MODEL_COLS: &str =
    "m.id, m.container_id, m.library_id, m.name, m.ext, m.rel_path, m.size,
  m.mtime, m.encrypted, m.missing, m.vaulted,
  (SELECT kind FROM containers WHERE id = m.container_id) <> 'folder',
  m.original_path,
  (SELECT group_concat(tag_id) FROM model_tags WHERE model_id = m.id),
  m.rot_x, m.rot_y, m.rot_z";

pub fn model_from_row(r: &Row) -> rusqlite::Result<ModelInfo> {
    Ok(ModelInfo {
        id: r.get(0)?,
        container_id: r.get(1)?,
        library_id: r.get(2)?,
        name: r.get(3)?,
        ext: r.get(4)?,
        rel_path: r.get(5)?,
        size: r.get(6)?,
        mtime: r.get(7)?,
        encrypted: r.get(8)?,
        missing: r.get(9)?,
        vaulted: r.get(10)?,
        in_archive: r.get(11)?,
        original_path: r.get(12)?,
        tag_ids: parse_ids(r.get(13)?),
        rotation: rotation(r.get(14)?, r.get(15)?, r.get(16)?),
    })
}

pub fn get_container(conn: &Connection, id: i64) -> AppResult<ContainerInfo> {
    conn.query_row(
        &format!("SELECT {CONTAINER_COLS} FROM containers c WHERE c.id = ?1"),
        [id],
        container_from_row,
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("container {id}")))
}

pub fn get_model(conn: &Connection, id: i64) -> AppResult<ModelInfo> {
    conn.query_row(
        &format!("SELECT {MODEL_COLS} FROM models m WHERE m.id = ?1"),
        [id],
        model_from_row,
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("model {id}")))
}

pub fn list_libraries(conn: &Connection) -> AppResult<Vec<Library>> {
    let mut stmt = conn.prepare(
        "SELECT l.id, l.name, l.path, l.enabled, l.is_vault, l.last_scanned_at,
                c.id, COALESCE(c.model_count, 0), COALESCE(c.issue_count, 0)
         FROM libraries l
         LEFT JOIN containers c ON c.library_id = l.id AND c.parent_id IS NULL AND c.kind = 'folder'
         ORDER BY l.is_vault DESC, l.sort_order, l.name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| {
        let path: String = r.get(2)?;
        Ok(Library {
            id: r.get(0)?,
            name: r.get(1)?,
            path_exists: std::path::Path::new(&path).is_dir(),
            path,
            enabled: r.get(3)?,
            is_vault: r.get(4)?,
            last_scanned_at: r.get(5)?,
            root_container_id: r.get(6)?,
            model_count: r.get(7)?,
            issue_count: r.get(8)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get_library(conn: &Connection, id: i64) -> AppResult<Library> {
    list_libraries(conn)?
        .into_iter()
        .find(|l| l.id == id)
        .ok_or_else(|| AppError::NotFound(format!("library {id}")))
}

/// Root-to-container chain, plus one crumb per directory inside an archive.
pub fn breadcrumbs(conn: &Connection, container_id: i64, subpath: &str) -> AppResult<Vec<Crumb>> {
    let mut chain = Vec::new();
    let mut next = Some(container_id);
    while let Some(id) = next {
        let (parent, name, kind, lib_name): (Option<i64>, String, String, String) = conn
            .query_row(
                "SELECT c.parent_id, c.name, c.kind, l.name FROM containers c
             JOIN libraries l ON l.id = c.library_id WHERE c.id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?;
        let name = if parent.is_none() { lib_name } else { name };
        chain.push(Crumb {
            container_id: id,
            subpath: String::new(),
            name,
            kind,
        });
        next = parent;
        if chain.len() > 512 {
            break;
        }
    }
    chain.reverse();
    if !subpath.is_empty() {
        let mut acc = String::new();
        for part in subpath.split('/') {
            if !acc.is_empty() {
                acc.push('/');
            }
            acc.push_str(part);
            chain.push(Crumb {
                container_id,
                subpath: acc.clone(),
                name: part.to_string(),
                kind: "dir".into(),
            });
        }
    }
    Ok(chain)
}

pub fn get_setting(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn delete_setting(conn: &Connection, key: &str) -> AppResult<()> {
    conn.execute("DELETE FROM settings WHERE key = ?1", [key])?;
    Ok(())
}

/// Rebuild the search rows for the given models.
pub fn refresh_fts(conn: &Connection, model_ids: &[i64]) -> AppResult<()> {
    let mut del = conn.prepare_cached("DELETE FROM models_fts WHERE rowid = ?1")?;
    let mut ins = conn.prepare_cached(
        "INSERT INTO models_fts (rowid, name, container_name, path, tags)
         SELECT m.id, m.name, c.name,
           CASE WHEN c.kind = 'folder' THEN m.rel_path ELSE c.path || '/' || m.rel_path END,
           COALESCE((SELECT group_concat(t.name, ' ') FROM model_tags mt
                     JOIN tags t ON t.id = mt.tag_id WHERE mt.model_id = m.id), '') || ' ' ||
           COALESCE((SELECT group_concat(t.name, ' ') FROM container_tags ct
                     JOIN tags t ON t.id = ct.tag_id WHERE ct.container_id = m.container_id), '')
         FROM models m JOIN containers c ON c.id = m.container_id
         WHERE m.id = ?1",
    )?;
    for id in model_ids {
        del.execute([id])?;
        ins.execute([id])?;
    }
    Ok(())
}

pub fn model_ids_in_container(conn: &Connection, container_id: i64) -> AppResult<Vec<i64>> {
    let mut stmt = conn.prepare("SELECT id FROM models WHERE container_id = ?1")?;
    let ids = stmt
        .query_map([container_id], |r| r.get(0))?
        .collect::<Result<Vec<i64>, _>>()?;
    Ok(ids)
}

/// Recompute `model_count` and `issue_count` for every container in a library, bottom up.
pub fn recount_library(conn: &Connection, library_id: i64) -> AppResult<()> {
    use std::collections::HashMap;
    struct Node {
        parent: Option<i64>,
        missing: bool,
        issue: bool,
        direct: i64,
        files: i64,
        old: (i64, i64, i64),
    }
    let mut nodes: HashMap<i64, Node> = HashMap::new();
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT c.id, c.parent_id, c.missing,
                    (c.kind <> 'folder' AND (c.status <> 'ok' OR c.encrypted = 1)),
                    (SELECT COUNT(*) FROM models m WHERE m.container_id = c.id AND m.missing = 0
                       AND m.ext IN {MESH_EXTS_SQL}),
                    (SELECT COUNT(*) FROM models m WHERE m.container_id = c.id AND m.missing = 0),
                    c.model_count, c.issue_count, c.file_count
             FROM containers c WHERE c.library_id = ?1"
        ))?;
        let rows = stmt.query_map([library_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                Node {
                    parent: r.get(1)?,
                    missing: r.get(2)?,
                    issue: r.get(3)?,
                    direct: r.get(4)?,
                    files: r.get(5)?,
                    old: (r.get(6)?, r.get(7)?, r.get(8)?),
                },
            ))
        })?;
        for row in rows {
            let (id, n) = row?;
            nodes.insert(id, n);
        }
    }
    let mut totals: HashMap<i64, (i64, i64, i64)> = HashMap::new();
    for (&id, n) in &nodes {
        if n.missing {
            continue;
        }
        let add = (n.direct, i64::from(n.issue), n.files);
        let mut cur = Some(id);
        let mut hops = 0;
        while let Some(c) = cur {
            let e = totals.entry(c).or_insert((0, 0, 0));
            e.0 += add.0;
            e.1 += add.1;
            e.2 += add.2;
            cur = nodes.get(&c).and_then(|n| n.parent);
            hops += 1;
            if hops > 512 {
                break;
            }
        }
    }
    let mut upd = conn.prepare_cached(
        "UPDATE containers SET model_count = ?2, issue_count = ?3, file_count = ?4 WHERE id = ?1",
    )?;
    for (id, n) in &nodes {
        let t = totals.get(id).copied().unwrap_or((0, 0, 0));
        if t != n.old {
            upd.execute(params![id, t.0, t.1, t.2])?;
        }
    }
    Ok(())
}
