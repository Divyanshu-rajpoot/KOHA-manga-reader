//! Watches the user's library folders for new archives.
//!
//! When files appear (and once at startup), the folders are scanned and any
//! archive that is not yet in the library is reported to the UI with a
//! `watch-found` event. Nothing is imported automatically.

use std::sync::mpsc;
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter, Manager};

use crate::import::expand_paths;
use crate::state::AppState;

pub const EVENT_WATCH_FOUND: &str = "watch-found";
/// Wait for the folder to go quiet (a copy to finish) before scanning.
const QUIET_PERIOD: Duration = Duration::from_millis(1500);

/// Archives in the watched folders that are not in the library.
pub fn find_new_files(state: &AppState, folders: &[String]) -> Vec<String> {
    let found = expand_paths(folders);
    let library = state.library();
    found
        .into_iter()
        .map(|path| path.display().to_string())
        .filter(|path| library.volume_by_path(path).is_none())
        .collect()
}

/// Starts watching. Dropping the returned watcher stops it.
pub fn start(app: AppHandle, folders: Vec<String>) -> Option<RecommendedWatcher> {
    if folders.is_empty() {
        let _ = app.emit(EVENT_WATCH_FOUND, Vec::<String>::new());
        return None;
    }

    let (sender, receiver) = mpsc::channel::<()>();
    let _ = sender.send(()); // scan once right away
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.is_ok() {
            let _ = sender.send(());
        }
    })
    .ok()?;
    for folder in &folders {
        // A folder that is missing (an unplugged drive) is skipped, not fatal.
        let _ = watcher.watch(std::path::Path::new(folder), RecursiveMode::Recursive);
    }

    std::thread::spawn(move || {
        // Ends when the watcher (and with it the sender) is dropped.
        while receiver.recv().is_ok() {
            while receiver.recv_timeout(QUIET_PERIOD).is_ok() {}
            let state = app.state::<AppState>();
            let _ = app.emit(EVENT_WATCH_FOUND, find_new_files(&state, &folders));
        }
    });
    Some(watcher)
}
