//! The print batch: files gathered from anywhere to send to a slicer in one launch. The list
//! itself lives in the UI; this only resolves its ids to current details.

use rusqlite::Connection;
use tauri::AppHandle;

use super::blocking;
use super::index::model_detail_impl;
use crate::error::{AppError, AppResult};
use crate::types::BatchItem;

/// Ids that no longer exist (deleted, or forgotten by "clean up missing") are left out, so the
/// UI can drop them.
pub fn batch_items_impl(conn: &Connection, ids: &[i64]) -> AppResult<Vec<BatchItem>> {
    let mut out = Vec::with_capacity(ids.len());
    for &id in ids {
        let detail = match model_detail_impl(conn, id) {
            Ok(d) => d,
            Err(AppError::NotFound(_)) => continue,
            Err(e) => return Err(e),
        };
        let location = detail
            .breadcrumbs
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
            .join(" › ");
        let (container_id, subpath) = detail
            .breadcrumbs
            .last()
            .map(|c| (c.container_id, c.subpath.clone()))
            .unwrap_or((detail.container.id, String::new()));
        out.push(BatchItem {
            model: detail.model,
            location,
            container_id,
            subpath,
        });
    }
    Ok(out)
}

#[tauri::command]
pub async fn batch_items(app: AppHandle, ids: Vec<i64>) -> AppResult<Vec<BatchItem>> {
    blocking(app, move |_, st| batch_items_impl(&*st.db.lock()?, &ids)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_locations_and_drops_unknown_ids() {
        let conn = crate::db::test_conn();
        conn.execute_batch(
            "INSERT INTO libraries (id, name, path) VALUES (1, 'L', '/l');
             INSERT INTO containers (id, library_id, parent_id, kind, path, name) VALUES
               (1, 1, NULL, 'folder', '/l', 'L'),
               (2, 1, 1, 'folder', '/l/Pack', 'Pack'),
               (3, 1, 2, 'zip', '/l/Pack/p.zip', 'p.zip');
             INSERT INTO models (id, container_id, library_id, rel_path, name, ext) VALUES
               (10, 2, 1, '/l/Pack/loose.stl', 'loose.stl', 'stl'),
               (11, 3, 1, 'a/b/two.stl', 'two.stl', 'stl');",
        )
        .expect("seed");
        let items = batch_items_impl(&conn, &[11, 999, 10]).expect("items");
        let ids: Vec<i64> = items.iter().map(|i| i.model.id).collect();
        assert_eq!(
            ids,
            vec![11, 10],
            "keeps batch order and skips the unknown id"
        );
        assert_eq!(items[0].location, "L › Pack › p.zip › a › b");
        assert_eq!(
            (items[0].container_id, items[0].subpath.as_str()),
            (3, "a/b")
        );
        assert_eq!(items[1].location, "L › Pack");
    }
}
