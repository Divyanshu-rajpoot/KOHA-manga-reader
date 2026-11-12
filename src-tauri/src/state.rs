//! Shared application state, managed by Tauri.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};

use rayon::prelude::*;

use crate::archive::{Archive, ArchiveRegistry};
use crate::cache::pages::{dimensions, PageCache};
use crate::cache::{cache_key, ThumbCache};
use crate::library::store::LibraryStore;
use crate::library::Library;
use crate::settings::Settings;

/// Enough of a page to hold its image header, even with a large EXIF or colour profile.
const HEADER_BYTES: usize = 64 * 1024;

/// Width and height of every page of a volume. `(0, 0)` means unknown.
pub type PageSizes = Arc<Vec<(u32, u32)>>;

pub struct AppState {
    library: RwLock<Library>,
    pub store: LibraryStore,
    pub archives: ArchiveRegistry,
    pub thumbs: ThumbCache,
    pub pages: Arc<PageCache>,
    page_sizes: Mutex<HashMap<String, PageSizes>>,
    settings: RwLock<Settings>,
    /// Used when the settings do not name a library folder.
    default_library_folder: PathBuf,
    /// Kept alive while folders are being watched.
    pub watcher: Mutex<Option<notify::RecommendedWatcher>>,
    pub import_running: AtomicBool,
}

impl AppState {
    /// `data_dir` holds library.json; `cache_dir` holds covers and thumbnails.
    pub fn new(data_dir: &Path, cache_dir: &Path, library_folder: PathBuf) -> io::Result<Self> {
        let store = LibraryStore::new(data_dir);
        let settings = Settings::load(data_dir, &library_folder);
        let pages = Arc::new(PageCache::new(settings.cache_mb as usize * 1024 * 1024));
        pages.configure(settings.cache_mb as usize * 1024 * 1024, settings.preload_ahead, settings.preload_behind);
        Ok(Self {
            library: RwLock::new(store.load()?),
            store,
            archives: ArchiveRegistry::default(),
            thumbs: ThumbCache::new(cache_dir.join("thumbs")),
            pages,
            page_sizes: Mutex::new(HashMap::new()),
            settings: RwLock::new(settings),
            default_library_folder: library_folder,
            watcher: Mutex::new(None),
            import_running: AtomicBool::new(false),
        })
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
    }

    /// Validates, saves and applies new settings. Returns what was stored.
    pub fn save_settings(&self, settings: Settings) -> io::Result<Settings> {
        let settings = settings.sanitized(&self.default_library_folder);
        settings.save(self.store.dir())?;
        self.pages.configure(settings.cache_mb as usize * 1024 * 1024, settings.preload_ahead, settings.preload_behind);
        *self.settings.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = settings.clone();
        Ok(settings)
    }

    /// Where "Copy into KOMA library" puts files.
    pub fn library_folder(&self) -> PathBuf {
        PathBuf::from(self.settings().library_folder)
    }

    /// Drops everything cached for a volume that is leaving the library.
    pub fn forget_volume(&self, volume: &crate::library::Volume) {
        self.archives.close(&volume.id);
        self.pages.forget_volume(&volume.id);
        self.thumbs.remove(&cache_key(&volume.path, volume.size, volume.mtime));
        self.page_sizes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&volume.id);
    }

    pub fn library(&self) -> RwLockReadGuard<'_, Library> {
        self.library.read().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn library_mut(&self) -> RwLockWriteGuard<'_, Library> {
        self.library.write().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The open archive and thumbnail-cache key for a library volume.
    pub fn open_volume(&self, volume_id: &str) -> Result<(Arc<Archive>, String), String> {
        let (path, key) = {
            let library = self.library();
            let volume = library.volume(volume_id).ok_or("volume is not in the library")?;
            (volume.path.clone(), cache_key(&volume.path, volume.size, volume.mtime))
        };
        let archive = self
            .archives
            .get_or_open(volume_id, Path::new(&path))
            .map_err(|err| err.to_string())?;
        Ok((archive, key))
    }

    /// Page sizes for a volume, read from image headers only (no decoding),
    /// so the long-strip layout is known before any page is shown.
    pub fn page_sizes(&self, volume_id: &str) -> Result<PageSizes, String> {
        let cached = |this: &Self| {
            this.page_sizes
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get(volume_id)
                .cloned()
        };
        if let Some(sizes) = cached(self) {
            return Ok(sizes);
        }
        let (archive, _) = self.open_volume(volume_id)?;
        let sizes: Vec<(u32, u32)> = (0..archive.pages().len())
            .into_par_iter()
            .map(|index| {
                archive
                    .read_page_head(index, HEADER_BYTES)
                    .ok()
                    .and_then(|head| dimensions(&head))
                    // The header did not fit in the first bytes: read the whole page.
                    .or_else(|| archive.read_page(index).ok().and_then(|page| dimensions(&page)))
                    .unwrap_or((0, 0))
            })
            .collect();
        let sizes = Arc::new(sizes);
        self.page_sizes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(volume_id.to_string(), sizes.clone());
        Ok(sizes)
    }
}
