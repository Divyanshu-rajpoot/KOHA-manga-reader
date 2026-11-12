//! KOMA backend. Everything that touches files lives here.

mod archive;
mod cache;
mod commands;
mod import;
mod library;
mod protocol;
mod settings;
mod state;
#[cfg(test)]
mod testutil;
mod watch;

use directories::{BaseDirs, UserDirs};
use tauri::Manager;

use state::AppState;

const APP_FOLDER: &str = "Koma";

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dirs = BaseDirs::new().ok_or("could not find the user's folders")?;
            // %APPDATA%\Koma for the library; %LOCALAPPDATA%\Koma for caches.
            let data_dir = dirs.data_dir().join(APP_FOLDER);
            let cache_dir = dirs.data_local_dir().join(APP_FOLDER);
            let library_folder = UserDirs::new()
                .and_then(|user| user.document_dir().map(|documents| documents.to_path_buf()))
                .unwrap_or_else(|| dirs.home_dir().to_path_buf())
                .join("KOMA Library");
            app.manage(AppState::new(&data_dir, &cache_dir, library_folder)?);
            commands::restart_watcher(app.handle(), &app.state::<AppState>());
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol("koma", protocol::handle)
        .invoke_handler(tauri::generate_handler![
            commands::get_library,
            commands::get_app_info,
            commands::update_series,
            commands::remove_volume,
            commands::remove_series,
            commands::set_read,
            commands::get_series_files,
            commands::get_page_sizes,
            commands::get_cached_pages,
            commands::set_progress,
            commands::get_settings,
            commands::save_settings,
            commands::check_watch_folders,
            commands::get_cache_info,
            commands::clear_thumb_cache,
            commands::scan_import,
            commands::run_import,
        ])
        .run(tauri::generate_context!())
        .expect("error while running KOMA");
}
