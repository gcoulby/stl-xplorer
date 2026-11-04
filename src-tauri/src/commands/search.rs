use rusqlite::{params, Connection};
use tauri::AppHandle;

use super::blocking;
use crate::db::queries::{self, MODEL_COLS};
use crate::error::AppResult;
use crate::types::SearchHit;

/// Turn what the user typed into an FTS5 query: every word must match, each as a prefix.
pub fn fts_query(input: &str) -> Option<String> {
    let terms: Vec<String> = input
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}

pub fn search_impl(conn: &Connection, query: &str, limit: i64) -> AppResult<Vec<SearchHit>> {
    let Some(q) = fts_query(query) else {
        return Ok(Vec::new());
    };
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {MODEL_COLS}, c.name, c.kind, c.path, l.name
         FROM models_fts f
         JOIN models m ON m.id = f.rowid
         JOIN containers c ON c.id = m.container_id
         JOIN libraries l ON l.id = m.library_id
         WHERE models_fts MATCH ?1 AND m.missing = 0 AND l.enabled = 1
         ORDER BY bm25(models_fts, 10.0, 4.0, 1.0, 6.0)
         LIMIT ?2"
    ))?;
    let rows = stmt.query_map(params![q, limit], |r| {
        let model = queries::model_from_row(r)?;
        let container_name: String = r.get(17)?;
        let container_kind: String = r.get(18)?;
        let container_path: String = r.get(19)?;
        let library_name: String = r.get(20)?;
        let location = if container_kind == "folder" {
            container_path
        } else {
            match model.rel_path.rsplit_once('/') {
                Some((dir, _)) => format!("{container_name} › {}", dir.replace('/', " › ")),
                None => container_name.clone(),
            }
        };
        Ok(SearchHit {
            model,
            container_name,
            container_kind,
            location,
            library_name,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[tauri::command]
pub async fn search(
    app: AppHandle,
    query: String,
    limit: Option<i64>,
) -> AppResult<Vec<SearchHit>> {
    blocking(app, move |_, st| {
        search_impl(&*st.db.lock()?, &query, limit.unwrap_or(300).clamp(1, 2000))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_prefix_queries() {
        assert_eq!(fts_query("drag ").as_deref(), Some("\"drag\"*"));
        assert_eq!(
            fts_query("big \"dragon").as_deref(),
            Some("\"big\"* \"dragon\"*")
        );
        assert_eq!(fts_query("  - "), None);
    }

    #[test]
    fn finds_by_name_pack_path_and_tag() {
        let conn = crate::db::test_conn();
        conn.execute_batch(
            "INSERT INTO libraries (id, name, path) VALUES (1, 'L', '/l');
             INSERT INTO containers (id, library_id, parent_id, kind, path, name) VALUES
               (1, 1, NULL, 'folder', '/l', 'L'),
               (2, 1, 1, 'zip', '/l/Fantasy Minis.zip', 'Fantasy Minis.zip');
             INSERT INTO models (id, container_id, library_id, rel_path, name, ext) VALUES
               (1, 2, 1, 'orcs/orc_warrior_body.stl', 'orc_warrior_body.stl', 'stl'),
               (2, 1, 1, '/l/benchy.stl', 'benchy.stl', 'stl');
             INSERT INTO tags (id, name) VALUES (1, 'painted');
             INSERT INTO model_tags (model_id, tag_id) VALUES (2, 1);",
        )
        .expect("seed");
        queries::refresh_fts(&conn, &[1, 2]).expect("fts");
        let ids = |q: &str| -> Vec<i64> {
            search_impl(&conn, q, 10)
                .expect("search")
                .iter()
                .map(|h| h.model.id)
                .collect()
        };
        assert_eq!(ids("warr"), vec![1]);
        assert_eq!(ids("fantasy"), vec![1]);
        assert_eq!(ids("orcs body"), vec![1]);
        assert_eq!(ids("paint"), vec![2]);
        assert!(ids("nothing").is_empty());
        let hit = &search_impl(&conn, "orc", 10).expect("search")[0];
        assert_eq!(hit.location, "Fantasy Minis.zip › orcs");
    }
}
