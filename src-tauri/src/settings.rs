//! User settings, saved as `settings.json` next to the library.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::library::Direction;

const FILE_NAME: &str = "settings.json";
const CACHE_MB_RANGE: std::ops::RangeInclusive<u32> = 128..=16_384;
const MAX_PRELOAD_AHEAD: usize = 16;
const MAX_PRELOAD_BEHIND: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// `system`, `light` or `dark`.
    pub theme: String,
    /// `red` or `orange`.
    pub accent: String,
    pub reduce_motion: bool,
    /// Memory for ready-to-show pages, in MB.
    pub cache_mb: u32,
    /// Pages prepared ahead of and behind the one being read.
    pub preload_ahead: usize,
    pub preload_behind: usize,
    /// Reading direction given to newly imported volume series.
    pub default_direction: Direction,
    /// Where "Copy into KOMA library" puts files.
    pub library_folder: String,
    /// Folders checked for new archives.
    pub watch_folders: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            accent: "red".into(),
            reduce_motion: false,
            cache_mb: 1024,
            preload_ahead: 6,
            preload_behind: 2,
            default_direction: Direction::Rtl,
            library_folder: String::new(),
            watch_folders: Vec::new(),
        }
    }
}

impl Settings {
    /// Replaces out-of-range or unknown values with safe ones.
    pub fn sanitized(mut self, default_library_folder: &Path) -> Self {
        if !["system", "light", "dark"].contains(&self.theme.as_str()) {
            self.theme = "system".into();
        }
        if !["red", "orange"].contains(&self.accent.as_str()) {
            self.accent = "red".into();
        }
        self.cache_mb = self.cache_mb.clamp(*CACHE_MB_RANGE.start(), *CACHE_MB_RANGE.end());
        self.preload_ahead = self.preload_ahead.min(MAX_PRELOAD_AHEAD);
        self.preload_behind = self.preload_behind.min(MAX_PRELOAD_BEHIND);
        if self.library_folder.trim().is_empty() {
            self.library_folder = default_library_folder.display().to_string();
        }
        self.watch_folders.retain(|folder| !folder.trim().is_empty());
        self.watch_folders.sort();
        self.watch_folders.dedup();
        self
    }

    /// Loads settings. A missing or unreadable file gives the defaults.
    pub fn load(dir: &Path, default_library_folder: &Path) -> Self {
        fs::read_to_string(dir.join(FILE_NAME))
            .ok()
            .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
            .unwrap_or_default()
            .sanitized(default_library_folder)
    }

    /// Atomic save: temp file, then rename.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let temp = dir.join(format!("{FILE_NAME}.tmp"));
        {
            let mut file = File::create(&temp)?;
            file.write_all(&serde_json::to_vec_pretty(self).map_err(io::Error::other)?)?;
            file.sync_all()?;
        }
        fs::rename(temp, dir.join(FILE_NAME))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn defaults_when_missing_and_round_trips() {
        let dir = TempDir::new("settings");
        let fallback = Path::new("C:\\Docs\\KOMA Library");

        let mut settings = Settings::load(dir.path(), fallback);
        assert_eq!(settings.theme, "system");
        assert_eq!(settings.cache_mb, 1024);
        assert_eq!((settings.preload_ahead, settings.preload_behind), (6, 2));
        assert_eq!(settings.library_folder, "C:\\Docs\\KOMA Library");

        settings.theme = "dark".into();
        settings.watch_folders = vec!["D:\\Manga".into()];
        settings.save(dir.path()).unwrap();
        assert_eq!(Settings::load(dir.path(), fallback), settings);
    }

    #[test]
    fn sanitizes_bad_values_and_tolerates_partial_files() {
        let dir = TempDir::new("settings-bad");
        let fallback = Path::new("L");
        fs::write(
            dir.path().join(FILE_NAME),
            r#"{ "theme": "neon", "accent": "blue", "cacheMb": 5, "preloadAhead": 999,
                 "watchFolders": ["B", "", "A", "B"], "somethingNew": true }"#,
        )
        .unwrap();

        let settings = Settings::load(dir.path(), fallback);
        assert_eq!((settings.theme.as_str(), settings.accent.as_str()), ("system", "red"));
        assert_eq!(settings.cache_mb, 128);
        assert_eq!(settings.preload_ahead, 16);
        assert_eq!(settings.preload_behind, 2, "missing fields keep their defaults");
        assert_eq!(settings.watch_folders, ["A", "B"]);

        fs::write(dir.path().join(FILE_NAME), "garbage").unwrap();
        assert_eq!(Settings::load(dir.path(), fallback).theme, "system");
    }
}
