use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::archive::cache::{MeshCache, SolidCache};
use crate::archive::sevenzip::SevenZip;
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::index::watcher::LibraryWatcher;
use crate::index::ScanManager;
use crate::util::Cancel;

pub const MESH_CACHE_BYTES: u64 = 256 * 1024 * 1024;
pub const DEFAULT_SOLID_CAP: u64 = 5 * 1024 * 1024 * 1024;
pub const SOLID_CAP_KEY: &str = "solid_cache_cap_bytes";
pub const VAULT_KEY: &str = "vault_path";

pub struct AppState {
    pub db: Db,
    pub sz: Arc<SevenZip>,
    pub mesh_cache: Mutex<MeshCache>,
    pub solid: SolidCache,
    pub cache_root: PathBuf,
    pub extract_root: PathBuf,
    pub data_dir: PathBuf,
    /// The archive currently being previewed and a handle to cancel its 7-Zip process.
    pub preview: Mutex<Option<(i64, Cancel)>>,
    pub scans: ScanManager,
    pub watcher: Mutex<Option<LibraryWatcher>>,
}

impl AppState {
    pub fn mesh_cache(&self) -> AppResult<std::sync::MutexGuard<'_, MeshCache>> {
        self.mesh_cache
            .lock()
            .map_err(|_| AppError::Other("mesh cache lock poisoned".into()))
    }

    /// Start a preview of a model in `container_id`. Supersedes any preview still loading,
    /// except an unpack of the same solid archive, which the new preview also needs.
    pub fn begin_preview(&self, container_id: i64, solid: bool) -> Cancel {
        let Ok(mut active) = self.preview.lock() else {
            return Cancel::new();
        };
        if let Some((cid, c)) = active.as_ref() {
            if *cid == container_id && solid && !c.is_cancelled() {
                return c.clone();
            }
            c.cancel();
        }
        let c = Cancel::new();
        *active = Some((container_id, c.clone()));
        c
    }

    pub fn cancel_preview(&self) {
        if let Ok(mut active) = self.preview.lock() {
            if let Some((_, c)) = active.take() {
                c.cancel();
            }
        }
    }

    /// Forget cached bytes for archives whose contents changed.
    pub fn invalidate_archives(&self, ids: &[i64]) {
        if ids.is_empty() {
            return;
        }
        for id in ids {
            self.solid.invalidate(*id);
        }
        if let Ok(mut c) = self.mesh_cache.lock() {
            c.clear();
        }
    }
}
