//! Import: scan files or folders into candidates, then add them to the library.
//!
//! Scanning only reads each archive's index (and ComicInfo.xml), so it is fast
//! at any file size. Adding a volume optionally copies the file, makes its
//! cover, saves the library, then makes page thumbnails in parallel.

#[cfg(test)]
mod tests;

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, UNIX_EPOCH};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::archive::Archive;
use crate::cache::cache_key;
use crate::library::comicinfo;
use crate::library::parser::parse_file_name;
use crate::library::{Direction, Library, SeriesType, Storage, Unit, Volume};
use crate::state::AppState;

const ARCHIVE_EXTENSIONS: [&str; 2] = ["cbz", "zip"];
const COPY_CHUNK: usize = 4 * 1024 * 1024;

/// A file found by a scan, with KOMA's best guess at what it is.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub path: String,
    pub file_name: String,
    pub size: u64,
    pub pages: usize,
    pub index_ms: f64,
    pub series: String,
    pub number: Option<f64>,
    pub number_end: Option<f64>,
    pub unit: Unit,
    pub series_type: SeriesType,
    pub direction: Direction,
    /// Series and number were both detected.
    pub matched: bool,
    /// Detected from ComicInfo.xml rather than the file name.
    pub from_comic_info: bool,
    /// The series already exists in the library.
    pub existing_series: bool,
    /// This exact file is already in the library.
    pub duplicate: bool,
    pub error: Option<String>,
}

/// What the user confirmed for one file.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportItem {
    pub path: String,
    pub series: String,
    pub number: f64,
    pub number_end: Option<f64>,
    pub unit: Unit,
    pub series_type: SeriesType,
    pub direction: Direction,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    /// The source path: identifies the queue row.
    pub path: String,
    /// `copy`, `index`, `cover`, `thumbs`, `done` or `error`.
    pub step: &'static str,
    pub done: u64,
    pub total: u64,
    pub volume_id: Option<String>,
    pub message: Option<String>,
}

fn is_archive(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ARCHIVE_EXTENSIONS.iter().any(|known| ext.eq_ignore_ascii_case(known)))
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            collect(&entry.path(), out);
        }
    } else if is_archive(path) {
        out.push(path.to_path_buf());
    }
}

/// Expands files and folders (recursively) into archive files, in natural order.
pub fn expand_paths(paths: &[String]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for path in paths {
        collect(Path::new(path), &mut out);
    }
    out.sort_by(|a, b| natord::compare_ignore_case(&a.to_string_lossy(), &b.to_string_lossy()));
    out.dedup();
    out
}

fn mtime_secs(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_secs() as i64)
}

fn scan_one(path: &Path, library: &Library, default_direction: Direction) -> Candidate {
    let path_text = path.display().to_string();
    let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("");
    let parsed = parse_file_name(stem);

    let mut candidate = Candidate {
        file_name: path.file_name().and_then(|name| name.to_str()).unwrap_or("").to_string(),
        size: fs::metadata(path).map_or(0, |metadata| metadata.len()),
        pages: 0,
        index_ms: 0.0,
        series: parsed.series.clone(),
        number: parsed.number,
        number_end: parsed.number_end,
        unit: parsed.unit,
        series_type: SeriesType::Manga,
        direction: default_direction,
        matched: false,
        from_comic_info: false,
        existing_series: false,
        duplicate: library.volume_by_path(&path_text).is_some(),
        error: None,
        path: path_text,
    };

    let started = Instant::now();
    let archive = match Archive::open(path) {
        Ok(archive) => archive,
        Err(err) => {
            candidate.error = Some(err.to_string());
            return candidate;
        }
    };
    candidate.index_ms = started.elapsed().as_secs_f64() * 1000.0;
    candidate.pages = archive.pages().len();
    if candidate.pages == 0 {
        candidate.error = Some("no page images in this archive".into());
        return candidate;
    }

    // ComicInfo.xml overrides the file name.
    let info = archive.read_comic_info().map(|bytes| comicinfo::parse(&bytes)).unwrap_or_default();
    if let Some(series) = info.series {
        candidate.series = series;
        candidate.from_comic_info = true;
        if let Some(number) = info.number {
            candidate.number = Some(number);
            candidate.number_end = None;
        }
    }

    // Chapter files are usually manhwa (left-to-right, long strip).
    if candidate.unit == Unit::Chapter {
        candidate.series_type = SeriesType::Manhwa;
        candidate.direction = Direction::Ltr;
    }
    match info.right_to_left {
        Some(true) => {
            candidate.series_type = SeriesType::Manga;
            candidate.direction = Direction::Rtl;
        }
        Some(false) => candidate.direction = Direction::Ltr,
        None => {}
    }
    // An existing series keeps its own settings.
    if let Some(series) = library.find_series(&candidate.series) {
        candidate.series = series.title.clone();
        candidate.series_type = series.kind;
        candidate.direction = series.direction;
        candidate.existing_series = true;
    }

    candidate.matched = !candidate.series.is_empty() && candidate.number.is_some();
    candidate
}

/// Scans files and folders. Reads each archive's index only.
pub fn scan(paths: &[String], library: &Library, default_direction: Direction) -> Vec<Candidate> {
    expand_paths(paths)
        .par_iter()
        .map(|path| scan_one(path, library, default_direction))
        .collect()
}

fn format_number(number: f64) -> String {
    if number.fract() == 0.0 {
        format!("{number:02}")
    } else {
        format!("{number}")
    }
}

fn sanitize_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim_matches(|c: char| c == '.' || c.is_whitespace());
    if cleaned.is_empty() {
        "Untitled".to_string()
    } else {
        cleaned.to_string()
    }
}

/// `{library}\{Series}\Vol NN.cbz` (or `Ch NNN.cbz`, `Vol 01-05.cbz`).
pub fn library_destination(library_folder: &Path, item: &ImportItem) -> PathBuf {
    let label = match item.unit {
        Unit::Volume => "Vol",
        Unit::Chapter => "Ch",
    };
    let mut name = format!("{label} {}", format_number(item.number));
    if let Some(end) = item.number_end {
        name.push('-');
        name.push_str(&format_number(end));
    }
    library_folder
        .join(sanitize_file_name(&item.series))
        .join(format!("{name}.cbz"))
}

fn copy_with_progress(source: &Path, dest: &Path, report: &mut dyn FnMut(u64, u64)) -> io::Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let total = fs::metadata(source)?.len();
    let partial = dest.with_extension("cbz.part");
    let result = (|| {
        let mut reader = File::open(source)?;
        let mut writer = File::create(&partial)?;
        let mut buffer = vec![0u8; COPY_CHUNK];
        let mut done = 0u64;
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            writer.write_all(&buffer[..read])?;
            done += read as u64;
            report(done, total);
        }
        writer.sync_all()?;
        drop(writer);
        fs::rename(&partial, dest)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result
}

/// Adds one file to the library. Returns the new volume id.
/// `progress` may be called from several threads.
pub fn import_one(
    state: &AppState,
    item: &ImportItem,
    storage: Storage,
    progress: &(dyn Fn(ImportProgress) + Sync),
) -> Result<String, String> {
    let emit = |step: &'static str, done: u64, total: u64, volume_id: Option<&str>| {
        progress(ImportProgress {
            path: item.path.clone(),
            step,
            done,
            total,
            volume_id: volume_id.map(str::to_string),
            message: None,
        });
    };

    if item.series.trim().is_empty() {
        return Err("choose a series first".into());
    }
    let source = PathBuf::from(&item.path);
    if state.library().volume_by_path(&item.path).is_some() {
        return Err("this file is already in the library".into());
    }

    // 1. Copy into the library folder, if asked.
    let path = match storage {
        Storage::Linked => source,
        Storage::Copied => {
            let dest = library_destination(&state.library_folder(), item);
            if dest.exists() {
                return Err(format!("{} already exists", dest.display()));
            }
            let mut last = Instant::now();
            copy_with_progress(&source, &dest, &mut |done, total| {
                if done == total || last.elapsed().as_millis() >= 100 {
                    last = Instant::now();
                    emit("copy", done, total, None);
                }
            })
            .map_err(|err| format!("could not copy the file: {err}"))?;
            dest
        }
    };
    let path_text = path.display().to_string();

    // 2. Read the index.
    let metadata = fs::metadata(&path).map_err(|err| format!("could not read the file: {err}"))?;
    let volume_id = uuid::Uuid::new_v4().to_string();
    let archive = state
        .archives
        .get_or_open(&volume_id, &path)
        .map_err(|err| err.to_string())?;
    let pages = archive.pages().len();
    if pages == 0 {
        return Err("no page images in this archive".into());
    }
    emit("index", 1, 1, None);

    // 3. Cover. A page that cannot be decoded should not block the import.
    let size = metadata.len();
    let mtime = mtime_secs(&metadata);
    let key = cache_key(&path_text, size, mtime);
    let _ = state.thumbs.ensure_cover(&archive, &key);
    emit("cover", 1, 1, None);

    // 4. Add to the library and save. The volume is usable from here on.
    {
        let mut library = state.library_mut();
        library.add_volume(
            item.series.trim(),
            item.series_type,
            item.direction,
            Volume {
                id: volume_id.clone(),
                number: item.number,
                number_end: item.number_end,
                unit: item.unit,
                path: path_text,
                storage,
                size,
                mtime,
                pages,
                progress: None,
                sessions: Vec::new(),
            },
        );
        state
            .store
            .save(&library)
            .map_err(|err| format!("could not save the library: {err}"))?;
    }
    emit("saved", 1, 1, Some(&volume_id));

    // 5. Page thumbnails, in parallel.
    let finished = AtomicU64::new(0);
    (0..pages).into_par_iter().for_each(|index| {
        let _ = state.thumbs.ensure_thumb(&archive, &key, index);
        let done = finished.fetch_add(1, Ordering::Relaxed) + 1;
        if done % 8 == 0 || done == pages as u64 {
            emit("thumbs", done, pages as u64, Some(&volume_id));
        }
    });

    emit("done", 1, 1, Some(&volume_id));
    Ok(volume_id)
}

/// Imports files one after another. A failed file is reported and skipped.
pub fn import_all(
    state: &AppState,
    items: &[ImportItem],
    storage: Storage,
    progress: &(dyn Fn(ImportProgress) + Sync),
) {
    for item in items {
        if let Err(message) = import_one(state, item, storage, progress) {
            progress(ImportProgress {
                path: item.path.clone(),
                step: "error",
                done: 0,
                total: 0,
                volume_id: None,
                message: Some(message),
            });
        }
    }
}
