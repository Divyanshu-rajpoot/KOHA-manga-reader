//! In-memory page cache with preloading.
//!
//! Pages are kept ready to serve: either the original file bytes, or a JPEG
//! resized (Lanczos3) to the width the reader asked for. The cache is an LRU
//! bounded by total bytes. Around the page being read, the next pages are
//! prepared ahead of time on the rayon pool.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use fast_image_resize::images::Image;
use fast_image_resize::{PixelType, Resizer};
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ExtendedColorType, ImageReader};
use lru::LruCache;

use crate::archive::{mime_from_bytes, Archive};

#[cfg(test)]
pub const DEFAULT_CAPACITY_BYTES: usize = 1 << 30;
pub const DEFAULT_PRELOAD_AHEAD: usize = 6;
pub const DEFAULT_PRELOAD_BEHIND: usize = 2;
const RESIZED_QUALITY: u8 = 92;
/// JPEG cannot be taller or wider than 65,535 px.
const MAX_JPEG_SIDE: u32 = 65_000;

pub struct CachedPage {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
}

/// Volume id, page index, requested width (0 = original).
type Key = (String, usize, u32);
type Slot = Arc<OnceLock<Result<Arc<CachedPage>, String>>>;

struct Inner {
    pages: LruCache<Key, Arc<CachedPage>>,
    bytes: usize,
    /// Pages being rendered right now, so two threads never render the same one.
    pending: HashMap<Key, Slot>,
    /// The page the reader last asked for.
    focus: (String, usize),
}

pub struct PageCache {
    inner: Mutex<Inner>,
    capacity: AtomicUsize,
    /// Pages prepared ahead of and behind the one being read.
    preload_ahead: AtomicUsize,
    preload_behind: AtomicUsize,
}

impl PageCache {
    pub fn new(capacity_bytes: usize) -> Self {
        Self {
            inner: Mutex::new(Inner {
                pages: LruCache::unbounded(),
                bytes: 0,
                pending: HashMap::new(),
                focus: (String::new(), 0),
            }),
            capacity: AtomicUsize::new(capacity_bytes),
            preload_ahead: AtomicUsize::new(DEFAULT_PRELOAD_AHEAD),
            preload_behind: AtomicUsize::new(DEFAULT_PRELOAD_BEHIND),
        }
    }

    /// Applies new settings. Shrinking the cache evicts the oldest pages at once.
    pub fn configure(&self, capacity_bytes: usize, preload_ahead: usize, preload_behind: usize) {
        self.capacity.store(capacity_bytes, Ordering::Relaxed);
        self.preload_ahead.store(preload_ahead, Ordering::Relaxed);
        self.preload_behind.store(preload_behind, Ordering::Relaxed);
        self.evict(&mut self.lock());
    }

    fn evict(&self, inner: &mut Inner) {
        let capacity = self.capacity.load(Ordering::Relaxed);
        while inner.bytes > capacity && inner.pages.len() > 1 {
            if let Some((_, evicted)) = inner.pages.pop_lru() {
                inner.bytes -= evicted.bytes.len();
            }
        }
    }

    /// Bytes of page data held right now.
    pub fn total_bytes(&self) -> usize {
        self.lock().bytes
    }

    /// Drops every cached page of one volume.
    pub fn forget_volume(&self, volume_id: &str) {
        let mut inner = self.lock();
        let keys: Vec<Key> = inner.pages.iter().filter(|(key, _)| key.0 == volume_id).map(|(key, _)| key.clone()).collect();
        for key in keys {
            if let Some(page) = inner.pages.pop(&key) {
                inner.bytes -= page.bytes.len();
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Returns the page, rendering and caching it if needed.
    pub fn get(
        &self,
        archive: &Archive,
        volume_id: &str,
        index: usize,
        width: Option<u32>,
    ) -> Result<Arc<CachedPage>, String> {
        let key: Key = (volume_id.to_string(), index, width.unwrap_or(0));
        let slot = {
            let mut inner = self.lock();
            if let Some(page) = inner.pages.get(&key) {
                return Ok(page.clone());
            }
            inner.pending.entry(key.clone()).or_default().clone()
        };

        // Other threads asking for the same page wait here instead of rendering it again.
        let result = slot.get_or_init(|| render(archive, index, width).map(Arc::new)).clone();

        let mut inner = self.lock();
        inner.pending.remove(&key);
        if let Ok(page) = &result {
            if inner.pages.peek(&key).is_none() {
                inner.bytes += page.bytes.len();
                inner.pages.put(key, page.clone());
                self.evict(&mut inner);
            }
        }
        result
    }

    /// Records the page being read and prepares its neighbours in the background.
    pub fn preload_around(self: &Arc<Self>, archive: &Arc<Archive>, volume_id: &str, index: usize, width: Option<u32>) {
        self.lock().focus = (volume_id.to_string(), index);
        let pages = archive.pages().len();
        let ahead = self.preload_ahead.load(Ordering::Relaxed);
        let behind = self.preload_behind.load(Ordering::Relaxed);
        // A preload is dropped if the reader has moved further than this from it.
        let reach = ahead.max(behind) + 2;
        // Nearest pages first, alternating ahead and behind.
        let offsets = (1..=ahead.max(behind) as isize)
            .flat_map(|step| [(step as usize <= ahead).then_some(step), (step as usize <= behind).then_some(-step)])
            .flatten();
        for offset in offsets {
            let Some(target) = index.checked_add_signed(offset).filter(|&target| target < pages) else {
                continue;
            };
            let cache = self.clone();
            let archive = archive.clone();
            let volume_id = volume_id.to_string();
            rayon::spawn(move || {
                // Skip work for a page the reader has already scrolled far away from.
                let still_near = {
                    let inner = cache.lock();
                    inner.focus.0 == volume_id && inner.focus.1.abs_diff(target) <= reach
                };
                if still_near {
                    let _ = cache.get(&archive, &volume_id, target, width);
                }
            });
        }
    }

    /// Page indexes of this volume that are ready in memory, sorted.
    pub fn cached_pages(&self, volume_id: &str) -> Vec<usize> {
        let inner = self.lock();
        let mut pages: Vec<usize> = inner
            .pages
            .iter()
            .filter(|((volume, _, _), _)| volume == volume_id)
            .map(|((_, index, _), _)| *index)
            .collect();
        pages.sort_unstable();
        pages.dedup();
        pages
    }

}

/// Width and height from the image header, without decoding the pixels.
pub fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

fn render(archive: &Archive, index: usize, width: Option<u32>) -> Result<CachedPage, String> {
    let bytes = archive.read_page(index).map_err(|err| err.to_string())?;
    let mime = mime_from_bytes(&bytes);
    let original = |bytes| CachedPage { bytes, mime };

    let Some(width) = width else {
        return Ok(original(bytes));
    };
    // Animated GIFs must stay as they are.
    if mime == "image/gif" {
        return Ok(original(bytes));
    }
    let Some((source_w, source_h)) = dimensions(&bytes) else {
        return Ok(original(bytes));
    };
    // Only shrink, and only when it is worth it.
    if source_w == 0 || width == 0 || width as u64 * 10 >= source_w as u64 * 9 {
        return Ok(original(bytes));
    }
    let height = ((source_h as u64 * width as u64) / source_w as u64).max(1) as u32;
    if height > MAX_JPEG_SIDE {
        return Ok(original(bytes));
    }
    match resize_to_jpeg(&bytes, width, height) {
        Some(resized) => Ok(CachedPage { bytes: resized, mime: "image/jpeg" }),
        // A page the decoder cannot handle is still shown by the webview.
        None => Ok(original(bytes)),
    }
}

fn resize_to_jpeg(bytes: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let image = image::load_from_memory(bytes).ok()?;
    // Most manga pages are greyscale: keep them one channel.
    let (pixel_type, color, source_w, source_h, raw) = match image {
        DynamicImage::ImageLuma8(gray) => (PixelType::U8, ExtendedColorType::L8, gray.width(), gray.height(), gray.into_raw()),
        other => {
            let rgb = other.into_rgb8();
            (PixelType::U8x3, ExtendedColorType::Rgb8, rgb.width(), rgb.height(), rgb.into_raw())
        }
    };
    let source = Image::from_vec_u8(source_w, source_h, raw, pixel_type).ok()?;
    let mut target = Image::new(width, height, pixel_type);
    // The default algorithm is Lanczos3 convolution.
    Resizer::new().resize(&source, &mut target, None).ok()?;

    let mut out = Vec::with_capacity((width as usize * height as usize) / 4);
    JpegEncoder::new_with_quality(&mut out, RESIZED_QUALITY)
        .encode(target.buffer(), width, height, color)
        .ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;
    use std::time::{Duration, Instant};

    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    use super::*;
    use crate::testutil::TempDir;

    fn encode(image: DynamicImage, format: image::ImageFormat) -> Vec<u8> {
        let mut bytes = Vec::new();
        image.write_to(&mut Cursor::new(&mut bytes), format).unwrap();
        bytes
    }

    fn color_page(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbImage::from_fn(width, height, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 90]));
        encode(DynamicImage::ImageRgb8(image), image::ImageFormat::Jpeg)
    }

    fn gray_page(width: u32, height: u32) -> Vec<u8> {
        let image = image::GrayImage::from_fn(width, height, |x, y| image::Luma([((x + y) % 256) as u8]));
        encode(DynamicImage::ImageLuma8(image), image::ImageFormat::Png)
    }

    fn archive_of(dir: &TempDir, pages: &[Vec<u8>]) -> Arc<Archive> {
        let path = dir.path().join("book.cbz");
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (index, page) in pages.iter().enumerate() {
            let ext = if mime_from_bytes(page) == "image/png" { "png" } else { "jpg" };
            zip.start_file(format!("{index:03}.{ext}"), stored).unwrap();
            zip.write_all(page).unwrap();
        }
        zip.finish().unwrap();
        Arc::new(Archive::open(&path).unwrap())
    }

    #[test]
    fn returns_original_bytes_without_a_width() {
        let dir = TempDir::new("pages-original");
        let page = color_page(800, 1200);
        let archive = archive_of(&dir, &[page.clone()]);
        let cache = PageCache::new(DEFAULT_CAPACITY_BYTES);

        let cached = cache.get(&archive, "v", 0, None).unwrap();
        assert_eq!(cached.bytes, page);
        assert_eq!(cached.mime, "image/jpeg");
    }

    #[test]
    fn resizes_to_the_requested_width_keeping_the_shape() {
        let dir = TempDir::new("pages-resize");
        let archive = archive_of(&dir, &[color_page(1600, 2400), gray_page(1600, 2400)]);
        let cache = PageCache::new(DEFAULT_CAPACITY_BYTES);

        for index in 0..2 {
            let cached = cache.get(&archive, "v", index, Some(800)).unwrap();
            assert_eq!(cached.mime, "image/jpeg");
            assert_eq!(dimensions(&cached.bytes), Some((800, 1200)), "page {index}");
        }
        // The greyscale page stays one channel.
        let gray = image::load_from_memory(&cache.get(&archive, "v", 1, Some(800)).unwrap().bytes).unwrap();
        assert_eq!(gray.color(), image::ColorType::L8);
    }

    #[test]
    fn never_enlarges_and_skips_tiny_reductions() {
        let dir = TempDir::new("pages-noupscale");
        let page = color_page(800, 1200);
        let archive = archive_of(&dir, &[page.clone()]);
        let cache = PageCache::new(DEFAULT_CAPACITY_BYTES);

        for width in [800, 1600, 760] {
            assert_eq!(cache.get(&archive, "v", 0, Some(width)).unwrap().bytes, page, "w={width}");
        }
    }

    #[test]
    fn evicts_least_recently_used_pages_over_capacity() {
        let dir = TempDir::new("pages-evict");
        let pages: Vec<Vec<u8>> = (0..6).map(|_| color_page(400, 600)).collect();
        let page_len = pages[0].len();
        let archive = archive_of(&dir, &pages);
        // Room for three pages.
        let cache = PageCache::new(page_len * 3 + page_len / 2);

        for index in 0..6 {
            cache.get(&archive, "v", index, None).unwrap();
        }
        assert_eq!(cache.cached_pages("v"), [3, 4, 5]);
        assert!(cache.total_bytes() <= page_len * 3 + page_len / 2);

        // Touch page 3, add page 0: page 4 is now the oldest and goes.
        cache.get(&archive, "v", 3, None).unwrap();
        cache.get(&archive, "v", 0, None).unwrap();
        assert_eq!(cache.cached_pages("v"), [0, 3, 5]);
        assert_eq!(cache.cached_pages("other"), Vec::<usize>::new());
    }

    #[test]
    fn preloads_six_ahead_and_two_behind() {
        let dir = TempDir::new("pages-preload");
        let pages: Vec<Vec<u8>> = (0..20).map(|_| color_page(300, 450)).collect();
        let archive = archive_of(&dir, &pages);
        let cache = Arc::new(PageCache::new(DEFAULT_CAPACITY_BYTES));

        cache.get(&archive, "v", 10, None).unwrap();
        cache.preload_around(&archive, "v", 10, None);

        let expected: Vec<usize> = (8..=16).collect();
        let deadline = Instant::now() + Duration::from_secs(20);
        while cache.cached_pages("v") != expected && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(cache.cached_pages("v"), expected);
    }

    #[test]
    fn reports_missing_pages_as_errors_and_does_not_cache_them() {
        let dir = TempDir::new("pages-missing");
        let archive = archive_of(&dir, &[color_page(100, 100)]);
        let cache = PageCache::new(DEFAULT_CAPACITY_BYTES);
        assert!(cache.get(&archive, "v", 5, None).is_err());
        assert!(cache.cached_pages("v").is_empty());
    }
}
