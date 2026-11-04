use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use tauri::{AppHandle, Manager};

use super::blocking;
use crate::db::queries;
use crate::error::{AppError, AppResult};
use crate::index::watcher;
use crate::state::{DEFAULT_SOLID_CAP, SOLID_CAP_KEY, VAULT_KEY};
use crate::types::{AppSettings, CacheStats, Library, Rotation, SevenZipInfo};

#[tauri::command]
pub async fn list_libraries(app: AppHandle) -> AppResult<Vec<Library>> {
    blocking(app, |_, st| queries::list_libraries(&*st.db.lock()?)).await
}

#[tauri::command]
pub async fn add_library(app: AppHandle, path: String, name: Option<String>) -> AppResult<Library> {
    blocking(app, move |app, st| {
        let p = PathBuf::from(&path);
        if !p.is_dir() {
            return Err(AppError::Invalid(format!("{path} is not a folder")));
        }
        let name = name
            .filter(|n| !n.trim().is_empty())
            .or_else(|| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| path.clone());
        let id = {
            let conn = st.db.lock()?;
            let exists: Option<i64> = conn
                .query_row("SELECT id FROM libraries WHERE path = ?1", [&path], |r| {
                    r.get(0)
                })
                .ok();
            if exists.is_some() {
                return Err(AppError::Invalid("That folder is already a library".into()));
            }
            conn.execute(
                "INSERT INTO libraries (name, path, sort_order)
                 VALUES (?1, ?2, (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM libraries))",
                params![name.trim(), path],
            )?;
            conn.last_insert_rowid()
        };
        watcher::restart(app)?;
        st.scans.request(app, id);
        queries::get_library(&*st.db.lock()?, id)
    })
    .await
}

#[tauri::command]
pub async fn update_library(
    app: AppHandle,
    id: i64,
    name: Option<String>,
    enabled: Option<bool>,
) -> AppResult<Library> {
    blocking(app, move |app, st| {
        {
            let conn = st.db.lock()?;
            if let Some(n) = name.filter(|n| !n.trim().is_empty()) {
                conn.execute(
                    "UPDATE libraries SET name = ?2 WHERE id = ?1",
                    params![id, n.trim()],
                )?;
                conn.execute(
                    "UPDATE containers SET name = ?2 WHERE library_id = ?1 AND parent_id IS NULL",
                    params![id, n.trim()],
                )?;
            }
            if let Some(e) = enabled {
                let is_vault: bool =
                    conn.query_row("SELECT is_vault FROM libraries WHERE id = ?1", [id], |r| {
                        r.get(0)
                    })?;
                if is_vault && !e {
                    return Err(AppError::Invalid("The vault cannot be disabled".into()));
                }
                conn.execute(
                    "UPDATE libraries SET enabled = ?2 WHERE id = ?1",
                    params![id, e],
                )?;
            }
        }
        if enabled.is_some() {
            watcher::restart(app)?;
            if enabled == Some(true) {
                st.scans.request(app, id);
            }
        }
        queries::get_library(&*st.db.lock()?, id)
    })
    .await
}

#[tauri::command]
pub async fn remove_library(app: AppHandle, id: i64) -> AppResult<()> {
    blocking(app, move |app, st| {
        st.scans.cancel(id);
        {
            let conn = st.db.lock()?;
            let is_vault: bool =
                conn.query_row("SELECT is_vault FROM libraries WHERE id = ?1", [id], |r| {
                    r.get(0)
                })?;
            if is_vault {
                return Err(AppError::Invalid("The vault cannot be removed".into()));
            }
            conn.execute("DELETE FROM libraries WHERE id = ?1", [id])?;
        }
        watcher::restart(app)
    })
    .await
}

#[tauri::command]
pub async fn rescan_library(app: AppHandle, id: i64) -> AppResult<()> {
    blocking(app, move |app, st| {
        st.scans.request(app, id);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn rescan_all(app: AppHandle) -> AppResult<()> {
    blocking(app, |app, st| {
        let libs = queries::list_libraries(&*st.db.lock()?)?;
        for l in libs.into_iter().filter(|l| l.enabled) {
            st.scans.request(app, l.id);
        }
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn running_scans(app: AppHandle) -> AppResult<Vec<i64>> {
    blocking(app, |_, st| Ok(st.scans.running())).await
}

pub fn default_vault_path(app: &AppHandle) -> PathBuf {
    app.path()
        .document_dir()
        .or_else(|_| app.path().home_dir())
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("STL Xplorer Vault")
}

/// Make sure the vault folder exists and is registered as a library.
pub fn ensure_vault(conn: &Connection, default: &Path) -> AppResult<PathBuf> {
    let path = queries::get_setting(conn, VAULT_KEY)?
        .map(PathBuf::from)
        .unwrap_or_else(|| default.to_path_buf());
    std::fs::create_dir_all(&path)?;
    let p = path.to_string_lossy().into_owned();
    queries::set_setting(conn, VAULT_KEY, &p)?;
    let current: Option<(i64, String)> = conn
        .query_row(
            "SELECT id, path FROM libraries WHERE is_vault = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    match current {
        Some((_, existing)) if existing == p => {}
        Some((id, _)) => {
            // The old vault stays as an ordinary library so nothing in it disappears.
            conn.execute(
                "UPDATE libraries SET is_vault = 0, name = name || ' (old vault)' WHERE id = ?1",
                [id],
            )?;
            register_vault(conn, &p)?;
        }
        None => register_vault(conn, &p)?,
    }
    Ok(path)
}

fn register_vault(conn: &Connection, path: &str) -> AppResult<()> {
    let updated = conn.execute(
        "UPDATE libraries SET is_vault = 1, enabled = 1 WHERE path = ?1",
        [path],
    )?;
    if updated == 0 {
        conn.execute(
            "INSERT INTO libraries (name, path, is_vault, sort_order) VALUES ('Vault', ?1, 1, 0)",
            [path],
        )?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> AppResult<AppSettings> {
    blocking(app, |_, st| {
        let conn = st.db.lock()?;
        let count = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0));
        Ok(AppSettings {
            vault_path: queries::get_setting(&conn, VAULT_KEY)?.unwrap_or_default(),
            solid_cache_cap_bytes: st.solid.cap(),
            global_rotation: crate::orientation::global(&conn)?,
            missing_models: count("SELECT COUNT(*) FROM models WHERE missing = 1")?,
            missing_containers: count("SELECT COUNT(*) FROM containers WHERE missing = 1")?,
            data_dir: st.data_dir.to_string_lossy().into_owned(),
            autotag_on_index: crate::autotag::enabled_on_index(&conn)?,
            autotag_suggestions: crate::autotag::suggestions_enabled(&conn)?,
            dismissed_suggestions: crate::autotag::dismissed(&conn)?,
        })
    })
    .await
}

#[tauri::command]
pub async fn set_vault_path(app: AppHandle, path: String) -> AppResult<()> {
    blocking(app, move |app, st| {
        if path.trim().is_empty() {
            return Err(AppError::Invalid("Choose a folder for the vault".into()));
        }
        {
            let conn = st.db.lock()?;
            queries::set_setting(&conn, VAULT_KEY, path.trim())?;
            ensure_vault(&conn, Path::new(path.trim()))?;
        }
        watcher::restart(app)?;
        let id: i64 =
            st.db
                .lock()?
                .query_row("SELECT id FROM libraries WHERE is_vault = 1", [], |r| {
                    r.get(0)
                })?;
        st.scans.request(app, id);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn set_solid_cache_cap(app: AppHandle, bytes: u64) -> AppResult<()> {
    blocking(app, move |_, st| {
        let bytes = bytes.max(256 * 1024 * 1024);
        queries::set_setting(&*st.db.lock()?, SOLID_CAP_KEY, &bytes.to_string())?;
        st.solid.set_cap(bytes);
        Ok(())
    })
    .await
}

pub fn solid_cap(conn: &Connection) -> u64 {
    queries::get_setting(conn, SOLID_CAP_KEY)
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SOLID_CAP)
}

#[tauri::command]
pub async fn set_global_rotation(app: AppHandle, rotation: Option<Rotation>) -> AppResult<()> {
    blocking(app, move |_, st| {
        let conn = st.db.lock()?;
        match rotation {
            Some(r) => queries::set_setting(
                &conn,
                crate::orientation::GLOBAL_KEY,
                &serde_json::to_string(&crate::orientation::normalise(r))?,
            ),
            None => queries::delete_setting(&conn, crate::orientation::GLOBAL_KEY),
        }
    })
    .await
}

#[tauri::command]
pub async fn sevenzip_info(app: AppHandle) -> AppResult<SevenZipInfo> {
    blocking(app, |_, st| Ok(st.sz.info())).await
}

#[tauri::command]
pub async fn cache_stats(app: AppHandle) -> AppResult<CacheStats> {
    blocking(app, |_, st| {
        let (solid_bytes, solid_archives) = st.solid.stats();
        Ok(CacheStats {
            solid_bytes,
            solid_archives,
            solid_cap_bytes: st.solid.cap(),
            extracted_bytes: crate::archive::cache::dir_size(&st.extract_root),
            memory_bytes: st.mesh_cache()?.total(),
            cache_dir: st.cache_root.to_string_lossy().into_owned(),
        })
    })
    .await
}

#[tauri::command]
pub async fn clear_cache(app: AppHandle) -> AppResult<()> {
    blocking(app, |_, st| {
        st.cancel_preview();
        st.solid.clear()?;
        if st.extract_root.is_dir() {
            std::fs::remove_dir_all(&st.extract_root)?;
            std::fs::create_dir_all(&st.extract_root)?;
        }
        st.mesh_cache()?.clear();
        Ok(())
    })
    .await
}

/// Permanently forget rows for files that are gone. Tags and orientation on them go too.
#[tauri::command]
pub async fn clean_up_missing(app: AppHandle) -> AppResult<u64> {
    blocking(app, |_, st| {
        let mut conn = st.db.lock()?;
        let tx = conn.transaction()?;
        let models = tx.execute("DELETE FROM models WHERE missing = 1", [])?;
        let containers = tx.execute("DELETE FROM containers WHERE missing = 1", [])?;
        let libs: Vec<i64> = {
            let mut stmt = tx.prepare("SELECT id FROM libraries")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect::<Result<_, _>>()?
        };
        for l in libs {
            queries::recount_library(&tx, l)?;
        }
        tx.commit()?;
        Ok((models + containers) as u64)
    })
    .await
}
