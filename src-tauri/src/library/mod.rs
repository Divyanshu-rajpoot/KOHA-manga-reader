//! Library model: series, each holding volumes. Saved as JSON (see `store`).

pub mod comicinfo;
pub mod parser;
pub mod store;

use serde::{Deserialize, Serialize};

pub const LIBRARY_VERSION: u32 = 1;
/// Reading sessions kept per volume.
const MAX_SESSIONS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SeriesType {
    Manga,
    Manhwa,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Rtl,
    Ltr,
}

/// Whether a file is a volume or a single chapter (chapter files are common for manhwa).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    #[default]
    Volume,
    Chapter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Storage {
    /// The file stays where the user keeps it.
    Linked,
    /// The file was copied into the KOMA library folder.
    Copied,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub page: usize,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub date: String,
    pub from: usize,
    pub to: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub id: String,
    pub number: f64,
    /// Last volume of an omnibus that covers a range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number_end: Option<f64>,
    #[serde(default)]
    pub unit: Unit,
    pub path: String,
    pub storage: Storage,
    pub size: u64,
    /// File modified time, seconds since the Unix epoch.
    pub mtime: i64,
    pub pages: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<Progress>,
    #[serde(default)]
    pub sessions: Vec<Session>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Series {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_jp: Option<String>,
    #[serde(rename = "type")]
    pub kind: SeriesType,
    pub direction: Direction,
    pub volumes: Vec<Volume>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub version: u32,
    pub series: Vec<Series>,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            version: LIBRARY_VERSION,
            series: Vec::new(),
        }
    }
}

/// Titles match when their letters and digits match, ignoring case, spacing and punctuation.
pub fn title_key(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn slug(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

fn same_path(a: &str, b: &str) -> bool {
    // Windows paths are case-insensitive.
    a.replace('/', "\\").to_lowercase() == b.replace('/', "\\").to_lowercase()
}

impl Library {
    pub fn find_series(&self, title: &str) -> Option<&Series> {
        let key = title_key(title);
        self.series.iter().find(|series| title_key(&series.title) == key)
    }

    pub fn volume(&self, id: &str) -> Option<&Volume> {
        self.series
            .iter()
            .flat_map(|series| &series.volumes)
            .find(|volume| volume.id == id)
    }

    pub fn volume_by_path(&self, path: &str) -> Option<&Volume> {
        self.series
            .iter()
            .flat_map(|series| &series.volumes)
            .find(|volume| same_path(&volume.path, path))
    }

    #[cfg(test)]
    pub fn volume_count(&self) -> usize {
        self.series.iter().map(|series| series.volumes.len()).sum()
    }

    /// Records the page being read (1-based) and extends today's reading session.
    /// `now` is an ISO timestamp and `today` a local `YYYY-MM-DD` date, both from the UI.
    /// Returns false if the volume is not in the library.
    pub fn set_progress(&mut self, volume_id: &str, page: usize, now: String, today: String) -> bool {
        let Some(volume) = self
            .series
            .iter_mut()
            .flat_map(|series| &mut series.volumes)
            .find(|volume| volume.id == volume_id)
        else {
            return false;
        };
        let page = page.clamp(1, volume.pages.max(1));
        let previous = volume.progress.as_ref().map_or(page, |progress| progress.page);

        match volume.sessions.last_mut().filter(|session| session.date == today) {
            Some(session) => {
                session.from = session.from.min(page);
                session.to = session.to.max(page);
            }
            None => {
                volume.sessions.push(Session {
                    date: today,
                    from: previous.min(page),
                    to: page,
                });
                let extra = volume.sessions.len().saturating_sub(MAX_SESSIONS);
                volume.sessions.drain(..extra);
            }
        }
        volume.progress = Some(Progress { page, updated_at: now });
        true
    }

    /// Marks a volume finished (progress on its last page) or unread (no progress).
    pub fn set_read(&mut self, volume_id: &str, read: bool, now: String) -> bool {
        let Some(volume) = self
            .series
            .iter_mut()
            .flat_map(|series| &mut series.volumes)
            .find(|volume| volume.id == volume_id)
        else {
            return false;
        };
        volume.progress = read.then(|| Progress { page: volume.pages.max(1), updated_at: now });
        true
    }

    /// Removes a volume from the library (the file itself is not touched).
    /// A series left with no volumes is removed too.
    pub fn remove_volume(&mut self, volume_id: &str) -> Option<Volume> {
        let series_at = self
            .series
            .iter()
            .position(|series| series.volumes.iter().any(|volume| volume.id == volume_id))?;
        let series = &mut self.series[series_at];
        let volume_at = series.volumes.iter().position(|volume| volume.id == volume_id)?;
        let removed = series.volumes.remove(volume_at);
        if series.volumes.is_empty() {
            self.series.remove(series_at);
        }
        Some(removed)
    }

    /// Removes a series and returns its volumes.
    pub fn remove_series(&mut self, series_id: &str) -> Option<Vec<Volume>> {
        let at = self.series.iter().position(|series| series.id == series_id)?;
        Some(self.series.remove(at).volumes)
    }

    /// Edits a series. Renaming it to the title of another series merges the
    /// two. Returns the id of the series that now holds the volumes.
    pub fn update_series(
        &mut self,
        series_id: &str,
        title: &str,
        title_jp: Option<String>,
        kind: SeriesType,
        direction: Direction,
    ) -> Option<String> {
        let at = self.series.iter().position(|series| series.id == series_id)?;
        let title = title.trim();
        if title.is_empty() {
            return None;
        }
        let title_jp = title_jp.map(|text| text.trim().to_string()).filter(|text| !text.is_empty());

        let key = title_key(title);
        let twin = self
            .series
            .iter()
            .position(|series| series.id != series_id && title_key(&series.title) == key);
        if let Some(twin) = twin {
            let mut moved = self.series.remove(at).volumes;
            let target = &mut self.series[if twin > at { twin - 1 } else { twin }];
            target.volumes.append(&mut moved);
            sort_volumes(&mut target.volumes);
            return Some(target.id.clone());
        }

        let series = &mut self.series[at];
        series.title = title.to_string();
        series.title_jp = title_jp;
        series.kind = kind;
        series.direction = direction;
        Some(series.id.clone())
    }

    fn unique_series_id(&self, title: &str) -> String {
        let base = slug(title);
        if base.is_empty() {
            return uuid::Uuid::new_v4().to_string();
        }
        let mut id = base.clone();
        let mut n = 2;
        while self.series.iter().any(|series| series.id == id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        id
    }

    /// Adds a volume to the series with this title, creating the series if needed.
    /// Volumes stay sorted by unit, then number. Returns the series id.
    pub fn add_volume(
        &mut self,
        series_title: &str,
        kind: SeriesType,
        direction: Direction,
        volume: Volume,
    ) -> String {
        let key = title_key(series_title);
        let at = match self.series.iter().position(|series| title_key(&series.title) == key) {
            Some(at) => at,
            None => {
                let id = self.unique_series_id(series_title);
                self.series.push(Series {
                    id,
                    title: series_title.trim().to_string(),
                    title_jp: None,
                    kind,
                    direction,
                    volumes: Vec::new(),
                });
                self.series.len() - 1
            }
        };
        let series = &mut self.series[at];
        series.volumes.push(volume);
        sort_volumes(&mut series.volumes);
        series.id.clone()
    }
}

/// Volumes before chapters, each by number.
fn sort_volumes(volumes: &mut [Volume]) {
    volumes.sort_by(|a, b| {
        a.unit
            .cmp(&b.unit)
            .then(a.number.partial_cmp(&b.number).unwrap_or(std::cmp::Ordering::Equal))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume(number: f64, path: &str) -> Volume {
        Volume {
            id: uuid::Uuid::new_v4().to_string(),
            number,
            number_end: None,
            unit: Unit::Volume,
            path: path.to_string(),
            storage: Storage::Linked,
            size: 1,
            mtime: 0,
            pages: 10,
            progress: None,
            sessions: Vec::new(),
        }
    }

    #[test]
    fn groups_volumes_into_series_and_sorts_them() {
        let mut library = Library::default();
        library.add_volume("Paper Ronin", SeriesType::Manga, Direction::Rtl, volume(7.0, "a"));
        library.add_volume("paper  ronin!", SeriesType::Manga, Direction::Rtl, volume(2.0, "b"));
        library.add_volume("Tidebreaker", SeriesType::Manhwa, Direction::Ltr, volume(1.0, "c"));

        assert_eq!(library.series.len(), 2);
        assert_eq!(library.volume_count(), 3);
        let ronin = library.find_series("PAPER RONIN").unwrap();
        assert_eq!(ronin.id, "paper-ronin");
        assert_eq!(ronin.title, "Paper Ronin");
        let numbers: Vec<f64> = ronin.volumes.iter().map(|v| v.number).collect();
        assert_eq!(numbers, [2.0, 7.0]);
    }

    #[test]
    fn series_ids_are_unique_even_without_ascii() {
        let mut library = Library::default();
        let a = library.add_volume("ペーパー", SeriesType::Manga, Direction::Rtl, volume(1.0, "a"));
        let b = library.add_volume("ローニン", SeriesType::Manga, Direction::Rtl, volume(1.0, "b"));
        assert!(!a.is_empty() && !b.is_empty());
        assert_ne!(a, b);

        let c = library.add_volume("A-B", SeriesType::Manga, Direction::Rtl, volume(1.0, "c"));
        let d = library.add_volume("A/B?", SeriesType::Manga, Direction::Rtl, volume(1.0, "d"));
        // Different punctuation, same letters: one series.
        assert_eq!(c, d);
    }

    #[test]
    fn records_progress_and_reading_sessions() {
        let mut library = Library::default();
        let volume = volume(1.0, "a");
        let id = volume.id.clone();
        library.add_volume("X", SeriesType::Manga, Direction::Rtl, volume);
        let set = |library: &mut Library, page, day: &str| {
            library.set_progress(&id, page, format!("{day}T10:00:00Z"), day.to_string())
        };

        assert!(set(&mut library, 4, "2026-11-11"));
        assert!(set(&mut library, 9, "2026-11-11"));
        assert!(set(&mut library, 6, "2026-11-11")); // paging back keeps the furthest page
        assert!(set(&mut library, 10, "2026-11-12"));
        assert!(set(&mut library, 999, "2026-11-12")); // clamped to the page count

        let volume = library.volume(&id).unwrap();
        let progress = volume.progress.as_ref().unwrap();
        assert_eq!((progress.page, progress.updated_at.as_str()), (10, "2026-11-12T10:00:00Z"));
        let sessions: Vec<(&str, usize, usize)> =
            volume.sessions.iter().map(|s| (s.date.as_str(), s.from, s.to)).collect();
        assert_eq!(sessions, [("2026-11-11", 4, 9), ("2026-11-12", 6, 10)]);

        assert!(!library.set_progress("missing", 1, String::new(), String::new()));
    }

    #[test]
    fn edits_merges_and_removes_series() {
        let mut library = Library::default();
        let a1 = volume(1.0, "a1");
        let a1_id = a1.id.clone();
        let a = library.add_volume("Paper Ronin", SeriesType::Manga, Direction::Rtl, a1);
        let b = library.add_volume("Papr Ronin", SeriesType::Manga, Direction::Rtl, volume(3.0, "b3"));
        library.add_volume("Papr Ronin", SeriesType::Manga, Direction::Rtl, volume(2.0, "b2"));

        // Plain edit.
        let same = library.update_series(&a, "Paper Ronin", Some(" ペーパー ".into()), SeriesType::Manhwa, Direction::Ltr);
        assert_eq!(same.as_deref(), Some(a.as_str()));
        let series = library.find_series("Paper Ronin").unwrap();
        assert_eq!((series.title_jp.as_deref(), series.kind, series.direction), (Some("ペーパー"), SeriesType::Manhwa, Direction::Ltr));

        // Fixing the typo merges the misnamed series into the right one.
        let merged = library.update_series(&b, "paper ronin", None, SeriesType::Manga, Direction::Rtl);
        assert_eq!(merged.as_deref(), Some(a.as_str()));
        assert_eq!(library.series.len(), 1);
        let numbers: Vec<f64> = library.series[0].volumes.iter().map(|v| v.number).collect();
        assert_eq!(numbers, [1.0, 2.0, 3.0]);
        assert_eq!(library.series[0].kind, SeriesType::Manhwa, "the surviving series keeps its settings");

        assert_eq!(library.update_series(&a, "  ", None, SeriesType::Manga, Direction::Rtl), None);
        assert_eq!(library.update_series("missing", "X", None, SeriesType::Manga, Direction::Rtl), None);

        // Mark read / unread.
        assert!(library.set_read(&a1_id, true, "now".into()));
        assert_eq!(library.volume(&a1_id).unwrap().progress.as_ref().unwrap().page, 10);
        assert!(library.set_read(&a1_id, false, "now".into()));
        assert!(library.volume(&a1_id).unwrap().progress.is_none());

        // Remove.
        assert_eq!(library.remove_volume(&a1_id).unwrap().path, "a1");
        assert!(library.remove_volume(&a1_id).is_none());
        assert_eq!(library.remove_series(&a).unwrap().len(), 2);
        assert!(library.series.is_empty());
    }

    #[test]
    fn removing_the_last_volume_removes_the_series() {
        let mut library = Library::default();
        let only = volume(1.0, "a");
        let id = only.id.clone();
        library.add_volume("Solo", SeriesType::Manga, Direction::Rtl, only);
        library.remove_volume(&id);
        assert!(library.series.is_empty());
    }

    #[test]
    fn finds_volumes_by_path_ignoring_case_and_slashes() {
        let mut library = Library::default();
        library.add_volume("X", SeriesType::Manga, Direction::Rtl, volume(1.0, "D:\\Manga\\X v01.cbz"));
        assert!(library.volume_by_path("d:/manga/x V01.CBZ").is_some());
        assert!(library.volume_by_path("D:\\Manga\\X v02.cbz").is_none());
    }

    #[test]
    fn matches_the_documented_json_shape() {
        let json = r#"{
          "version": 1,
          "series": [{
            "id": "paper-ronin", "title": "Paper Ronin", "titleJp": "ペーパーローニン",
            "type": "manga", "direction": "rtl",
            "volumes": [{
              "id": "uuid", "number": 7, "path": "D:\\Manga\\Paper Ronin v07.cbz",
              "storage": "linked", "size": 1395864371, "mtime": 1731300000, "pages": 208,
              "progress": { "page": 142, "updatedAt": "2026-11-11T18:00:00Z" },
              "sessions": [{ "date": "2026-11-11", "from": 104, "to": 142 }]
            }]
          }]
        }"#;
        let library: Library = serde_json::from_str(json).unwrap();
        let volume = &library.series[0].volumes[0];
        assert_eq!(volume.number, 7.0);
        assert_eq!(volume.progress.as_ref().unwrap().page, 142);
        assert_eq!(library.series[0].title_jp.as_deref(), Some("ペーパーローニン"));

        let out = serde_json::to_value(&library).unwrap();
        assert_eq!(out["series"][0]["type"], "manga");
        assert_eq!(out["series"][0]["volumes"][0]["storage"], "linked");
    }
}
