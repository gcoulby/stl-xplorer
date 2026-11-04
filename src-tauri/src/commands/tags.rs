use rusqlite::{params, Connection};
use tauri::AppHandle;

use super::blocking;
use crate::db::queries::{self, CONTAINER_COLS, MODEL_COLS};
use crate::error::{AppError, AppResult};
use crate::types::{Collection, ContainerInfo, Tag, TagSuggestion, TaggedItems};

fn list_tags_impl(conn: &Connection) -> AppResult<Vec<Tag>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, t.colour,
           (SELECT COUNT(*) FROM model_tags WHERE tag_id = t.id) +
           (SELECT COUNT(*) FROM container_tags WHERE tag_id = t.id),
           (SELECT COUNT(*) FROM model_tags WHERE tag_id = t.id AND auto = 1),
           t.keywords
         FROM tags t ORDER BY t.name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Tag {
            id: r.get(0)?,
            name: r.get(1)?,
            colour: r.get(2)?,
            usage: r.get(3)?,
            auto_usage: r.get(4)?,
            keywords: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Models whose search text includes this container's tags.
fn models_under(conn: &Connection, container_ids: &[i64]) -> AppResult<Vec<i64>> {
    let mut out = Vec::new();
    for id in container_ids {
        out.extend(queries::model_ids_in_container(conn, *id)?);
    }
    Ok(out)
}

#[tauri::command]
pub async fn list_tags(app: AppHandle) -> AppResult<Vec<Tag>> {
    blocking(app, |_, st| list_tags_impl(&*st.db.lock()?)).await
}

#[tauri::command]
pub async fn create_tag(
    app: AppHandle,
    name: String,
    colour: Option<String>,
    keywords: Option<String>,
) -> AppResult<Tag> {
    blocking(app, move |_, st| {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::Invalid("Tag name cannot be empty".into()));
        }
        let keywords = clean_keywords(keywords);
        let mut conn = st.db.lock()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO tags (name, colour, keywords) VALUES (?1, COALESCE(?2, '#64748b'), ?3)
             ON CONFLICT(name) DO UPDATE SET keywords = COALESCE(excluded.keywords, keywords)",
            params![name, colour, keywords],
        )?;
        let id: i64 = tx.query_row(
            "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
            [&name],
            |r| r.get(0),
        )?;
        if keywords.is_some() {
            let changed = crate::autotag::apply(&tx, None, Some(id))?;
            queries::refresh_fts(&tx, &changed)?;
        }
        tx.commit()?;
        list_tags_impl(&conn)?
            .into_iter()
            .find(|t| t.name.eq_ignore_ascii_case(&name))
            .ok_or_else(|| AppError::Other("tag was not created".into()))
    })
    .await
}

fn clean_keywords(k: Option<String>) -> Option<String> {
    k.map(|k| k.trim().to_string()).filter(|k| !k.is_empty())
}

fn refresh_for_tag(conn: &Connection, tag_id: i64) -> AppResult<()> {
    let mut stmt = conn.prepare(
        "SELECT model_id FROM model_tags WHERE tag_id = ?1
         UNION SELECT m.id FROM models m JOIN container_tags ct ON ct.container_id = m.container_id
         WHERE ct.tag_id = ?1",
    )?;
    let ids: Vec<i64> = stmt
        .query_map([tag_id], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    queries::refresh_fts(conn, &ids)
}

#[tauri::command]
pub async fn update_tag(
    app: AppHandle,
    id: i64,
    name: Option<String>,
    colour: Option<String>,
    keywords: Option<String>,
) -> AppResult<Vec<Tag>> {
    blocking(app, move |_, st| {
        let mut conn = st.db.lock()?;
        let tx = conn.transaction()?;
        // Some("") clears the rule, None leaves it alone.
        if let Some(k) = keywords {
            tx.execute(
                "UPDATE tags SET keywords = ?2 WHERE id = ?1",
                params![id, clean_keywords(Some(k))],
            )?;
            let changed = crate::autotag::apply(&tx, None, Some(id))?;
            queries::refresh_fts(&tx, &changed)?;
        }
        if let Some(n) = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()) {
            tx.execute("UPDATE tags SET name = ?2 WHERE id = ?1", params![id, n])?;
            refresh_for_tag(&tx, id)?;
        }
        if let Some(c) = colour {
            tx.execute("UPDATE tags SET colour = ?2 WHERE id = ?1", params![id, c])?;
        }
        tx.commit()?;
        list_tags_impl(&conn)
    })
    .await
}

#[tauri::command]
pub async fn delete_tag(app: AppHandle, id: i64) -> AppResult<Vec<Tag>> {
    blocking(app, move |_, st| {
        let mut conn = st.db.lock()?;
        let tx = conn.transaction()?;
        let mut stmt = tx.prepare(
            "SELECT model_id FROM model_tags WHERE tag_id = ?1
             UNION SELECT m.id FROM models m JOIN container_tags ct ON ct.container_id = m.container_id
             WHERE ct.tag_id = ?1",
        )?;
        let ids: Vec<i64> = stmt.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
        drop(stmt);
        // A deleted tag must not come straight back from the automatic suggestions.
        let words: Option<String> = tx.query_row(
            "SELECT COALESCE(keywords, name) FROM tags WHERE id = ?1",
            [id],
            |r| r.get(0),
        )?;
        if let Some(w) = words {
            let words: Vec<String> = w
                .split([',', '\n', ';'])
                .map(|k| k.trim().trim_end_matches('*').to_lowercase())
                .filter(|k| !k.is_empty())
                .collect();
            crate::autotag::dismiss(&tx, &words)?;
        }
        tx.execute("DELETE FROM tags WHERE id = ?1", [id])?;
        queries::refresh_fts(&tx, &ids)?;
        tx.commit()?;
        list_tags_impl(&conn)
    })
    .await
}

#[tauri::command]
pub async fn tag_items(
    app: AppHandle,
    tag_id: i64,
    model_ids: Vec<i64>,
    container_ids: Vec<i64>,
    add: bool,
) -> AppResult<()> {
    blocking(app, move |_, st| {
        let mut conn = st.db.lock()?;
        let tx = conn.transaction()?;
        for id in &model_ids {
            if add {
                // Tagging by hand makes the tag stick even if a rule later stops matching.
                tx.execute(
                    "INSERT INTO model_tags (model_id, tag_id, auto) VALUES (?1, ?2, 0)
                     ON CONFLICT (model_id, tag_id) DO UPDATE SET auto = 0",
                    params![id, tag_id],
                )?;
            } else {
                tx.execute(
                    "DELETE FROM model_tags WHERE model_id = ?1 AND tag_id = ?2",
                    params![id, tag_id],
                )?;
            }
        }
        for id in &container_ids {
            if add {
                tx.execute(
                    "INSERT OR IGNORE INTO container_tags (container_id, tag_id) VALUES (?1, ?2)",
                    params![id, tag_id],
                )?;
            } else {
                tx.execute(
                    "DELETE FROM container_tags WHERE container_id = ?1 AND tag_id = ?2",
                    params![id, tag_id],
                )?;
            }
        }
        let mut ids = model_ids;
        ids.extend(models_under(&tx, &container_ids)?);
        queries::refresh_fts(&tx, &ids)?;
        tx.commit()?;
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn tagged_items(app: AppHandle, tag_id: i64) -> AppResult<TaggedItems> {
    blocking(app, move |_, st| {
        let conn = st.db.lock()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {CONTAINER_COLS} FROM containers c JOIN container_tags ct ON ct.container_id = c.id
             WHERE ct.tag_id = ?1 AND c.missing = 0 ORDER BY c.name COLLATE NOCASE"
        ))?;
        let containers = stmt
            .query_map([tag_id], queries::container_from_row)?
            .collect::<Result<_, _>>()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {MODEL_COLS} FROM models m JOIN model_tags mt ON mt.model_id = m.id
             WHERE mt.tag_id = ?1 AND m.missing = 0 ORDER BY m.name COLLATE NOCASE"
        ))?;
        let models = stmt
            .query_map([tag_id], queries::model_from_row)?
            .collect::<Result<_, _>>()?;
        Ok(TaggedItems { containers, models })
    })
    .await
}

#[tauri::command]
pub async fn set_autotag_on_index(app: AppHandle, enabled: bool) -> AppResult<()> {
    blocking(app, move |_, st| {
        queries::set_setting(
            &*st.db.lock()?,
            crate::autotag::ON_INDEX_KEY,
            if enabled { "1" } else { "0" },
        )
    })
    .await
}

/// Run every rule over every file now. Returns how many files gained or lost a tag.
#[tauri::command]
pub async fn apply_auto_tags(app: AppHandle) -> AppResult<usize> {
    blocking(app, |_, st| {
        let mut conn = st.db.lock()?;
        let tx = conn.transaction()?;
        let changed = crate::autotag::apply(&tx, None, None)?;
        queries::refresh_fts(&tx, &changed)?;
        tx.commit()?;
        Ok(changed.len())
    })
    .await
}

/// Turn automatic tags from suggestions on or off. Turning it on applies them straight away.
/// Returns how many files gained a tag.
#[tauri::command]
pub async fn set_autotag_suggestions(app: AppHandle, enabled: bool) -> AppResult<usize> {
    blocking(app, move |_, st| {
        let mut conn = st.db.lock()?;
        let tx = conn.transaction()?;
        queries::set_setting(
            &tx,
            crate::autotag::SUGGESTIONS_KEY,
            if enabled { "1" } else { "0" },
        )?;
        let changed = if enabled {
            crate::autotag::create_from_suggestions(&tx)?
        } else {
            Vec::new()
        };
        queries::refresh_fts(&tx, &changed)?;
        tx.commit()?;
        Ok(changed.len())
    })
    .await
}

#[tauri::command]
pub async fn dismiss_suggestion(app: AppHandle, word: String) -> AppResult<()> {
    blocking(app, move |_, st| {
        crate::autotag::dismiss(&*st.db.lock()?, &[word])
    })
    .await
}

#[tauri::command]
pub async fn restore_suggestions(app: AppHandle) -> AppResult<()> {
    blocking(app, |_, st| {
        queries::delete_setting(&*st.db.lock()?, crate::autotag::DISMISSED_KEY)
    })
    .await
}

#[tauri::command]
pub async fn suggest_tags(app: AppHandle) -> AppResult<Vec<TagSuggestion>> {
    blocking(app, |_, st| crate::autotag::suggest(&*st.db.lock()?, 40)).await
}

fn list_collections_impl(conn: &Connection) -> AppResult<Vec<Collection>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.name, (SELECT COUNT(*) FROM collection_items WHERE collection_id = c.id)
         FROM collections c ORDER BY c.name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Collection {
            id: r.get(0)?,
            name: r.get(1)?,
            item_count: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[tauri::command]
pub async fn list_collections(app: AppHandle) -> AppResult<Vec<Collection>> {
    blocking(app, |_, st| list_collections_impl(&*st.db.lock()?)).await
}

#[tauri::command]
pub async fn create_collection(app: AppHandle, name: String) -> AppResult<Collection> {
    blocking(app, move |_, st| {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::Invalid("Collection name cannot be empty".into()));
        }
        let conn = st.db.lock()?;
        conn.execute("INSERT INTO collections (name) VALUES (?1)", [&name])?;
        let id = conn.last_insert_rowid();
        Ok(Collection {
            id,
            name,
            item_count: 0,
        })
    })
    .await
}

#[tauri::command]
pub async fn rename_collection(
    app: AppHandle,
    id: i64,
    name: String,
) -> AppResult<Vec<Collection>> {
    blocking(app, move |_, st| {
        let conn = st.db.lock()?;
        if !name.trim().is_empty() {
            conn.execute(
                "UPDATE collections SET name = ?2 WHERE id = ?1",
                params![id, name.trim()],
            )?;
        }
        list_collections_impl(&conn)
    })
    .await
}

#[tauri::command]
pub async fn delete_collection(app: AppHandle, id: i64) -> AppResult<Vec<Collection>> {
    blocking(app, move |_, st| {
        let conn = st.db.lock()?;
        conn.execute("DELETE FROM collections WHERE id = ?1", [id])?;
        list_collections_impl(&conn)
    })
    .await
}

#[tauri::command]
pub async fn collection_add(
    app: AppHandle,
    id: i64,
    container_ids: Vec<i64>,
    add: bool,
) -> AppResult<()> {
    blocking(app, move |_, st| {
        let conn = st.db.lock()?;
        for cid in container_ids {
            if add {
                conn.execute(
                    "INSERT OR IGNORE INTO collection_items VALUES (?1, ?2)",
                    params![id, cid],
                )?;
            } else {
                conn.execute(
                    "DELETE FROM collection_items WHERE collection_id = ?1 AND container_id = ?2",
                    params![id, cid],
                )?;
            }
        }
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn collection_items(app: AppHandle, id: i64) -> AppResult<Vec<ContainerInfo>> {
    blocking(app, move |_, st| {
        let conn = st.db.lock()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {CONTAINER_COLS} FROM containers c JOIN collection_items ci ON ci.container_id = c.id
             WHERE ci.collection_id = ?1 ORDER BY c.name COLLATE NOCASE"
        ))?;
        let rows = stmt.query_map([id], queries::container_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    })
    .await
}
