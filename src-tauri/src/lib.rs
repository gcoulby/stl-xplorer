mod archive;
mod autotag;
mod commands;
mod db;
mod error;
mod index;
mod orientation;
mod packs;
mod preview;
mod protocol;
mod slicer;
mod state;
mod types;
mod util;
mod vault;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::Manager;

use crate::archive::cache::{clean_extracted, MeshCache, SolidCache};
use crate::archive::sevenzip::SevenZip;
use crate::error::AppResult;
use crate::state::{AppState, MESH_CACHE_BYTES};

fn setup(app: &tauri::App) -> AppResult<()> {
    let handle = app.handle().clone();
    let data_dir = app.path().app_data_dir()?;
    let cache_root = app.path().app_cache_dir()?;
    std::fs::create_dir_all(&data_dir)?;
    std::fs::create_dir_all(&cache_root)?;
    let extract_root = cache_root.join("extracted");
    std::fs::create_dir_all(&extract_root)?;
    clean_extracted(&extract_root, Duration::from_secs(7 * 24 * 3600));

    let db = db::Db::open(data_dir.join("library.db"))?;
    let solid_cap = {
        let conn = db.lock()?;
        commands::library::ensure_vault(&conn, &commands::library::default_vault_path(&handle))?;
        commands::library::solid_cap(&conn)
    };

    app.manage(AppState {
        db,
        sz: Arc::new(SevenZip::discover()),
        mesh_cache: Mutex::new(MeshCache::new(MESH_CACHE_BYTES)),
        solid: SolidCache::new(cache_root.join("archives"), solid_cap),
        cache_root,
        extract_root,
        data_dir,
        preview: Mutex::new(None),
        scans: Default::default(),
        watcher: Mutex::new(None),
    });

    if let Err(e) = index::watcher::restart(&handle) {
        eprintln!("file watcher unavailable: {e}");
    }
    // Startup rescan. Incremental, so unchanged files cost one stat each.
    let st = handle.state::<AppState>();
    let libs = db::queries::list_libraries(&*st.db.lock()?)?;
    for l in libs.into_iter().filter(|l| l.enabled) {
        st.scans.request(&handle, l.id);
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol(protocol::SCHEME, |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            // Decompression can take a while; keep it off the webview's thread.
            std::thread::spawn(move || responder.respond(protocol::handle(&app, &request)));
        })
        .setup(|app| {
            setup(app).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::library::list_libraries,
            commands::library::add_library,
            commands::library::update_library,
            commands::library::remove_library,
            commands::library::rescan_library,
            commands::library::rescan_all,
            commands::library::running_scans,
            commands::library::get_settings,
            commands::library::set_vault_path,
            commands::library::set_solid_cache_cap,
            commands::library::set_global_rotation,
            commands::library::sevenzip_info,
            commands::library::cache_stats,
            commands::library::clear_cache,
            commands::library::clean_up_missing,
            commands::index::browse,
            commands::index::model_detail,
            commands::index::container_detail,
            commands::index::set_model_rotation,
            commands::index::set_pack_rotation,
            commands::index::set_pack,
            commands::mesh::cancel_preview,
            commands::search::search,
            commands::slicer::list_slicers,
            commands::slicer::detect_slicers,
            commands::slicer::add_slicer,
            commands::slicer::update_slicer,
            commands::slicer::delete_slicer,
            commands::slicer::set_default_slicer,
            commands::slicer::open_in_slicer,
            commands::tags::list_tags,
            commands::tags::create_tag,
            commands::tags::update_tag,
            commands::tags::delete_tag,
            commands::tags::tag_items,
            commands::tags::tagged_items,
            commands::tags::suggest_tags,
            commands::index::problem_archives,
            commands::index::archive_problem,
            commands::files::reveal_in_folder,
            commands::files::open_externally,
            commands::delete::delete_items,
            commands::packs::find_unpacked,
            commands::packs::delete_unpacked,
            commands::packs::zip_folders,
            commands::index::resolve_location,
            commands::tags::set_autotag_on_index,
            commands::tags::apply_auto_tags,
            commands::tags::list_collections,
            commands::tags::create_collection,
            commands::tags::rename_collection,
            commands::tags::delete_collection,
            commands::tags::collection_add,
            commands::tags::collection_items,
            commands::vault::vault_save,
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("STL Xplorer failed to start: {e}");
        std::process::exit(1);
    }
}
