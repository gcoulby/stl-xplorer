use std::path::PathBuf;

use rusqlite::{params, Connection, OptionalExtension};
use tauri::AppHandle;

use super::blocking;
use crate::error::{AppError, AppResult};
use crate::slicer::{self, LaunchMode};
use crate::types::{LaunchReport, Slicer, SlicerPreset};

fn list(conn: &Connection) -> AppResult<Vec<Slicer>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, exe_path, args, is_default, one_per_file FROM slicers
         ORDER BY is_default DESC, name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| {
        let args: String = r.get(3)?;
        let exe: String = r.get(2)?;
        Ok(Slicer {
            id: r.get(0)?,
            name: r.get(1)?,
            path_exists: PathBuf::from(&exe).exists(),
            exe_path: exe,
            args: serde_json::from_str(&args).unwrap_or_default(),
            is_default: r.get(4)?,
            one_per_file: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[tauri::command]
pub async fn list_slicers(app: AppHandle) -> AppResult<Vec<Slicer>> {
    blocking(app, |_, st| list(&*st.db.lock()?)).await
}

#[tauri::command]
pub async fn detect_slicers(app: AppHandle) -> AppResult<Vec<SlicerPreset>> {
    blocking(app, |_, st| {
        let existing = list(&*st.db.lock()?)?;
        Ok(slicer::detect_presets()
            .into_iter()
            .map(|(name, path)| {
                let p = path.to_string_lossy().into_owned();
                SlicerPreset {
                    already_added: existing.iter().any(|s| s.exe_path == p),
                    name,
                    exe_path: p,
                }
            })
            .collect())
    })
    .await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlicerInput {
    pub name: String,
    pub exe_path: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub one_per_file: bool,
}

fn validate(input: &SlicerInput) -> AppResult<()> {
    if input.name.trim().is_empty() {
        return Err(AppError::Invalid("Give the slicer a name".into()));
    }
    if !PathBuf::from(input.exe_path.trim()).exists() {
        return Err(AppError::Invalid(format!(
            "No program at {}",
            input.exe_path.trim()
        )));
    }
    Ok(())
}

#[tauri::command]
pub async fn add_slicer(app: AppHandle, input: SlicerInput) -> AppResult<Vec<Slicer>> {
    blocking(app, move |_, st| {
        validate(&input)?;
        let conn = st.db.lock()?;
        let none: i64 = conn.query_row("SELECT COUNT(*) FROM slicers", [], |r| r.get(0))?;
        conn.execute(
            "INSERT INTO slicers (name, exe_path, args, is_default, one_per_file) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                input.name.trim(),
                input.exe_path.trim(),
                serde_json::to_string(&input.args)?,
                none == 0,
                input.one_per_file
            ],
        )?;
        list(&conn)
    })
    .await
}

#[tauri::command]
pub async fn update_slicer(app: AppHandle, id: i64, input: SlicerInput) -> AppResult<Vec<Slicer>> {
    blocking(app, move |_, st| {
        validate(&input)?;
        let conn = st.db.lock()?;
        conn.execute(
            "UPDATE slicers SET name = ?2, exe_path = ?3, args = ?4, one_per_file = ?5 WHERE id = ?1",
            params![
                id,
                input.name.trim(),
                input.exe_path.trim(),
                serde_json::to_string(&input.args)?,
                input.one_per_file
            ],
        )?;
        list(&conn)
    })
    .await
}

#[tauri::command]
pub async fn delete_slicer(app: AppHandle, id: i64) -> AppResult<Vec<Slicer>> {
    blocking(app, move |_, st| {
        let conn = st.db.lock()?;
        conn.execute("DELETE FROM slicers WHERE id = ?1", [id])?;
        let has_default: i64 = conn.query_row(
            "SELECT COUNT(*) FROM slicers WHERE is_default = 1",
            [],
            |r| r.get(0),
        )?;
        if has_default == 0 {
            conn.execute(
                "UPDATE slicers SET is_default = 1 WHERE id = (SELECT MIN(id) FROM slicers)",
                [],
            )?;
        }
        list(&conn)
    })
    .await
}

#[tauri::command]
pub async fn set_default_slicer(app: AppHandle, id: i64) -> AppResult<Vec<Slicer>> {
    blocking(app, move |_, st| {
        let conn = st.db.lock()?;
        conn.execute("UPDATE slicers SET is_default = (id = ?1)", [id])?;
        list(&conn)
    })
    .await
}

#[tauri::command]
pub async fn open_in_slicer(
    app: AppHandle,
    model_ids: Vec<i64>,
    slicer_id: Option<i64>,
) -> AppResult<LaunchReport> {
    blocking(app, move |_, st| {
        if model_ids.is_empty() {
            return Err(AppError::Invalid("Select at least one file".into()));
        }
        let (exe, args, one_per_file, name): (String, String, bool, String) = {
            let conn = st.db.lock()?;
            let sql = "SELECT exe_path, args, one_per_file, name FROM slicers WHERE ";
            let row = match slicer_id {
                Some(id) => conn
                    .query_row(&format!("{sql} id = ?1"), [id], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                    })
                    .optional()?,
                None => conn
                    .query_row(&format!("{sql} is_default = 1 LIMIT 1"), [], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                    })
                    .optional()?,
            };
            row.ok_or_else(|| AppError::Invalid("Add a slicer in Settings first".into()))?
        };
        let args: Vec<String> = serde_json::from_str(&args).unwrap_or_default();
        // Images and PDFs ride along in selections; slicers only get the meshes.
        let model_ids: Vec<i64> = {
            let conn = st.db.lock()?;
            let mut keep = Vec::new();
            for id in model_ids {
                // A batch can outlive its files; skip ones that are gone rather than failing.
                let ext: Option<String> = conn
                    .query_row("SELECT ext FROM models WHERE id = ?1", [id], |r| r.get(0))
                    .optional()?;
                if ext.is_some_and(|e| crate::util::is_mesh_ext(&e)) {
                    keep.push(id);
                }
            }
            keep
        };
        if model_ids.is_empty() {
            return Err(AppError::Invalid(
                "None of the selected files are models".into(),
            ));
        }
        let files = crate::preview::real_paths(st, &model_ids)?;
        let mode = slicer::launch(&PathBuf::from(exe), &args, &files, one_per_file)?;
        Ok(match mode {
            LaunchMode::Together => LaunchReport {
                launches: 1,
                files: files.len(),
                per_file: false,
                message: None,
            },
            LaunchMode::PerFile { fallback } => LaunchReport {
                launches: files.len(),
                files: files.len(),
                per_file: true,
                message: fallback.then(|| {
                    format!(
                        "{name} could not take all {} files at once, so each opened separately",
                        files.len()
                    )
                }),
            },
        })
    })
    .await
}
