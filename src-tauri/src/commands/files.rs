//! Handing files to the operating system: show in Explorer/Finder, or open with the default
//! app (images and PDFs). Paths are passed as single arguments, never through a shell.

use std::path::{Path, PathBuf};
use std::process::Command;

use tauri::AppHandle;

use super::blocking;
use crate::db::queries;
use crate::error::{AppError, AppResult};
use crate::slicer::spawn_detached;

fn reveal(path: &Path) -> AppResult<()> {
    if !path.exists() {
        return Err(AppError::NotFound(format!(
            "{} is no longer on disk",
            path.display()
        )));
    }
    #[cfg(windows)]
    let cmd = {
        let mut c = Command::new("explorer.exe");
        // explorer wants "/select," and the path as one argument.
        let mut arg = std::ffi::OsString::from("/select,");
        arg.push(path.as_os_str());
        c.arg(arg);
        c
    };
    #[cfg(target_os = "macos")]
    let cmd = {
        let mut c = Command::new("open");
        c.arg("-R").arg(path);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let cmd = {
        let mut c = Command::new("xdg-open");
        c.arg(path.parent().unwrap_or(path));
        c
    };
    spawn_detached(cmd).map_err(|e| AppError::Other(format!("Could not open the folder: {e}")))
}

fn open_default(path: &Path) -> AppResult<()> {
    #[cfg(windows)]
    let cmd = {
        let mut c = Command::new("explorer.exe");
        c.arg(path);
        c
    };
    #[cfg(target_os = "macos")]
    let cmd = {
        let mut c = Command::new("open");
        c.arg(path);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let cmd = {
        let mut c = Command::new("xdg-open");
        c.arg(path);
        c
    };
    spawn_detached(cmd).map_err(|e| AppError::Other(format!("Could not open the file: {e}")))
}

/// Show a container (folder or archive) or a loose file in the system file manager.
#[tauri::command]
pub async fn reveal_in_folder(
    app: AppHandle,
    container_id: Option<i64>,
    model_id: Option<i64>,
) -> AppResult<()> {
    blocking(app, move |_, st| {
        let path = {
            let conn = st.db.lock()?;
            match (container_id, model_id) {
                (Some(cid), _) => {
                    let c = queries::get_container(&conn, cid)?;
                    // A nested zip lives inside its parent; show the outer archive.
                    let p = c.path.split("::").next().unwrap_or(&c.path).to_string();
                    PathBuf::from(p)
                }
                (None, Some(mid)) => {
                    let m = queries::get_model(&conn, mid)?;
                    if m.in_archive {
                        let c = queries::get_container(&conn, m.container_id)?;
                        PathBuf::from(c.path.split("::").next().unwrap_or(&c.path))
                    } else {
                        PathBuf::from(m.rel_path)
                    }
                }
                (None, None) => return Err(AppError::Invalid("Nothing to show".into())),
            }
        };
        reveal(&path)
    })
    .await
}

/// Open a file with its default app, extracting it from its archive first if needed.
#[tauri::command]
pub async fn open_externally(app: AppHandle, model_id: i64) -> AppResult<()> {
    blocking(app, move |_, st| {
        let paths = crate::preview::real_paths(st, &[model_id])?;
        let path = paths
            .first()
            .ok_or_else(|| AppError::NotFound("file could not be prepared".into()))?;
        open_default(path)
    })
    .await
}
