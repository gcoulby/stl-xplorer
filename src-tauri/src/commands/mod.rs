pub mod delete;
pub mod files;
pub mod index;
pub mod library;
pub mod mesh;
pub mod search;
pub mod slicer;
pub mod tags;
pub mod vault;

use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Run command work on the blocking pool so the main thread (and the UI) never waits on disk.
pub async fn blocking<T, F>(app: AppHandle, f: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(&AppHandle, &AppState) -> AppResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        f(&app, &st)
    })
    .await
    .map_err(|e| AppError::Other(format!("background task failed: {e}")))?
}
