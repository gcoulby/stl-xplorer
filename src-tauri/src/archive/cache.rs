//! Two caches: recently decompressed meshes in memory, and whole-archive mesh extractions for
//! solid archives on disk. Both are capped by bytes and evict least recently used first.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::util::now_secs;

pub struct MeshCache {
    cap: u64,
    total: u64,
    tick: u64,
    items: HashMap<i64, (Arc<Vec<u8>>, u64)>,
}

impl MeshCache {
    pub fn new(cap: u64) -> Self {
        Self {
            cap,
            total: 0,
            tick: 0,
            items: HashMap::new(),
        }
    }

    pub fn get(&mut self, id: i64) -> Option<Arc<Vec<u8>>> {
        self.tick += 1;
        let tick = self.tick;
        self.items.get_mut(&id).map(|(b, t)| {
            *t = tick;
            b.clone()
        })
    }

    pub fn insert(&mut self, id: i64, bytes: Arc<Vec<u8>>) {
        let len = bytes.len() as u64;
        if len > self.cap {
            return;
        }
        self.tick += 1;
        if let Some((old, _)) = self.items.insert(id, (bytes, self.tick)) {
            self.total -= old.len() as u64;
        }
        self.total += len;
        while self.total > self.cap {
            let Some((&victim, _)) = self.items.iter().min_by_key(|(_, (_, t))| *t) else {
                break;
            };
            if let Some((b, _)) = self.items.remove(&victim) {
                self.total -= b.len() as u64;
            }
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.total = 0;
    }

    pub fn total(&self) -> u64 {
        self.total
    }
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    signature: String,
    bytes: u64,
    last_used: i64,
}

const MANIFEST: &str = ".stlx-manifest.json";

/// On-disk cache for solid archives: the first preview extracts every mesh in the archive to
/// `<root>/<container_id>/`, and later previews read from there.
pub struct SolidCache {
    root: PathBuf,
    cap: AtomicU64,
    /// One lock per container so two previews never extract the same archive at once.
    locks: Mutex<HashMap<i64, Arc<Mutex<()>>>>,
}

impl SolidCache {
    pub fn new(root: PathBuf, cap: u64) -> Self {
        let _ = std::fs::create_dir_all(&root);
        // Leftovers from an extraction interrupted by a crash.
        if let Ok(rd) = std::fs::read_dir(&root) {
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().ends_with(".partial") {
                    let _ = std::fs::remove_dir_all(e.path());
                }
            }
        }
        Self {
            root,
            cap: AtomicU64::new(cap),
            locks: Mutex::new(HashMap::new()),
        }
    }

    pub fn set_cap(&self, cap: u64) {
        self.cap.store(cap, Ordering::SeqCst);
        let _ = self.enforce_cap(None);
    }

    pub fn cap(&self) -> u64 {
        self.cap.load(Ordering::SeqCst)
    }

    pub fn lock_for(&self, container_id: i64) -> AppResult<Arc<Mutex<()>>> {
        let mut locks = self
            .locks
            .lock()
            .map_err(|_| AppError::Other("cache lock poisoned".into()))?;
        Ok(locks.entry(container_id).or_default().clone())
    }

    fn dir(&self, container_id: i64) -> PathBuf {
        self.root.join(container_id.to_string())
    }

    fn read_manifest(dir: &Path) -> Option<Manifest> {
        let text = std::fs::read_to_string(dir.join(MANIFEST)).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// The cached copy of an entry, if this archive (at this signature) has been unpacked.
    pub fn lookup(&self, container_id: i64, signature: &str, rel_path: &str) -> Option<PathBuf> {
        let dir = self.dir(container_id);
        let mut m = Self::read_manifest(&dir)?;
        if m.signature != signature {
            return None;
        }
        let p = super::safe_join(&dir, rel_path)?;
        if !p.is_file() {
            return None;
        }
        let now = now_secs();
        if now - m.last_used > 60 {
            m.last_used = now;
            if let Ok(text) = serde_json::to_string(&m) {
                let _ = std::fs::write(dir.join(MANIFEST), text);
            }
        }
        Some(p)
    }

    #[cfg(test)]
    pub fn is_filled(&self, container_id: i64, signature: &str) -> bool {
        Self::read_manifest(&self.dir(container_id)).is_some_and(|m| m.signature == signature)
    }

    /// Run `extract` into a scratch folder and swap it in when it succeeds, so a cancelled or
    /// failed extraction never leaves a half-filled cache behind.
    pub fn fill(
        &self,
        container_id: i64,
        signature: &str,
        extract: impl FnOnce(&Path) -> AppResult<()>,
    ) -> AppResult<()> {
        let dir = self.dir(container_id);
        let partial = self.root.join(format!("{container_id}.partial"));
        let _ = std::fs::remove_dir_all(&partial);
        std::fs::create_dir_all(&partial)?;
        if let Err(e) = extract(&partial) {
            let _ = std::fs::remove_dir_all(&partial);
            return Err(e);
        }
        let bytes = dir_size(&partial);
        let m = Manifest {
            signature: signature.to_string(),
            bytes,
            last_used: now_secs(),
        };
        std::fs::write(partial.join(MANIFEST), serde_json::to_string(&m)?)?;
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::rename(&partial, &dir)?;
        self.enforce_cap(Some(container_id))
    }

    pub fn invalidate(&self, container_id: i64) {
        let _ = std::fs::remove_dir_all(self.dir(container_id));
    }

    /// Evict whole archives, oldest first, until under the cap. `keep` is never evicted, even
    /// if it alone is bigger than the cap, because a preview is waiting on it.
    pub fn enforce_cap(&self, keep: Option<i64>) -> AppResult<()> {
        let mut entries = self.entries();
        let mut total: u64 = entries.iter().map(|e| e.2).sum();
        let cap = self.cap();
        entries.sort_by_key(|e| e.1);
        for (id, _, bytes) in entries {
            if total <= cap {
                break;
            }
            if Some(id) == keep {
                continue;
            }
            self.invalidate(id);
            total = total.saturating_sub(bytes);
        }
        Ok(())
    }

    fn entries(&self) -> Vec<(i64, i64, u64)> {
        let Ok(rd) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        rd.flatten()
            .filter_map(|e| {
                let id: i64 = e.file_name().to_str()?.parse().ok()?;
                let m = Self::read_manifest(&e.path())?;
                Some((id, m.last_used, m.bytes))
            })
            .collect()
    }

    pub fn stats(&self) -> (u64, u64) {
        let e = self.entries();
        (e.iter().map(|x| x.2).sum(), e.len() as u64)
    }

    pub fn clear(&self) -> AppResult<()> {
        if let Ok(rd) = std::fs::read_dir(&self.root) {
            for e in rd.flatten() {
                if e.path().is_dir() {
                    std::fs::remove_dir_all(e.path())?;
                }
            }
        }
        Ok(())
    }
}

pub fn dir_size(dir: &Path) -> u64 {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// Remove launch folders older than `max_age` from the slicer extraction cache.
pub fn clean_extracted(root: &Path, max_age: Duration) {
    let Ok(rd) = std::fs::read_dir(root) else {
        return;
    };
    let now = SystemTime::now();
    for e in rd.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > max_age);
        if old {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_cache_evicts_lru_by_bytes() {
        let mut c = MeshCache::new(10);
        c.insert(1, Arc::new(vec![0; 4]));
        c.insert(2, Arc::new(vec![0; 4]));
        assert!(c.get(1).is_some()); // 1 is now newer than 2
        c.insert(3, Arc::new(vec![0; 4]));
        assert!(c.get(2).is_none());
        assert!(c.get(1).is_some());
        assert!(c.get(3).is_some());
        assert_eq!(c.total(), 8);
        c.insert(4, Arc::new(vec![0; 11])); // bigger than the cap, never cached
        assert!(c.get(4).is_none());
        assert_eq!(c.total(), 8);
    }

    #[test]
    fn solid_cache_fill_lookup_and_cap() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = SolidCache::new(dir.path().to_path_buf(), 15);
        cache
            .fill(1, "sig1", |d| {
                std::fs::create_dir_all(d.join("a"))?;
                std::fs::write(d.join("a/x.stl"), [0u8; 10])?;
                Ok(())
            })
            .expect("fill 1");
        assert!(cache.lookup(1, "sig1", "a/x.stl").is_some());
        assert!(cache.lookup(1, "other", "a/x.stl").is_none());
        assert!(cache.lookup(1, "sig1", "../x.stl").is_none());

        // A failed fill leaves nothing behind.
        let r = cache.fill(3, "sig3", |_| Err(AppError::Cancelled));
        assert!(r.is_err());
        assert!(!cache.is_filled(3, "sig3"));
        assert!(!dir.path().join("3.partial").exists());

        // Archive 2 pushes the total over 15 bytes, so the older archive 1 goes.
        cache
            .fill(2, "sig2", |d| {
                std::fs::write(d.join("y.stl"), [0u8; 10])?;
                Ok(())
            })
            .expect("fill 2");
        assert!(!cache.is_filled(1, "sig1"));
        assert!(cache.is_filled(2, "sig2"));
    }
}
