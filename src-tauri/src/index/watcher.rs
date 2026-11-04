//! Watches enabled libraries and queues an incremental rescan once a burst of changes settles.
//! The scanner itself holds back archive parts that are still arriving.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

const SETTLE: Duration = Duration::from_secs(2);

pub struct LibraryWatcher {
    _watcher: RecommendedWatcher,
}

pub fn start(app: AppHandle, libraries: Vec<(i64, PathBuf)>) -> AppResult<LibraryWatcher> {
    let (tx, rx) = mpsc::channel::<Vec<PathBuf>>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            if !matches!(ev.kind, EventKind::Access(_)) {
                let _ = tx.send(ev.paths);
            }
        }
    })
    .map_err(|e| AppError::Other(format!("file watcher: {e}")))?;

    for (_, path) in &libraries {
        if path.is_dir() {
            // A library on a drive that cannot be watched still works; it just needs rescans.
            let _ = watcher.watch(path, RecursiveMode::Recursive);
        }
    }

    // Ends by itself when the watcher (and so the sender) is dropped.
    std::thread::spawn(move || {
        let mut pending: HashSet<i64> = HashSet::new();
        loop {
            match rx.recv_timeout(SETTLE) {
                Ok(paths) => {
                    for p in paths {
                        let best = libraries
                            .iter()
                            .filter(|(_, root)| p.starts_with(root))
                            .max_by_key(|(_, root)| root.as_os_str().len());
                        if let Some((id, _)) = best {
                            pending.insert(*id);
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if pending.is_empty() {
                        continue;
                    }
                    let st = app.state::<AppState>();
                    for id in pending.drain() {
                        st.scans.request(&app, id);
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    Ok(LibraryWatcher { _watcher: watcher })
}

/// Re-create the watcher for the current set of enabled libraries.
pub fn restart(app: &AppHandle) -> AppResult<()> {
    let st = app.state::<AppState>();
    let libs: Vec<(i64, PathBuf)> = {
        let conn = st.db.lock()?;
        crate::db::queries::list_libraries(&conn)?
            .into_iter()
            .filter(|l| l.enabled)
            .map(|l| (l.id, PathBuf::from(l.path)))
            .collect()
    };
    let w = start(app.clone(), libs)?;
    let mut slot = st
        .watcher
        .lock()
        .map_err(|_| AppError::Other("watcher lock poisoned".into()))?;
    *slot = Some(w);
    Ok(())
}
