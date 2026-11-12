//! Disk cache for covers and page thumbnails.
//!
//! Files live in `%LOCALAPPDATA%\Koma\thumbs\{key}\`, where the key is a hash
//! of the archive's path, size and modified time, so a changed file gets fresh
//! images. Everything here can be deleted safely; it is rebuilt on demand.
//!
//! Images are JPEG rather than the WebP named in ARCHITECTURE.md: the `image`
//! crate only writes lossless WebP, which is several times larger for scans.

pub mod pages;

use std::fs::{self, File};
use std::io::{self, BufWriter};
use std::path::{Path, PathBuf};

use image::codecs::jpeg::JpegEncoder;

use crate::archive::Archive;

const COVER_WIDTH: u32 = 480;
const COVER_QUALITY: u8 = 85;
const THUMB_WIDTH: u32 = 200;
const THUMB_MAX_HEIGHT: u32 = 400;
const THUMB_QUALITY: u8 = 75;

pub struct ThumbCache {
    root: PathBuf,
}

/// FNV-1a. Stable across runs and Rust versions, unlike the std hasher.
fn fnv1a(parts: &[&[u8]]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in parts.iter().flat_map(|part| part.iter()) {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Cache key for an archive file.
pub fn cache_key(path: &str, size: u64, mtime: i64) -> String {
    let path = path.replace('/', "\\").to_lowercase();
    format!("{:016x}", fnv1a(&[path.as_bytes(), &size.to_le_bytes(), &mtime.to_le_bytes()]))
}

impl ThumbCache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Deletes one archive's cover and thumbnails.
    pub fn remove(&self, key: &str) {
        let _ = fs::remove_dir_all(self.root.join(key));
    }

    /// Deletes every cover and thumbnail. They are rebuilt on demand.
    pub fn clear(&self) -> io::Result<()> {
        match fs::remove_dir_all(&self.root) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
            _ => Ok(()),
        }
    }

    /// Bytes used on disk.
    pub fn size_bytes(&self) -> u64 {
        fn walk(dir: &Path) -> u64 {
            fs::read_dir(dir).map_or(0, |entries| {
                entries
                    .flatten()
                    .map(|entry| match entry.metadata() {
                        Ok(meta) if meta.is_dir() => walk(&entry.path()),
                        Ok(meta) => meta.len(),
                        Err(_) => 0,
                    })
                    .sum()
            })
        }
        walk(&self.root)
    }

    pub fn cover_path(&self, key: &str) -> PathBuf {
        self.root.join(key).join("cover.jpg")
    }

    pub fn thumb_path(&self, key: &str, index: usize) -> PathBuf {
        self.root.join(key).join(format!("p{index:05}.jpg"))
    }

    /// Makes the cover (the archive's first page) unless it is already cached.
    pub fn ensure_cover(&self, archive: &Archive, key: &str) -> Result<PathBuf, String> {
        let path = self.cover_path(key);
        if !path.exists() {
            render(archive, 0, &path, COVER_WIDTH, u32::MAX, COVER_QUALITY)?;
        }
        Ok(path)
    }

    /// Makes one page thumbnail unless it is already cached.
    pub fn ensure_thumb(&self, archive: &Archive, key: &str, index: usize) -> Result<PathBuf, String> {
        let path = self.thumb_path(key, index);
        if !path.exists() {
            render(archive, index, &path, THUMB_WIDTH, THUMB_MAX_HEIGHT, THUMB_QUALITY)?;
        }
        Ok(path)
    }
}

fn render(
    archive: &Archive,
    index: usize,
    dest: &Path,
    max_width: u32,
    max_height: u32,
    quality: u8,
) -> Result<(), String> {
    let bytes = archive.read_page(index).map_err(|err| err.to_string())?;
    let image = image::load_from_memory(&bytes).map_err(|err| format!("could not decode page {index}: {err}"))?;
    drop(bytes);
    let image = if image.width() > max_width || image.height() > max_height {
        image.thumbnail(max_width, max_height)
    } else {
        image
    };
    write_jpeg(&image.to_rgb8(), dest, quality).map_err(|err| format!("could not save image: {err}"))
}

/// Written to a temp name, then renamed, so a half-written file is never served.
fn write_jpeg(image: &image::RgbImage, dest: &Path, quality: u8) -> io::Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = dest.with_extension(format!("tmp-{}", uuid::Uuid::new_v4().simple()));
    let result = (|| {
        let mut writer = BufWriter::new(File::create(&temp)?);
        JpegEncoder::new_with_quality(&mut writer, quality)
            .encode_image(image)
            .map_err(io::Error::other)?;
        writer.into_inner().map_err(|err| err.into_error())?.sync_all()?;
        fs::rename(&temp, dest)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_is_stable_and_depends_on_path_size_and_mtime() {
        let key = cache_key("D:\\Manga\\A v01.cbz", 100, 200);
        assert_eq!(key.len(), 16);
        assert_eq!(key, cache_key("d:/manga/a V01.CBZ", 100, 200));
        assert_ne!(key, cache_key("D:\\Manga\\A v02.cbz", 100, 200));
        assert_ne!(key, cache_key("D:\\Manga\\A v01.cbz", 101, 200));
        assert_ne!(key, cache_key("D:\\Manga\\A v01.cbz", 100, 201));
        // Pinned: a changed hash would orphan every user's thumbnail cache.
        assert_eq!(cache_key("a", 0, 0), "da0b881500262f8c");
    }
}
