use std::collections::BTreeMap;

use rusqlite::{params, Connection};
use tauri::AppHandle;

use super::blocking;
use crate::db::queries::{self, CONTAINER_COLS, MODEL_COLS};
use crate::error::AppResult;
use crate::orientation;
use crate::types::{BrowseResult, ContainerInfo, ModelDetail, ModelInfo, Rotation, VirtualDir};

pub fn browse_impl(conn: &Connection, container_id: i64, subpath: &str) -> AppResult<BrowseResult> {
    let container = queries::get_container(conn, container_id)?;
    let subpath = subpath.trim_matches('/').to_string();
    let mut dirs = Vec::new();
    let containers: Vec<ContainerInfo>;
    let models: Vec<ModelInfo>;

    if container.kind == "folder" {
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT {CONTAINER_COLS} FROM containers c
             WHERE c.parent_id = ?1 AND c.missing = 0 AND (c.model_count > 0 OR c.issue_count > 0)
             ORDER BY c.kind <> 'folder', c.name COLLATE NOCASE"
        ))?;
        containers = stmt
            .query_map([container_id], queries::container_from_row)?
            .collect::<Result<_, _>>()?;
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT {MODEL_COLS} FROM models m WHERE m.container_id = ?1 AND m.missing = 0
             ORDER BY m.name COLLATE NOCASE"
        ))?;
        models = stmt
            .query_map([container_id], queries::model_from_row)?
            .collect::<Result<_, _>>()?;
    } else {
        // Archive folders are virtual: derive them from entry paths.
        let prefix = if subpath.is_empty() {
            String::new()
        } else {
            format!("{subpath}/")
        };
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT {MODEL_COLS} FROM models m WHERE m.container_id = ?1 AND m.missing = 0
             ORDER BY m.rel_path COLLATE NOCASE"
        ))?;
        let all: Vec<ModelInfo> = stmt
            .query_map([container_id], queries::model_from_row)?
            .collect::<Result<_, _>>()?;
        let mut here = Vec::new();
        let mut sub: BTreeMap<String, (i64, bool)> = BTreeMap::new();
        for m in all {
            let Some(rest) = m.rel_path.strip_prefix(&prefix) else {
                continue;
            };
            match rest.split_once('/') {
                Some((dir, deeper)) => {
                    let e = sub.entry(dir.to_string()).or_insert((0, false));
                    e.0 += 1;
                    e.1 |= deeper.contains('/');
                }
                None => here.push(m),
            }
        }
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT {CONTAINER_COLS} FROM containers c WHERE c.parent_id = ?1 AND c.missing = 0
             ORDER BY c.name COLLATE NOCASE"
        ))?;
        let nested: Vec<ContainerInfo> = stmt
            .query_map([container_id], queries::container_from_row)?
            .collect::<Result<_, _>>()?;
        let mut kids = Vec::new();
        for c in nested {
            let entry = c.entry_path.clone().unwrap_or_default();
            let parent = entry.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
            if parent == subpath {
                kids.push(c);
            } else if let Some(rest) = entry.strip_prefix(&prefix) {
                if let Some((dir, _)) = rest.split_once('/') {
                    let e = sub.entry(dir.to_string()).or_insert((0, false));
                    e.0 += c.model_count;
                    e.1 = true;
                }
            }
        }
        dirs = sub
            .into_iter()
            .map(|(name, (count, deeper))| VirtualDir {
                path: format!("{prefix}{name}"),
                name,
                model_count: count,
                has_children: deeper,
            })
            .collect();
        containers = kids;
        models = here;
    }

    Ok(BrowseResult {
        breadcrumbs: queries::breadcrumbs(conn, container_id, &subpath)?,
        container,
        subpath,
        dirs,
        containers,
        models,
    })
}

#[tauri::command]
pub async fn browse(
    app: AppHandle,
    container_id: i64,
    subpath: Option<String>,
) -> AppResult<BrowseResult> {
    blocking(app, move |_, st| {
        browse_impl(
            &*st.db.lock()?,
            container_id,
            subpath.as_deref().unwrap_or(""),
        )
    })
    .await
}

pub fn model_detail_impl(conn: &Connection, id: i64) -> AppResult<ModelDetail> {
    let model = queries::get_model(conn, id)?;
    let container = queries::get_container(conn, model.container_id)?;
    let (rotation, pack) = orientation::resolve_for_model(conn, model.rotation, container.id)?;
    let subpath = if container.kind == "folder" {
        String::new()
    } else {
        model
            .rel_path
            .rsplit_once('/')
            .map(|(d, _)| d.to_string())
            .unwrap_or_default()
    };
    let library_name: String = conn.query_row(
        "SELECT name FROM libraries WHERE id = ?1",
        [model.library_id],
        |r| r.get(0),
    )?;
    Ok(ModelDetail {
        breadcrumbs: queries::breadcrumbs(conn, container.id, &subpath)?,
        model,
        container,
        pack,
        rotation,
        library_name,
    })
}

#[tauri::command]
pub async fn model_detail(app: AppHandle, id: i64) -> AppResult<ModelDetail> {
    blocking(app, move |_, st| model_detail_impl(&*st.db.lock()?, id)).await
}

#[tauri::command]
pub async fn container_detail(app: AppHandle, id: i64) -> AppResult<ContainerInfo> {
    blocking(app, move |_, st| {
        queries::get_container(&*st.db.lock()?, id)
    })
    .await
}

#[tauri::command]
pub async fn set_model_rotation(
    app: AppHandle,
    model_ids: Vec<i64>,
    rotation: Option<Rotation>,
) -> AppResult<()> {
    blocking(app, move |_, st| {
        let mut conn = st.db.lock()?;
        let tx = conn.transaction()?;
        let r = rotation.map(orientation::normalise);
        for id in model_ids {
            tx.execute(
                "UPDATE models SET rot_x = ?2, rot_y = ?3, rot_z = ?4 WHERE id = ?1",
                params![id, r.map(|r| r.x), r.map(|r| r.y), r.map(|r| r.z)],
            )?;
        }
        tx.commit()?;
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn set_pack_rotation(
    app: AppHandle,
    container_id: i64,
    rotation: Option<Rotation>,
) -> AppResult<()> {
    blocking(app, move |_, st| {
        let r = rotation.map(orientation::normalise);
        st.db.lock()?.execute(
            "UPDATE containers SET rot_x = ?2, rot_y = ?3, rot_z = ?4 WHERE id = ?1",
            params![container_id, r.map(|r| r.x), r.map(|r| r.y), r.map(|r| r.z)],
        )?;
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn set_pack(app: AppHandle, container_ids: Vec<i64>, is_pack: bool) -> AppResult<()> {
    blocking(app, move |_, st| {
        let conn = st.db.lock()?;
        for id in container_ids {
            conn.execute(
                "UPDATE containers SET is_pack = ?2 WHERE id = ?1 AND kind = 'folder'",
                params![id, is_pack],
            )?;
        }
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_folders_are_virtual() {
        let conn = crate::db::test_conn();
        conn.execute_batch(
            "INSERT INTO libraries (id, name, path) VALUES (1, 'L', '/l');
             INSERT INTO containers (id, library_id, parent_id, kind, path, name) VALUES
               (1, 1, NULL, 'folder', '/l', 'L'),
               (2, 1, 1, 'zip', '/l/p.zip', 'p.zip');
             INSERT INTO models (container_id, library_id, rel_path, name, ext) VALUES
               (2, 1, 'top.stl', 'top.stl', 'stl'),
               (2, 1, 'a/one.stl', 'one.stl', 'stl'),
               (2, 1, 'a/b/two.stl', 'two.stl', 'stl'),
               (2, 1, 'c/three.stl', 'three.stl', 'stl');",
        )
        .expect("seed");
        let root = browse_impl(&conn, 2, "").expect("browse");
        assert_eq!(root.models.len(), 1);
        assert_eq!(root.dirs.len(), 2);
        assert_eq!(root.dirs[0].name, "a");
        assert_eq!(root.dirs[0].model_count, 2);
        assert!(root.dirs[0].has_children);
        assert!(!root.dirs[1].has_children);

        let a = browse_impl(&conn, 2, "a").expect("browse a");
        assert_eq!(a.models.len(), 1);
        assert_eq!(a.dirs.len(), 1);
        assert_eq!(a.dirs[0].path, "a/b");
        assert_eq!(a.breadcrumbs.last().map(|c| c.name.as_str()), Some("a"));
    }
}
