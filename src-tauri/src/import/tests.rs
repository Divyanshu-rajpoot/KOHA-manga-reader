use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::*;
use crate::testutil::{test_jpeg, write_pdf, PdfTestPage, TempDir};

/// A real, decodable JPEG page.
fn jpeg(width: u32, height: u32, shade: u8) -> Vec<u8> {
    let image = image::RgbImage::from_pixel(width, height, image::Rgb([shade, 40, 40]));
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Jpeg)
        .unwrap();
    bytes
}

fn write_cbz(path: &Path, pages: usize, comic_info: Option<&str>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for page in 0..pages {
        zip.start_file(format!("{:03}.jpg", page + 1), stored).unwrap();
        zip.write_all(&jpeg(900, 1300, (page * 20) as u8)).unwrap();
    }
    if let Some(xml) = comic_info {
        zip.start_file("ComicInfo.xml", stored).unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

struct Fixture {
    dir: TempDir,
    state: AppState,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let dir = TempDir::new(label);
        let state = AppState::new(
            &dir.path().join("data"),
            &dir.path().join("cache"),
            dir.path().join("KOMA Library"),
        )
        .unwrap();
        Self { dir, state }
    }

    fn source(&self, name: &str) -> PathBuf {
        self.dir.path().join("downloads").join(name)
    }

    fn scan(&self, paths: &[&Path]) -> Vec<Candidate> {
        let paths: Vec<String> = paths.iter().map(|path| path.display().to_string()).collect();
        scan(&paths, &self.state.library(), Direction::Rtl)
    }
}

fn item(candidate: &Candidate) -> ImportItem {
    ImportItem {
        path: candidate.path.clone(),
        series: candidate.series.clone(),
        number: candidate.number.unwrap(),
        number_end: candidate.number_end,
        unit: candidate.unit,
        series_type: candidate.series_type,
        direction: candidate.direction,
    }
}

#[test]
fn scans_a_folder_recursively_and_detects_series() {
    let fx = Fixture::new("scan");
    write_cbz(&fx.source("Paper Ronin/Paper Ronin v10 (Digital).cbz"), 2, None);
    write_cbz(&fx.source("Paper Ronin/Paper Ronin v02.cbz"), 2, None);
    write_cbz(&fx.source("Tidebreaker c012.zip"), 1, None);
    write_cbz(&fx.source("chapter_bundle_final.cbz"), 1, None);
    fs::write(fx.source("notes.txt"), "not an archive").unwrap();
    fs::write(fx.source("broken.cbz"), "not a zip").unwrap();

    let found = fx.scan(&[&fx.dir.path().join("downloads")]);
    let by_name = |name: &str| found.iter().find(|c| c.file_name == name).unwrap();
    assert_eq!(found.len(), 5);

    let v2 = by_name("Paper Ronin v02.cbz");
    assert!(v2.matched && !v2.duplicate && v2.error.is_none());
    assert_eq!((v2.series.as_str(), v2.number, v2.pages), ("Paper Ronin", Some(2.0), 2));
    assert_eq!((v2.series_type, v2.direction), (SeriesType::Manga, Direction::Rtl));
    // Natural order: v02 before v10.
    let position = |name: &str| found.iter().position(|c| c.file_name == name).unwrap();
    assert!(position("Paper Ronin v02.cbz") < position("Paper Ronin v10 (Digital).cbz"));

    let chapter = by_name("Tidebreaker c012.zip");
    assert_eq!((chapter.unit, chapter.series_type, chapter.direction), (Unit::Chapter, SeriesType::Manhwa, Direction::Ltr));

    let unmatched = by_name("chapter_bundle_final.cbz");
    assert!(!unmatched.matched && unmatched.error.is_none());

    assert!(by_name("broken.cbz").error.is_some());
}

#[test]
fn comic_info_overrides_the_file_name() {
    let fx = Fixture::new("comicinfo");
    let path = fx.source("random-download-name.cbz");
    let xml = "<ComicInfo><Series>Ember Ward</Series><Number>4</Number><Manga>YesAndRightToLeft</Manga></ComicInfo>";
    write_cbz(&path, 1, Some(xml));

    let found = fx.scan(&[&path]);
    let candidate = &found[0];
    assert!(candidate.matched && candidate.from_comic_info);
    assert_eq!((candidate.series.as_str(), candidate.number), ("Ember Ward", Some(4.0)));
    assert_eq!(candidate.pages, 1, "ComicInfo.xml is not a page");
}

#[test]
fn imports_a_linked_volume_with_cover_and_thumbnails() {
    let fx = Fixture::new("linked");
    let path = fx.source("Paper Ronin v07.cbz");
    write_cbz(&path, 3, None);
    let candidate = fx.scan(&[&path]).remove(0);

    let steps = Mutex::new(Vec::new());
    let volume_id = import_one(&fx.state, &item(&candidate), Storage::Linked, &|p| {
        steps.lock().unwrap().push(p.step);
    })
    .unwrap();

    let steps = steps.into_inner().unwrap();
    assert_eq!(steps.first(), Some(&"index"));
    assert_eq!(steps.last(), Some(&"done"));
    assert!(steps.contains(&"cover") && steps.contains(&"saved") && steps.contains(&"thumbs"));
    assert!(!steps.contains(&"copy"));

    // In memory and on disk.
    let saved = fx.state.store.load().unwrap();
    let volume = &saved.series[0].volumes[0];
    assert_eq!(saved.series[0].title, "Paper Ronin");
    assert_eq!((volume.id.as_str(), volume.number, volume.pages), (volume_id.as_str(), 7.0, 3));
    assert_eq!(volume.storage, Storage::Linked);
    assert_eq!(Path::new(&volume.path), path);
    assert!(volume.size > 0 && volume.mtime > 0);

    // Cover is resized to 480 px wide; one thumbnail per page.
    let (_, key) = fx.state.open_volume(&volume_id).unwrap();
    let cover = image::open(fx.state.thumbs.cover_path(&key)).unwrap();
    assert_eq!(cover.width(), 480);
    for index in 0..3 {
        let thumb = image::open(fx.state.thumbs.thumb_path(&key, index)).unwrap();
        assert!(thumb.width() <= 200 && thumb.height() <= 400);
    }

    // Scanning again flags it; importing again is refused.
    let again = fx.scan(&[&path]).remove(0);
    assert!(again.duplicate && again.existing_series);
    assert!(import_one(&fx.state, &item(&again), Storage::Linked, &|_| {}).is_err());
    assert_eq!(fx.state.library().volume_count(), 1);
}

#[test]
fn reads_page_sizes_from_headers() {
    let fx = Fixture::new("sizes");
    let path = fx.source("Paper Ronin v01.cbz");
    write_cbz(&path, 4, None);
    let candidate = fx.scan(&[&path]).remove(0);
    let volume_id = import_one(&fx.state, &item(&candidate), Storage::Linked, &|_| {}).unwrap();

    let sizes = fx.state.page_sizes(&volume_id).unwrap();
    assert_eq!(*sizes, vec![(900, 1300); 4]);
    assert!(fx.state.page_sizes("missing").is_err());
}

#[test]
fn copies_into_the_library_folder() {
    let fx = Fixture::new("copied");
    let path = fx.source("Quiet: Blade? Omnibus 01-05.cbz".replace([':', '?'], "").as_str());
    write_cbz(&path, 1, None);
    let mut import = item(&fx.scan(&[&path]).remove(0));
    import.series = "Quiet: Blade?".into(); // characters Windows does not allow in names

    let copied = Mutex::new(false);
    let volume_id = import_one(&fx.state, &import, Storage::Copied, &|p| {
        if p.step == "copy" && p.done == p.total {
            *copied.lock().unwrap() = true;
        }
    })
    .unwrap();
    assert!(copied.into_inner().unwrap());

    let library = fx.state.library();
    let volume = library.volume(&volume_id).unwrap();
    let expected = fx.dir.path().join("KOMA Library").join("Quiet_ Blade_").join("Vol 01-05.cbz");
    assert_eq!(Path::new(&volume.path), expected);
    assert_eq!(volume.storage, Storage::Copied);
    assert_eq!(fs::read(&expected).unwrap(), fs::read(&path).unwrap());
    assert!(path.exists(), "the original is left in place");
    assert!(!expected.with_extension("cbz.part").exists());
}

#[test]
fn a_failed_file_does_not_stop_the_rest() {
    let fx = Fixture::new("batch");
    let good = fx.source("Paper Ronin v01.cbz");
    write_cbz(&good, 1, None);
    let good_item = item(&fx.scan(&[&good]).remove(0));
    let missing = ImportItem {
        path: fx.source("Paper Ronin v02.cbz").display().to_string(),
        number: 2.0,
        ..good_item.clone()
    };

    let errors = Mutex::new(Vec::new());
    import_all(&fx.state, &[missing, good_item], Storage::Linked, &|p| {
        if p.step == "error" {
            errors.lock().unwrap().push(p.path.clone());
        }
    });

    assert_eq!(errors.into_inner().unwrap().len(), 1);
    assert_eq!(fx.state.library().volume_count(), 1);
}

#[test]
fn names_library_files() {
    let base = ImportItem {
        path: String::new(),
        series: "Paper Ronin".into(),
        number: 7.0,
        number_end: None,
        unit: Unit::Volume,
        series_type: SeriesType::Manga,
        direction: Direction::Rtl,
    };
    let root = Path::new("L");
    assert_eq!(library_destination(root, &base), root.join("Paper Ronin").join("Vol 07.cbz"));
    let chapter = ImportItem { unit: Unit::Chapter, number: 12.5, ..base.clone() };
    assert_eq!(library_destination(root, &chapter), root.join("Paper Ronin").join("Ch 12.5.cbz"));
    let blank = ImportItem { series: " .. ".into(), ..base.clone() };
    assert_eq!(library_destination(root, &blank), root.join("Untitled").join("Vol 07.cbz"));
    // A ZIP is filed as CBZ; a PDF stays a PDF.
    let zip = ImportItem { path: "D:/in/Paper Ronin v07.zip".into(), ..base.clone() };
    assert_eq!(library_destination(root, &zip), root.join("Paper Ronin").join("Vol 07.cbz"));
    let pdf = ImportItem { path: "D:/in/Paper Ronin v07.PDF".into(), ..base };
    assert_eq!(library_destination(root, &pdf), root.join("Paper Ronin").join("Vol 07.pdf"));
}

#[test]
fn imports_a_pdf_chapter_into_the_library_folder() {
    let fx = Fixture::new("pdf");
    let path = fx.source("Tidebreaker/Tidebreaker c012.pdf");
    write_pdf(
        &path,
        &[
            PdfTestPage::Jpeg { bytes: test_jpeg(720, 2400, 30), width: 720, height: 2400 },
            PdfTestPage::Vector,
        ],
    );

    // A folder scan finds PDFs next to archives.
    let candidate = fx.scan(&[&fx.source("Tidebreaker")]).remove(0);
    assert_eq!(candidate.error, None);
    assert_eq!((candidate.series.as_str(), candidate.number, candidate.pages), ("Tidebreaker", Some(12.0), 2));
    assert_eq!((candidate.unit, candidate.series_type), (Unit::Chapter, SeriesType::Manhwa));
    assert!(candidate.matched);

    let volume_id = import_one(&fx.state, &item(&candidate), Storage::Copied, &|_| {}).unwrap();
    let expected = fx.dir.path().join("KOMA Library").join("Tidebreaker").join("Ch 12.pdf");
    {
        let library = fx.state.library();
        let volume = library.volume(&volume_id).unwrap();
        assert_eq!(Path::new(&volume.path), expected);
        assert_eq!(volume.pages, 2);
    }
    assert_eq!(fs::read(&expected).unwrap(), fs::read(&path).unwrap());

    // Cover and thumbnails for both kinds of page; sizes for the long strip.
    let (_, key) = fx.state.open_volume(&volume_id).unwrap();
    assert_eq!(image::open(fx.state.thumbs.cover_path(&key)).unwrap().width(), 480);
    for index in 0..2 {
        let thumb = image::open(fx.state.thumbs.thumb_path(&key, index)).unwrap();
        assert!(thumb.width() <= 200 && thumb.height() <= 400);
    }
    assert_eq!(*fx.state.page_sizes(&volume_id).unwrap(), vec![(720, 2400), (833, 1111)]);
}
