use tauri::AppHandle;

use super::blocking;
use crate::error::AppResult;

/// Called when the viewer stops waiting on a preview, so a running 7-Zip process is killed.
#[tauri::command]
pub async fn cancel_preview(app: AppHandle) -> AppResult<()> {
    blocking(app, |_, st| {
        st.cancel_preview();
        Ok(())
    })
    .await
}
