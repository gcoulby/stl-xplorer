//! `mesh://localhost/<model_id>` serves raw mesh bytes to the viewer, so large files never go
//! through JSON. On Windows the webview sees it as `http://mesh.localhost/<id>`.

use tauri::http::{Request, Response, StatusCode};
use tauri::{AppHandle, Manager};

use crate::error::AppError;
use crate::state::AppState;

pub const SCHEME: &str = "mesh";

pub fn handle(app: &AppHandle, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let id = request
        .uri()
        .path()
        .trim_matches('/')
        .split('/')
        .next_back()
        .and_then(|s| s.parse::<i64>().ok());
    let Some(id) = id else {
        return error_response(&AppError::Invalid("bad mesh URL".into()));
    };
    let st = app.state::<AppState>();
    let ext: String = st
        .db
        .lock()
        .ok()
        .and_then(|c| {
            c.query_row("SELECT ext FROM models WHERE id = ?1", [id], |r| r.get(0))
                .ok()
        })
        .unwrap_or_default();
    match crate::preview::model_bytes(app, &st, id) {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", crate::util::content_type(&ext))
            .header("Access-Control-Allow-Origin", "*")
            .header("Cache-Control", "no-store")
            .body(bytes.as_ref().clone())
            .unwrap_or_else(|_| Response::new(Vec::new())),
        Err(e) => error_response(&e),
    }
}

fn error_response(e: &AppError) -> Response<Vec<u8>> {
    let status = match e.kind() {
        "not_found" => StatusCode::NOT_FOUND,
        "invalid" => StatusCode::BAD_REQUEST,
        "missing_parts" => StatusCode::CONFLICT,
        "encrypted" => StatusCode::LOCKED,
        "too_large" => StatusCode::PAYLOAD_TOO_LARGE,
        "cancelled" => StatusCode::from_u16(499).unwrap_or(StatusCode::BAD_REQUEST),
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    let body = serde_json::to_vec(e).unwrap_or_default();
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(body)
        .unwrap_or_else(|_| Response::new(Vec::new()))
}
