//! Tauri commands. A thin layer: real work lives in the modules.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::archive::Archive;
use crate::import::{self, Candidate, ImportItem};
use crate::library::{Direction, Library, SeriesType, Storage};
use crate::settings::Settings;
use crate::state::AppState;
use crate::watch;

pub const EVENT_IMPORT_PROGRESS: &str = "import-progress";
pub const EVENT_IMPORT_FINISHED: &str = "import-finished";
pub const EVENT_LIBRARY_CHANGED: &str = "library-changed";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    library_folder: String,
    data_folder: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    volume_id: String,
    /// e.g. `ZIP64 · stored`. Empty when the file is missing.
    archive: String,
    open_ms: f64,
    missing: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheInfo {
    /// Covers and thumbnails on disk.
    thumb_bytes: u64,
    /// Pages held in memory right now.
    page_bytes: u64,
}

/// Saves the library and tells the UI it changed.
fn persist(app: &AppHandle, state: &AppState, library: &Library) -> Result<(), String> {
    state
        .store
        .save(library)
        .map_err(|err| format!("could not save the library: {err}"))?;
    let _ = app.emit(EVENT_LIBRARY_CHANGED, ());
    Ok(())
}

// --- library ---

#[tauri::command]
pub async fn get_library(state: State<'_, AppState>) -> Result<Library, String> {
    Ok(state.library().clone())
}

#[tauri::command]
pub async fn get_app_info(state: State<'_, AppState>) -> Result<AppInfo, String> {
    Ok(AppInfo {
        library_folder: state.library_folder().display().to_string(),
        data_folder: state.store.dir().display().to_string(),
    })
}

/// Edits a series. Returns the id of the series that now holds its volumes
/// (a different id if the new title merged it into another series).
#[tauri::command]
pub async fn update_series(
    app: AppHandle,
    state: State<'_, AppState>,
    series_id: String,
    title: String,
    title_jp: Option<String>,
    series_type: SeriesType,
    direction: Direction,
) -> Result<String, String> {
    let mut library = state.library_mut();
    let id = library
        .update_series(&series_id, &title, title_jp, series_type, direction)
        .ok_or("the series was not found, or the title is empty")?;
    persist(&app, &state, &library)?;
    Ok(id)
}

/// Removes a volume from the library. The file on disk is left alone.
#[tauri::command]
pub async fn remove_volume(app: AppHandle, state: State<'_, AppState>, volume_id: String) -> Result<(), String> {
    let removed = {
        let mut library = state.library_mut();
        let removed = library.remove_volume(&volume_id).ok_or("volume is not in the library")?;
        persist(&app, &state, &library)?;
        removed
    };
    state.forget_volume(&removed);
    Ok(())
}

/// Removes a series from the library. The files on disk are left alone.
#[tauri::command]
pub async fn remove_series(app: AppHandle, state: State<'_, AppState>, series_id: String) -> Result<(), String> {
    let removed = {
        let mut library = state.library_mut();
        let removed = library.remove_series(&series_id).ok_or("series is not in the library")?;
        persist(&app, &state, &library)?;
        removed
    };
    for volume in &removed {
        state.forget_volume(volume);
    }
    Ok(())
}

#[tauri::command]
pub async fn set_read(
    app: AppHandle,
    state: State<'_, AppState>,
    volume_id: String,
    read: bool,
    now: String,
) -> Result<(), String> {
    let mut library = state.library_mut();
    if !library.set_read(&volume_id, read, now) {
        return Err("volume is not in the library".into());
    }
    persist(&app, &state, &library)
}

/// Archive details for a series' files table. Each file is opened fresh so the
/// open time is real. A file that cannot be found is reported as missing.
#[tauri::command]
pub async fn get_series_files(app: AppHandle, series_id: String) -> Result<Vec<FileInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let volumes: Vec<(String, String)> = state
            .library()
            .series
            .iter()
            .filter(|series| series.id == series_id)
            .flat_map(|series| &series.volumes)
            .map(|volume| (volume.id.clone(), volume.path.clone()))
            .collect();
        volumes
            .into_iter()
            .map(|(volume_id, path)| {
                let started = Instant::now();
                match Archive::open(Path::new(&path)) {
                    Ok(archive) => FileInfo {
                        volume_id,
                        archive: archive.describe(),
                        open_ms: started.elapsed().as_secs_f64() * 1000.0,
                        missing: false,
                    },
                    Err(_) => FileInfo { volume_id, archive: String::new(), open_ms: 0.0, missing: true },
                }
            })
            .collect()
    })
    .await
    .map_err(|err| err.to_string())
}

// --- reader ---

/// `[width, height]` of every page, read from image headers off the UI thread.
#[tauri::command]
pub async fn get_page_sizes(app: AppHandle, volume_id: String) -> Result<Vec<(u32, u32)>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>().page_sizes(&volume_id).map(|sizes| sizes.to_vec())
    })
    .await
    .map_err(|err| err.to_string())?
}

/// Page indexes that are ready in the memory cache.
#[tauri::command]
pub async fn get_cached_pages(state: State<'_, AppState>, volume_id: String) -> Result<Vec<usize>, String> {
    Ok(state.pages.cached_pages(&volume_id))
}

/// Saves the page being read (1-based). `now` is an ISO timestamp, `today` a local date.
#[tauri::command]
pub async fn set_progress(
    app: AppHandle,
    state: State<'_, AppState>,
    volume_id: String,
    page: usize,
    now: String,
    today: String,
) -> Result<(), String> {
    let mut library = state.library_mut();
    if !library.set_progress(&volume_id, page, now, today) {
        return Err("volume is not in the library".into());
    }
    persist(&app, &state, &library)
}

// --- settings and caches ---

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    Ok(state.settings())
}

/// Saves settings, applies them, and restarts folder watching if the folders changed.
#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<Settings, String> {
    let before = state.settings().watch_folders;
    let saved = state
        .save_settings(settings)
        .map_err(|err| format!("could not save settings: {err}"))?;
    if saved.watch_folders != before {
        restart_watcher(&app, &state);
    }
    Ok(saved)
}

/// (Re)starts watching the folders named in the settings.
pub fn restart_watcher(app: &AppHandle, state: &AppState) {
    let watcher = watch::start(app.clone(), state.settings().watch_folders);
    *state.watcher.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = watcher;
}

/// Archives in the watched folders that are not in the library yet.
#[tauri::command]
pub async fn check_watch_folders(app: AppHandle) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        watch::find_new_files(&state, &state.settings().watch_folders)
    })
    .await
    .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn get_cache_info(app: AppHandle) -> Result<CacheInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        CacheInfo {
            thumb_bytes: state.thumbs.size_bytes(),
            page_bytes: state.pages.total_bytes() as u64,
        }
    })
    .await
    .map_err(|err| err.to_string())
}

/// Deletes all cached covers and thumbnails. They are rebuilt when next shown.
#[tauri::command]
pub async fn clear_thumb_cache(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>()
            .thumbs
            .clear()
            .map_err(|err| format!("could not clear the cache: {err}"))
    })
    .await
    .map_err(|err| err.to_string())?
}

// --- import ---

/// Scans files and folders for archives. Reads indexes only, off the UI thread.
#[tauri::command]
pub async fn scan_import(app: AppHandle, paths: Vec<String>) -> Result<Vec<Candidate>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let library = state.library().clone();
        import::scan(&paths, &library, state.settings().default_direction)
    })
    .await
    .map_err(|err| err.to_string())
}

/// Starts importing in the background. Progress arrives as `import-progress`
/// events, then one `import-finished`.
#[tauri::command]
pub async fn run_import(
    app: AppHandle,
    state: State<'_, AppState>,
    items: Vec<ImportItem>,
    storage: Storage,
) -> Result<(), String> {
    if state.import_running.swap(true, Ordering::SeqCst) {
        return Err("an import is already running".into());
    }
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        import::import_all(&state, &items, storage, &|progress| {
            if progress.step == "saved" {
                let _ = app.emit(EVENT_LIBRARY_CHANGED, ());
            }
            let _ = app.emit(EVENT_IMPORT_PROGRESS, progress);
        });
        state.import_running.store(false, Ordering::SeqCst);
        let _ = app.emit(EVENT_IMPORT_FINISHED, ());
    });
    Ok(())
}
