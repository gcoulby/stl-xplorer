use std::collections::{BTreeMap, HashMap};

use rusqlite::{params, Connection};
use tauri::AppHandle;

use super::blocking;
use crate::db::queries::{self, CONTAINER_COLS, MODEL_COLS};
use crate::error::AppResult;
use crate::orientation;
use crate::types::{
    BrowseResult, ContainerInfo, FlatFiles, ModelDetail, ModelInfo, ProblemArchive,
    ResolvedLocation, Rotation, TreeMatches, VirtualDir,
};
use crate::util::is_mesh_ext;

pub fn browse_impl(conn: &Connection, container_id: i64, subpath: &str) -> AppResult<BrowseResult> {
    let container = queries::get_container(conn, container_id)?;
    let subpath = subpath.trim_matches('/').to_string();
    let mut dirs = Vec::new();
    let containers: Vec<ContainerInfo>;
    let models: Vec<ModelInfo>;

    if container.kind == "folder" {
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT {CONTAINER_COLS} FROM containers c
             WHERE c.parent_id = ?1 AND c.missing = 0 AND (c.file_count > 0 OR c.issue_count > 0)
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
        let prefix = dir_prefix(&subpath);
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT {MODEL_COLS} FROM models m WHERE m.container_id = ?1 AND m.missing = 0
             ORDER BY m.rel_path COLLATE NOCASE"
        ))?;
        let all: Vec<ModelInfo> = stmt
            .query_map([container_id], queries::model_from_row)?
            .collect::<Result<_, _>>()?;
        let mut here = Vec::new();
        // dir name -> (models, files, has deeper dirs)
        let mut sub: BTreeMap<String, (i64, i64, bool)> = BTreeMap::new();
        for m in all {
            let Some(rest) = m.rel_path.strip_prefix(&prefix) else {
                continue;
            };
            match rest.split_once('/') {
                Some((dir, deeper)) => {
                    let e = sub.entry(dir.to_string()).or_insert((0, 0, false));
                    e.0 += i64::from(is_mesh_ext(&m.ext));
                    e.1 += 1;
                    e.2 |= deeper.contains('/');
                }
                None => here.push(m),
            }
        }
        let mut kids = Vec::new();
        for c in nested_containers(conn, container_id)? {
            let entry = c.entry_path.clone().unwrap_or_default();
            let parent = entry.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
            if parent == subpath {
                kids.push(c);
            } else if let Some(rest) = entry.strip_prefix(&prefix) {
                if let Some((dir, _)) = rest.split_once('/') {
                    let e = sub.entry(dir.to_string()).or_insert((0, 0, false));
                    e.0 += c.model_count;
                    e.1 += c.file_count;
                    e.2 = true;
                }
            }
        }
        dirs = sub
            .into_iter()
            .map(|(name, (count, files, deeper))| VirtualDir {
                path: format!("{prefix}{name}"),
                name,
                model_count: count,
                file_count: files,
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
        flat: false,
        locations: Vec::new(),
    })
}

fn dir_prefix(subpath: &str) -> String {
    if subpath.is_empty() {
        String::new()
    } else {
        format!("{subpath}/")
    }
}

fn nested_containers(conn: &Connection, parent_id: i64) -> AppResult<Vec<ContainerInfo>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {CONTAINER_COLS} FROM containers c WHERE c.parent_id = ?1 AND c.missing = 0
         ORDER BY c.name COLLATE NOCASE"
    ))?;
    let rows = stmt.query_map([parent_id], queries::container_from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

const SUBTREE: &str = "WITH RECURSIVE sub(id) AS (
   SELECT ?1
   UNION ALL
   SELECT c.id FROM containers c JOIN sub ON c.parent_id = sub.id WHERE c.missing = 0
 )";

/// Every file below a folder, archive or archive directory, as one list. Each file carries
/// where it sits relative to the browsed location, so a whole pack can be seen at a glance.
pub fn browse_flat_impl(
    conn: &Connection,
    container_id: i64,
    subpath: &str,
) -> AppResult<BrowseResult> {
    let container = queries::get_container(conn, container_id)?;
    let subpath = subpath.trim_matches('/').to_string();
    let prefix = dir_prefix(&subpath);

    // Every live container below this one: parent, name and entry path.
    let mut stmt = conn.prepare_cached(&format!(
        "{SUBTREE} SELECT c.id, c.parent_id, c.name, c.entry_path
         FROM containers c JOIN sub ON sub.id = c.id"
    ))?;
    let mut tree: HashMap<i64, (Option<i64>, String, Option<String>)> = HashMap::new();
    for row in stmt.query_map([container_id], |r| {
        Ok((r.get::<_, i64>(0)?, (r.get(1)?, r.get(2)?, r.get(3)?)))
    })? {
        let (id, v) = row?;
        tree.insert(id, v);
    }

    // Inside an archive directory, nested archives only count if they sit under it.
    let in_scope = |id: i64| -> bool {
        if id == container_id || prefix.is_empty() {
            return true;
        }
        tree.get(&id)
            .and_then(|(_, _, entry)| entry.as_deref())
            .is_some_and(|e| e.starts_with(&prefix))
    };
    let label = |id: i64| -> Vec<String> {
        let mut parts = Vec::new();
        let mut cur = id;
        let mut hops = 0;
        while cur != container_id && hops < 512 {
            let Some((parent, name, _)) = tree.get(&cur) else {
                break;
            };
            parts.push(name.clone());
            match parent {
                Some(p) => cur = *p,
                None => break,
            }
            hops += 1;
        }
        parts.reverse();
        parts
    };

    let mut stmt = conn.prepare_cached(&format!(
        "{SUBTREE} SELECT {MODEL_COLS}, c.kind FROM models m
         JOIN sub ON sub.id = m.container_id
         JOIN containers c ON c.id = m.container_id
         WHERE m.missing = 0"
    ))?;
    let mut rows: Vec<(String, ModelInfo)> = Vec::new();
    let mut labels: HashMap<i64, Vec<String>> = HashMap::new();
    for row in stmt.query_map([container_id], |r| {
        Ok((queries::model_from_row(r)?, r.get::<_, String>(17)?))
    })? {
        let (m, kind) = row?;
        if !in_scope(m.container_id) {
            continue;
        }
        let mut parts = labels
            .entry(m.container_id)
            .or_insert_with(|| label(m.container_id))
            .clone();
        if kind != "folder" {
            let rel = if m.container_id == container_id {
                match m.rel_path.strip_prefix(&prefix) {
                    Some(r) => r,
                    None => continue,
                }
            } else {
                m.rel_path.as_str()
            };
            if let Some((dir, _)) = rel.rsplit_once('/') {
                parts.extend(dir.split('/').map(str::to_string));
            }
        }
        rows.push((parts.join(" › "), m));
    }
    rows.sort_by_cached_key(|(loc, m)| (loc.to_lowercase(), m.name.to_lowercase()));
    let (locations, models) = rows.into_iter().unzip();

    Ok(BrowseResult {
        breadcrumbs: queries::breadcrumbs(conn, container_id, &subpath)?,
        container,
        subpath,
        dirs: Vec::new(),
        containers: Vec::new(),
        models,
        flat: true,
        locations,
    })
}

/// Every file in every pack of a collection. Locations start with the pack's name, and a file
/// reachable through two members (a folder and a pack inside it) is listed once.
pub fn collection_files_impl(conn: &Connection, collection_id: i64) -> AppResult<FlatFiles> {
    let mut stmt = conn.prepare_cached(
        "SELECT c.id, c.name FROM containers c
         JOIN collection_items ci ON ci.container_id = c.id
         WHERE ci.collection_id = ?1 AND c.missing = 0",
    )?;
    let members: Vec<(i64, String)> = stmt
        .query_map([collection_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut seen = std::collections::HashSet::new();
    let mut rows: Vec<(String, ModelInfo)> = Vec::new();
    for (id, name) in members {
        let r = browse_flat_impl(conn, id, "")?;
        for (m, loc) in r.models.into_iter().zip(r.locations) {
            if !seen.insert(m.id) {
                continue;
            }
            let loc = if loc.is_empty() {
                name.clone()
            } else {
                format!("{name} › {loc}")
            };
            rows.push((loc, m));
        }
    }
    rows.sort_by_cached_key(|(loc, m)| (loc.to_lowercase(), m.name.to_lowercase()));
    let (locations, models) = rows.into_iter().unzip();
    Ok(FlatFiles { models, locations })
}

const TREE_MATCH_CAP: usize = 500;

/// Folders and archives in enabled libraries whose name contains `query`, for filtering the
/// sidebar. Done in SQL because the tree only loads what is expanded.
pub fn tree_filter_impl(conn: &Connection, query: &str) -> AppResult<TreeMatches> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(TreeMatches {
            matches: Vec::new(),
            ancestors: Vec::new(),
            truncated: false,
        });
    }
    let pattern = format!(
        "%{}%",
        q.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    let mut stmt = conn.prepare_cached(
        "SELECT c.id FROM containers c JOIN libraries l ON l.id = c.library_id
         WHERE l.enabled = 1 AND c.missing = 0 AND c.parent_id IS NOT NULL
           AND (c.file_count > 0 OR c.issue_count > 0)
           AND c.name LIKE ?1 ESCAPE '\\'
         LIMIT ?2",
    )?;
    let mut matches: Vec<i64> = stmt
        .query_map(params![pattern, TREE_MATCH_CAP as i64 + 1], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let truncated = matches.len() > TREE_MATCH_CAP;
    matches.truncate(TREE_MATCH_CAP);

    let mut ancestors = std::collections::BTreeSet::new();
    let mut stmt = conn.prepare_cached(
        "WITH RECURSIVE up(id) AS (
           SELECT parent_id FROM containers WHERE id = ?1
           UNION
           SELECT c.parent_id FROM containers c JOIN up ON c.id = up.id
         ) SELECT id FROM up WHERE id IS NOT NULL",
    )?;
    for &id in &matches {
        for row in stmt.query_map([id], |r| r.get::<_, i64>(0))? {
            ancestors.insert(row?);
        }
    }
    Ok(TreeMatches {
        matches,
        ancestors: ancestors.into_iter().collect(),
        truncated,
    })
}

#[tauri::command]
pub async fn tree_filter(app: AppHandle, query: String) -> AppResult<TreeMatches> {
    blocking(app, move |_, st| {
        let conn = st.db.lock()?;
        tree_filter_impl(&conn, &query)
    })
    .await
}

#[tauri::command]
pub async fn collection_files(app: AppHandle, id: i64) -> AppResult<FlatFiles> {
    blocking(app, move |_, st| {
        let conn = st.db.lock()?;
        collection_files_impl(&conn, id)
    })
    .await
}

#[tauri::command]
pub async fn browse(
    app: AppHandle,
    container_id: i64,
    subpath: Option<String>,
    flat: Option<bool>,
) -> AppResult<BrowseResult> {
    blocking(app, move |_, st| {
        let conn = st.db.lock()?;
        let sub = subpath.as_deref().unwrap_or("");
        if flat.unwrap_or(false) {
            browse_flat_impl(&conn, container_id, sub)
        } else {
            browse_impl(&conn, container_id, sub)
        }
    })
    .await
}

fn norm(p: &str) -> String {
    p.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

/// Turn a typed path into a place to browse: a folder, an archive, or a folder inside one.
pub fn resolve_location_impl(conn: &Connection, input: &str) -> AppResult<ResolvedLocation> {
    let typed = input.trim().trim_matches('"');
    let want = norm(typed);
    if want.is_empty() {
        return Err(crate::error::AppError::Invalid("Type a path".into()));
    }
    let mut stmt = conn.prepare_cached(
        "SELECT c.id, c.path, c.kind FROM containers c JOIN libraries l ON l.id = c.library_id
         WHERE c.missing = 0 AND l.enabled = 1 AND c.entry_path IS NULL",
    )?;
    let mut best: Option<(i64, String, String)> = None;
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (id, path, kind) = row?;
        let p = norm(&path);
        let fits = want == p || want.starts_with(&format!("{p}/"));
        if fits && best.as_ref().is_none_or(|b| norm(&b.1).len() < p.len()) {
            best = Some((id, path, kind));
        }
    }
    let Some((id, path, kind)) = best else {
        return Err(crate::error::AppError::NotFound(format!(
            "{typed} is not inside any of your libraries"
        )));
    };
    // What is left after the container, in the user's spelling.
    let rest: String = typed
        .replace('\\', "/")
        .chars()
        .skip(norm(&path).chars().count())
        .collect::<String>()
        .trim_matches('/')
        .to_string();
    if rest.is_empty() {
        return Ok(ResolvedLocation {
            container_id: id,
            subpath: String::new(),
        });
    }
    if kind == "folder" {
        return Err(crate::error::AppError::NotFound(format!(
            "No folder called {rest} was indexed there"
        )));
    }
    // Inside an archive: find the directory with the archive's own capitalisation.
    let want_dir = format!("{}/", rest.to_lowercase());
    let mut stmt =
        conn.prepare_cached("SELECT rel_path FROM models WHERE container_id = ?1 AND missing = 0")?;
    for row in stmt.query_map([id], |r| r.get::<_, String>(0))? {
        let rel = row?;
        if rel.to_lowercase().starts_with(&want_dir) {
            return Ok(ResolvedLocation {
                container_id: id,
                subpath: rel[..want_dir.len() - 1].to_string(),
            });
        }
    }
    Err(crate::error::AppError::NotFound(format!(
        "No folder called {rest} in that archive"
    )))
}

#[tauri::command]
pub async fn resolve_location(app: AppHandle, path: String) -> AppResult<ResolvedLocation> {
    blocking(app, move |_, st| {
        resolve_location_impl(&*st.db.lock()?, &path)
    })
    .await
}

fn file_name(p: &str) -> String {
    std::path::Path::new(p)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string())
}

const PROBLEM_COLS: &str = "c.part_paths, l.name,
  (SELECT p.path FROM containers p WHERE p.id = c.parent_id)";

fn problem_from_row(r: &rusqlite::Row) -> rusqlite::Result<ProblemArchive> {
    let container = queries::container_from_row(r)?;
    let part_paths: Option<String> = r.get(26)?;
    let library_name: String = r.get(27)?;
    let folder: Option<String> = r.get(28)?;
    let parts = match part_paths.and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok()) {
        Some(list) => list.iter().map(|p| file_name(p)).collect(),
        None => vec![file_name(
            container.path.split("::").next().unwrap_or(&container.path),
        )],
    };
    Ok(ProblemArchive {
        folder: folder.unwrap_or_default(),
        library_name,
        parts,
        container,
    })
}

/// Archives that cannot be previewed: incomplete, corrupt, unsupported or password protected.
pub fn problem_archives_impl(conn: &Connection) -> AppResult<Vec<ProblemArchive>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {CONTAINER_COLS}, {PROBLEM_COLS}
         FROM containers c JOIN libraries l ON l.id = c.library_id
         WHERE c.kind <> 'folder' AND c.missing = 0 AND l.enabled = 1
           AND (c.status <> 'ok' OR c.encrypted = 1)
         ORDER BY c.status, c.name COLLATE NOCASE"
    ))?;
    let rows = stmt.query_map([], problem_from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[tauri::command]
pub async fn problem_archives(app: AppHandle) -> AppResult<Vec<ProblemArchive>> {
    blocking(app, |_, st| problem_archives_impl(&*st.db.lock()?)).await
}

#[tauri::command]
pub async fn archive_problem(app: AppHandle, id: i64) -> AppResult<ProblemArchive> {
    blocking(app, move |_, st| {
        let conn = st.db.lock()?;
        Ok(conn.query_row(
            &format!(
                "SELECT {CONTAINER_COLS}, {PROBLEM_COLS}
                 FROM containers c JOIN libraries l ON l.id = c.library_id WHERE c.id = ?1"
            ),
            [id],
            problem_from_row,
        )?)
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

    #[test]
    fn flat_view_lists_everything_below_with_locations() {
        let conn = crate::db::test_conn();
        conn.execute_batch(
            "INSERT INTO libraries (id, name, path) VALUES (1, 'L', '/l');
             INSERT INTO containers (id, library_id, parent_id, kind, path, name) VALUES
               (1, 1, NULL, 'folder', '/l', 'L'),
               (2, 1, 1, 'folder', '/l/Pack', 'Pack'),
               (3, 1, 2, 'zip', '/l/Pack/p.zip', 'p.zip');
             INSERT INTO models (container_id, library_id, rel_path, name, ext) VALUES
               (1, 1, '/l/root.stl', 'root.stl', 'stl'),
               (2, 1, '/l/Pack/loose.stl', 'loose.stl', 'stl'),
               (3, 1, 'a/one.stl', 'one.stl', 'stl'),
               (3, 1, 'a/b/two.jpg', 'two.jpg', 'jpg'),
               (3, 1, 'top.stl', 'top.stl', 'stl');",
        )
        .expect("seed");
        let all = browse_flat_impl(&conn, 1, "").expect("flat");
        assert!(all.flat);
        assert_eq!(all.models.len(), 5);
        let loc = |r: &BrowseResult, name: &str| {
            let i = r.models.iter().position(|m| m.name == name).expect("model");
            r.locations[i].clone()
        };
        assert_eq!(loc(&all, "root.stl"), "");
        assert_eq!(loc(&all, "loose.stl"), "Pack");
        assert_eq!(loc(&all, "two.jpg"), "Pack › p.zip › a › b");

        let inside = browse_flat_impl(&conn, 3, "a").expect("flat a");
        assert_eq!(inside.models.len(), 2);
        assert_eq!(loc(&inside, "one.stl"), "");
        assert_eq!(loc(&inside, "two.jpg"), "b");
    }

    #[test]
    fn collection_files_flatten_members_once() {
        let conn = crate::db::test_conn();
        conn.execute_batch(
            "INSERT INTO libraries (id, name, path) VALUES (1, 'L', '/l');
             INSERT INTO containers (id, library_id, parent_id, kind, path, name) VALUES
               (1, 1, NULL, 'folder', '/l', 'L'),
               (2, 1, 1, 'folder', '/l/Pack', 'Pack'),
               (3, 1, 2, 'zip', '/l/Pack/p.zip', 'p.zip'),
               (4, 1, 1, 'zip', '/l/other.zip', 'other.zip');
             INSERT INTO models (container_id, library_id, rel_path, name, ext) VALUES
               (2, 1, '/l/Pack/loose.stl', 'loose.stl', 'stl'),
               (3, 1, 'a/one.stl', 'one.stl', 'stl'),
               (4, 1, 'two.stl', 'two.stl', 'stl');
             INSERT INTO collections (id, name) VALUES (1, 'C');
             INSERT INTO collection_items (collection_id, container_id) VALUES (1, 2), (1, 3), (1, 4);",
        )
        .expect("seed");
        let r = collection_files_impl(&conn, 1).expect("files");
        assert_eq!(r.models.len(), 3);
        let loc = |name: &str| {
            let i = r.models.iter().position(|m| m.name == name).expect("model");
            r.locations[i].clone()
        };
        assert_eq!(loc("loose.stl"), "Pack");
        assert_eq!(loc("two.stl"), "other.zip");
        assert!(loc("one.stl").ends_with("a"));
    }

    #[test]
    fn tree_filter_finds_deep_matches_and_their_parents() {
        let conn = crate::db::test_conn();
        conn.execute_batch(
            "INSERT INTO libraries (id, name, path) VALUES (1, 'L', '/l');
             INSERT INTO containers (id, library_id, parent_id, kind, path, name, file_count) VALUES
               (1, 1, NULL, 'folder', '/l', 'L', 3),
               (2, 1, 1, 'folder', '/l/Terrain', 'Terrain', 2),
               (3, 1, 2, 'zip', '/l/Terrain/Dungeon_100%.zip', 'Dungeon_100%.zip', 2),
               (4, 1, 1, 'zip', '/l/Dungeon empty.zip', 'Dungeon empty.zip', 0),
               (5, 1, 1, 'zip', '/l/Dungeonx100.zip', 'Dungeonx100.zip', 1);",
        )
        .expect("seed");
        let r = tree_filter_impl(&conn, "dungeon").expect("filter");
        assert_eq!(
            r.matches.len(),
            2,
            "empty containers stay hidden, as in the tree"
        );
        assert!(r.ancestors.contains(&1) && r.ancestors.contains(&2));
        // Wildcards in the query are literal.
        let r = tree_filter_impl(&conn, "_100%").expect("filter");
        assert_eq!(r.matches, vec![3]);
        assert!(tree_filter_impl(&conn, "  ")
            .expect("blank")
            .matches
            .is_empty());
    }

    #[test]
    fn resolves_typed_paths() {
        let conn = crate::db::test_conn();
        conn.execute_batch(
            r"INSERT INTO libraries (id, name, path) VALUES (1, 'L', 'T:\Lib');
             INSERT INTO containers (id, library_id, parent_id, kind, path, name) VALUES
               (1, 1, NULL, 'folder', 'T:\Lib', 'L'),
               (2, 1, 1, 'folder', 'T:\Lib\Orks', 'Orks'),
               (3, 1, 2, 'zip', 'T:\Lib\Orks\Boyz.zip', 'Boyz.zip');
             INSERT INTO models (container_id, library_id, rel_path, name, ext) VALUES
               (3, 1, 'Big Boyz/Nob/nob.stl', 'nob.stl', 'stl');",
        )
        .expect("seed");
        let r = resolve_location_impl(&conn, "t:/lib/orks/").expect("folder");
        assert_eq!((r.container_id, r.subpath.as_str()), (2, ""));
        let r = resolve_location_impl(&conn, r"T:\Lib\Orks\boyz.zip\big boyz\nob").expect("inside");
        assert_eq!((r.container_id, r.subpath.as_str()), (3, "Big Boyz/Nob"));
        assert!(resolve_location_impl(&conn, r"T:\Lib\Elves").is_err());
        assert!(resolve_location_impl(&conn, r"C:\Elsewhere").is_err());
    }

    #[test]
    fn lists_problem_archives_with_their_parts() {
        let conn = crate::db::test_conn();
        conn.execute_batch(
            r#"INSERT INTO libraries (id, name, path) VALUES (1, 'L', '/l');
             INSERT INTO containers (id, library_id, parent_id, kind, path, part_paths, name, status, status_detail) VALUES
               (1, 1, NULL, 'folder', '/l', NULL, 'L', 'ok', NULL),
               (2, 1, 1, 'rar', '/l/Big.part5.rar', '["/l/Big.part5.rar","/l/Big.part6.rar"]', 'Big.rar', 'missing_parts', 'Missing parts 1, 2, 3, 4 of 6'),
               (3, 1, 1, 'zip', '/l/fine.zip', NULL, 'fine.zip', 'ok', NULL);"#,
        )
        .expect("seed");
        let list = problem_archives_impl(&conn).expect("problems");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].parts, vec!["Big.part5.rar", "Big.part6.rar"]);
        assert_eq!(list[0].folder, "/l");
    }
}
