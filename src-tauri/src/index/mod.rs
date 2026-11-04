pub mod scanner;
pub mod watcher;

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;
use crate::types::{ScanFinished, ScanProgress};
use crate::util::Cancel;

struct Slot {
    cancel: Cancel,
    rerun: bool,
}

/// At most one scan per library at a time. A request during a scan queues one rerun.
#[derive(Default)]
pub struct ScanManager {
    slots: Mutex<HashMap<i64, Slot>>,
}

impl ScanManager {
    pub fn request(&self, app: &AppHandle, library_id: i64) {
        let Ok(mut slots) = self.slots.lock() else {
            return;
        };
        if let Some(slot) = slots.get_mut(&library_id) {
            slot.rerun = true;
            return;
        }
        let cancel = Cancel::new();
        slots.insert(
            library_id,
            Slot {
                cancel: cancel.clone(),
                rerun: false,
            },
        );
        let app = app.clone();
        std::thread::spawn(move || run_loop(app, library_id, cancel));
    }

    pub fn cancel(&self, library_id: i64) {
        if let Ok(mut slots) = self.slots.lock() {
            if let Some(slot) = slots.remove(&library_id) {
                slot.cancel.cancel();
            }
        }
    }

    pub fn running(&self) -> Vec<i64> {
        self.slots
            .lock()
            .map(|s| s.keys().copied().collect())
            .unwrap_or_default()
    }

    /// True if another pass was asked for; clears the slot otherwise.
    fn finish(&self, library_id: i64) -> bool {
        let Ok(mut slots) = self.slots.lock() else {
            return false;
        };
        match slots.get_mut(&library_id) {
            Some(slot) if slot.rerun && !slot.cancel.is_cancelled() => {
                slot.rerun = false;
                true
            }
            _ => {
                slots.remove(&library_id);
                false
            }
        }
    }
}

fn run_loop(app: AppHandle, library_id: i64, cancel: Cancel) {
    loop {
        let deferred = run_once(&app, library_id, &cancel);
        if deferred {
            let app = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(scanner::QUIET_SECS as u64 + 1));
                app.state::<AppState>().scans.request(&app, library_id);
            });
        }
        if !app.state::<AppState>().scans.finish(library_id) {
            break;
        }
    }
}

fn run_once(app: &AppHandle, library_id: i64, cancel: &Cancel) -> bool {
    let st = app.state::<AppState>();
    let progress = |p: ScanProgress| {
        let _ = app.emit("scan-progress", p);
    };
    let result = st.db.new_connection().and_then(|mut conn| {
        let lib = scanner::library_ref(&conn, library_id)?;
        let out = scanner::scan_library(&mut conn, &lib, &st.sz, &progress, cancel);
        Ok((lib, out))
    });
    let (name, finished) = match result {
        Ok((lib, Ok(out))) => {
            st.invalidate_archives(&out.changed_archives);
            let deferred = out.deferred;
            (
                lib.name,
                ScanFinished {
                    library_id,
                    library_name: String::new(),
                    changed: out.changed,
                    models: out.models,
                    errors: out.errors,
                    deferred,
                    error: None,
                },
            )
        }
        Ok((lib, Err(e))) => (lib.name, failed(library_id, e.to_string())),
        Err(e) => (String::new(), failed(library_id, e.to_string())),
    };
    let deferred = finished.deferred;
    let _ = app.emit(
        "scan-finished",
        ScanFinished {
            library_name: name,
            ..finished
        },
    );
    deferred
}

fn failed(library_id: i64, error: String) -> ScanFinished {
    ScanFinished {
        library_id,
        library_name: String::new(),
        changed: 0,
        models: 0,
        errors: 1,
        deferred: false,
        error: Some(error),
    }
}
