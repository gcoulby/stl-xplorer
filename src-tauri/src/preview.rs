//! Getting real bytes and real files for models, wherever they live.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use rusqlite::Connection;
use tauri::{AppHandle, Emitter};

use crate::archive::{open_reader, ArchiveSource, MAX_PREVIEW_BYTES};
use crate::db::queries;
use crate::error::{AppError, AppResult, ArchiveFailure};
use crate::state::AppState;
use crate::types::{ContainerInfo, ModelInfo, UnpackProgress};
use crate::util::{stable_hash, unique_path, Cancel};

pub struct ArchiveMeta {
    pub container: ContainerInfo,
    pub source: ArchiveSource,
    pub signature: String,
}

pub fn archive_meta(conn: &Connection, container: ContainerInfo) -> AppResult<ArchiveMeta> {
    let (signature, part_paths): (Option<String>, Option<String>) = conn.query_row(
        "SELECT signature, part_paths FROM containers WHERE id = ?1",
        [container.id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let nested = match (&container.entry_path, container.parent_id) {
        (Some(entry), Some(pid)) => {
            let outer = queries::get_container(conn, pid)?;
            Some((PathBuf::from(outer.path), entry.clone()))
        }
        _ => None,
    };
    let source = ArchiveSource {
        kind: container.kind.clone(),
        path: nested
            .as_ref()
            .map(|(o, _)| o.clone())
            .unwrap_or_else(|| PathBuf::from(&container.path)),
        multi_part: part_paths.is_some(),
        nested,
    };
    Ok(ArchiveMeta {
        container,
        source,
        signature: signature.unwrap_or_default(),
    })
}

fn check_readable(c: &ContainerInfo) -> AppResult<()> {
    match c.status.as_str() {
        "ok" => Ok(()),
        "missing_parts" => Err(AppError::Archive(ArchiveFailure::MissingParts(
            c.status_detail
                .clone()
                .unwrap_or_else(|| "A part is missing".into()),
        ))),
        "unsupported" => Err(AppError::Archive(ArchiveFailure::Unsupported(
            c.status_detail.clone().unwrap_or_default(),
        ))),
        _ => Err(AppError::Archive(ArchiveFailure::Corrupt(
            c.status_detail.clone().unwrap_or_default(),
        ))),
    }
}

fn read_capped(path: &std::path::Path) -> AppResult<Vec<u8>> {
    let len = std::fs::metadata(path)?.len();
    if len > MAX_PREVIEW_BYTES {
        return Err(AppError::Archive(ArchiveFailure::TooLarge(len)));
    }
    Ok(std::fs::read(path)?)
}

/// Mesh bytes for the viewer, from memory, disk, an archive, or the solid archive cache.
pub fn model_bytes(app: &AppHandle, st: &AppState, model_id: i64) -> AppResult<Arc<Vec<u8>>> {
    if let Some(b) = st.mesh_cache()?.get(model_id) {
        return Ok(b);
    }
    let (model, meta) = {
        let conn = st.db.lock()?;
        let model = queries::get_model(&conn, model_id)?;
        let container = queries::get_container(&conn, model.container_id)?;
        if container.kind == "folder" {
            (model, None)
        } else {
            let meta = archive_meta(&conn, container)?;
            (model, Some(meta))
        }
    };
    if model.missing {
        return Err(AppError::NotFound(format!(
            "{} is no longer on disk",
            model.name
        )));
    }

    let bytes = match meta {
        None => {
            st.cancel_preview();
            read_capped(std::path::Path::new(&model.rel_path))?
        }
        Some(meta) => {
            check_readable(&meta.container)?;
            if model.encrypted {
                return Err(AppError::Archive(ArchiveFailure::Encrypted));
            }
            let cancel = st.begin_preview(meta.container.id, meta.container.solid);
            if meta.container.solid {
                let path = ensure_solid_cached(app, st, &meta, &model.rel_path, &cancel)?;
                read_capped(&path)?
            } else {
                let reader = open_reader(&meta.source, &st.sz)?;
                reader.read_entry(&model.rel_path, MAX_PREVIEW_BYTES, &cancel)?
            }
        }
    };
    let bytes = Arc::new(bytes);
    st.mesh_cache()?.insert(model_id, bytes.clone());
    Ok(bytes)
}

/// For solid archives, unpack every mesh once into the bounded cache, then serve from there.
fn ensure_solid_cached(
    app: &AppHandle,
    st: &AppState,
    meta: &ArchiveMeta,
    rel_path: &str,
    cancel: &Cancel,
) -> AppResult<PathBuf> {
    let cid = meta.container.id;
    if let Some(p) = st.solid.lookup(cid, &meta.signature, rel_path) {
        return Ok(p);
    }
    let lock = st.solid.lock_for(cid)?;
    let _guard = lock
        .lock()
        .map_err(|_| AppError::Other("cache lock poisoned".into()))?;
    if let Some(p) = st.solid.lookup(cid, &meta.signature, rel_path) {
        return Ok(p);
    }
    let entries: Vec<String> = {
        let conn = st.db.lock()?;
        let mut stmt = conn.prepare(
            "SELECT rel_path FROM models WHERE container_id = ?1 AND missing = 0 AND encrypted = 0",
        )?;
        let rows = stmt.query_map([cid], |r| r.get(0))?;
        rows.collect::<Result<_, _>>()?
    };
    let reader = open_reader(&meta.source, &st.sz)?;
    let emit = |percent: f32| {
        let _ = app.emit(
            "unpack-progress",
            UnpackProgress {
                container_id: cid,
                percent,
            },
        );
    };
    emit(0.0);
    let mut last = -1.0f32;
    st.solid.fill(cid, &meta.signature, |dir| {
        reader
            .extract_entries(
                &entries,
                dir,
                &mut |p| {
                    if p - last >= 1.0 || p >= 100.0 {
                        last = p;
                        emit(p);
                    }
                },
                cancel,
            )
            .map(drop)
    })?;
    emit(100.0);
    st.solid
        .lookup(cid, &meta.signature, rel_path)
        .ok_or_else(|| AppError::NotFound(format!("{rel_path} was not in the archive")))
}

/// Real files a slicer can open, extracting only the selected archive entries when needed.
pub fn real_paths(st: &AppState, model_ids: &[i64]) -> AppResult<Vec<PathBuf>> {
    let mut by_container: HashMap<i64, Vec<ModelInfo>> = HashMap::new();
    let mut order: Vec<i64> = Vec::new();
    let mut containers: HashMap<i64, ContainerInfo> = HashMap::new();
    {
        let conn = st.db.lock()?;
        for id in model_ids {
            let m = queries::get_model(&conn, *id)?;
            if let std::collections::hash_map::Entry::Vacant(e) = containers.entry(m.container_id) {
                e.insert(queries::get_container(&conn, m.container_id)?);
                order.push(m.container_id);
            }
            by_container.entry(m.container_id).or_default().push(m);
        }
    }
    let mut resolved: HashMap<i64, PathBuf> = HashMap::new();
    for cid in order {
        let models = by_container.remove(&cid).unwrap_or_default();
        let Some(container) = containers.remove(&cid) else {
            continue;
        };
        if container.kind == "folder" {
            for m in models {
                let p = PathBuf::from(&m.rel_path);
                if !p.is_file() {
                    return Err(AppError::NotFound(format!(
                        "{} is no longer on disk",
                        m.name
                    )));
                }
                resolved.insert(m.id, p);
            }
            continue;
        }
        check_readable(&container)?;
        let meta = {
            let conn = st.db.lock()?;
            archive_meta(&conn, container)?
        };
        let mut todo = Vec::new();
        for m in models {
            if m.encrypted {
                return Err(AppError::Archive(ArchiveFailure::Encrypted));
            }
            match st.solid.lookup(cid, &meta.signature, &m.rel_path) {
                Some(p) => {
                    resolved.insert(m.id, p);
                }
                None => todo.push(m),
            }
        }
        if todo.is_empty() {
            continue;
        }
        for (id, p) in extract_for_launch(st, &meta, &todo)? {
            resolved.insert(id, p);
        }
    }
    model_ids
        .iter()
        .map(|id| {
            resolved
                .remove(id)
                .ok_or_else(|| AppError::NotFound(format!("model {id} could not be prepared")))
        })
        .collect()
}

/// Extract just these entries into `extracted/<hash>/`, flattened with their original names.
fn extract_for_launch(
    st: &AppState,
    meta: &ArchiveMeta,
    models: &[ModelInfo],
) -> AppResult<Vec<(i64, PathBuf)>> {
    let mut rels: Vec<&str> = models.iter().map(|m| m.rel_path.as_str()).collect();
    rels.sort_unstable();
    let dir = st
        .extract_root
        .join(stable_hash(&(meta.container.id, &meta.signature, &rels)));
    let marker = dir.join(".stlx-done.json");
    if let Ok(text) = std::fs::read_to_string(&marker) {
        if let Ok(map) = serde_json::from_str::<HashMap<String, PathBuf>>(&text) {
            let hits: Vec<(i64, PathBuf)> = models
                .iter()
                .filter_map(|m| map.get(&m.rel_path).map(|p| (m.id, p.clone())))
                .filter(|(_, p)| p.is_file())
                .collect();
            if hits.len() == models.len() {
                // Touch so the 7 day clean-up counts from the last use.
                let _ = std::fs::write(&marker, &text);
                return Ok(hits);
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    let tmp = dir.join(".tmp");
    std::fs::create_dir_all(&tmp)?;
    let reader = open_reader(&meta.source, &st.sz)?;
    let entries: Vec<String> = models.iter().map(|m| m.rel_path.clone()).collect();
    let done = reader.extract_entries(&entries, &tmp, &mut |_| {}, &Cancel::new())?;
    let mut map: HashMap<String, PathBuf> = HashMap::new();
    for (rel, p) in done {
        let name = rel.rsplit('/').next().unwrap_or(&rel).to_string();
        let target = unique_path(&dir, &name);
        std::fs::rename(&p, &target)?;
        map.insert(rel, target);
    }
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::write(&marker, serde_json::to_string(&map)?)?;
    Ok(models
        .iter()
        .filter_map(|m| map.get(&m.rel_path).map(|p| (m.id, p.clone())))
        .collect())
}
