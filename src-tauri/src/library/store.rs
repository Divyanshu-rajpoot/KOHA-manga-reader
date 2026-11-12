//! Saves the library as `library.json`. Writes are atomic: a temp file is
//! written and flushed, then renamed over the old file.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::Library;

pub struct LibraryStore {
    dir: PathBuf,
}

impl LibraryStore {
    /// `dir` is `%APPDATA%\Koma` in the app.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn file(&self) -> PathBuf {
        self.dir.join("library.json")
    }

    /// Loads the library. A missing file is an empty library. An unreadable
    /// file is kept as `library.corrupt-{n}.json` so nothing is lost.
    pub fn load(&self) -> io::Result<Library> {
        let path = self.file();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Library::default()),
            Err(err) => return Err(err),
        };
        match serde_json::from_str(&text) {
            Ok(library) => Ok(library),
            Err(_) => {
                let backup = (1..)
                    .map(|n| self.dir.join(format!("library.corrupt-{n}.json")))
                    .find(|candidate| !candidate.exists())
                    .unwrap_or_else(|| self.dir.join("library.corrupt.json"));
                fs::rename(&path, backup)?;
                Ok(Library::default())
            }
        }
    }

    pub fn save(&self, library: &Library) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let json = serde_json::to_vec_pretty(library).map_err(io::Error::other)?;
        let temp = self.dir.join("library.json.tmp");
        {
            let mut file = File::create(&temp)?;
            file.write_all(&json)?;
            file.sync_all()?;
        }
        fs::rename(&temp, self.file())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{Direction, SeriesType, Storage, Unit, Volume};
    use crate::testutil::TempDir;

    fn sample() -> Library {
        let mut library = Library::default();
        library.add_volume(
            "Paper Ronin",
            SeriesType::Manga,
            Direction::Rtl,
            Volume {
                id: "v1".into(),
                number: 7.0,
                number_end: None,
                unit: Unit::Volume,
                path: "D:\\Manga\\Paper Ronin v07.cbz".into(),
                storage: Storage::Linked,
                size: 123,
                mtime: 456,
                pages: 208,
                progress: None,
                sessions: Vec::new(),
            },
        );
        library
    }

    #[test]
    fn missing_file_is_an_empty_library() {
        let dir = TempDir::new("store-empty");
        let library = LibraryStore::new(dir.path()).load().unwrap();
        assert!(library.series.is_empty());
    }

    #[test]
    fn saves_and_loads_without_leaving_a_temp_file() {
        let dir = TempDir::new("store-roundtrip");
        let store = LibraryStore::new(dir.path().join("nested").join("Koma"));
        store.save(&sample()).unwrap();
        store.save(&sample()).unwrap(); // overwrite works

        let loaded = store.load().unwrap();
        assert_eq!(loaded.series[0].title, "Paper Ronin");
        assert_eq!(loaded.series[0].volumes[0].pages, 208);
        assert!(!store.dir().join("library.json.tmp").exists());
    }

    #[test]
    fn corrupt_file_is_backed_up_not_overwritten() {
        let dir = TempDir::new("store-corrupt");
        let store = LibraryStore::new(dir.path());
        fs::write(dir.path().join("library.json"), "{ not json").unwrap();

        assert!(store.load().unwrap().series.is_empty());
        let backup = fs::read_to_string(dir.path().join("library.corrupt-1.json")).unwrap();
        assert_eq!(backup, "{ not json");
    }
}
