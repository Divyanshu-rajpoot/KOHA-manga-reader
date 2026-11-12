//! CBZ engine tests. CBZ files are generated in the temp folder and removed afterwards.
//!
//! The multi-GB benchmark is ignored by default:
//! `cargo test --release -- --ignored --nocapture`

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::CbzArchive as Archive;
use crate::archive::{mime_from_bytes, ArchiveError};

/// A temp CBZ that deletes itself.
struct TempCbz(PathBuf);

impl TempCbz {
    fn new(label: &str) -> Self {
        let name = format!("koma-test-{label}-{}.cbz", uuid::Uuid::new_v4());
        Self(std::env::temp_dir().join(name))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempCbz {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Fake page: a JPEG signature, the page number, then filler.
fn page_bytes(number: u32, len: usize) -> Vec<u8> {
    let mut bytes = vec![(number % 251) as u8; len.max(8)];
    bytes[..4].copy_from_slice(&[0xFF, 0xD8, 0xFF, 0xE0]);
    bytes[4..8].copy_from_slice(&number.to_le_bytes());
    bytes
}

fn stored() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(CompressionMethod::Stored)
}

fn deflated() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated)
}

fn writer(path: &Path) -> ZipWriter<BufWriter<File>> {
    ZipWriter::new(BufWriter::with_capacity(1 << 20, File::create(path).unwrap()))
}

#[test]
fn lists_pages_in_natural_order_and_skips_non_pages() {
    let cbz = TempCbz::new("order");
    let mut zip = writer(cbz.path());
    // Deliberately written out of order, with noise entries.
    for (name, number) in [("page10.jpg", 10), ("page2.jpg", 2), ("Page1.JPG", 1), ("page100.jpg", 100)] {
        zip.start_file(name, stored()).unwrap();
        zip.write_all(&page_bytes(number, 64)).unwrap();
    }
    zip.start_file("ComicInfo.xml", stored()).unwrap();
    zip.write_all(b"<ComicInfo/>").unwrap();
    zip.start_file("__MACOSX/._page1.jpg", stored()).unwrap();
    zip.write_all(b"resource fork").unwrap();
    zip.add_directory("extras/", stored()).unwrap();
    zip.finish().unwrap();

    let archive = Archive::open(cbz.path()).unwrap();
    let names: Vec<&str> = archive.pages().iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Page1.JPG", "page2.jpg", "page10.jpg", "page100.jpg"]);

    for (index, number) in [1u32, 2, 10, 100].into_iter().enumerate() {
        assert_eq!(archive.read_page(index).unwrap(), page_bytes(number, 64));
    }
}

#[test]
fn orders_pages_across_chapter_folders() {
    let cbz = TempCbz::new("folders");
    let mut zip = writer(cbz.path());
    for name in ["Ch 10/001.png", "Ch 2/002.png", "Ch 2/001.png", "Ch 1/010.png", "Ch 1/009.png"] {
        zip.start_file(name, stored()).unwrap();
        zip.write_all(&page_bytes(0, 16)).unwrap();
    }
    zip.finish().unwrap();

    let archive = Archive::open(cbz.path()).unwrap();
    let names: Vec<&str> = archive.pages().iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Ch 1/009.png", "Ch 1/010.png", "Ch 2/001.png", "Ch 2/002.png", "Ch 10/001.png"]);
}

#[test]
fn reads_deflated_pages() {
    let cbz = TempCbz::new("deflate");
    let mut zip = writer(cbz.path());
    for number in 1..=5u32 {
        let options = if number % 2 == 0 { stored() } else { deflated() };
        zip.start_file(format!("{number:03}.jpg"), options).unwrap();
        zip.write_all(&page_bytes(number, 50_000)).unwrap();
    }
    zip.finish().unwrap();

    let archive = Archive::open(cbz.path()).unwrap();
    assert_eq!(archive.pages().len(), 5);
    assert!(!archive.pages()[0].is_stored());
    assert!(archive.pages()[1].is_stored());
    for number in 1..=5u32 {
        let bytes = archive.read_page(number as usize - 1).unwrap();
        assert_eq!(bytes, page_bytes(number, 50_000), "page {number}");
        assert_eq!(mime_from_bytes(&bytes), "image/jpeg");
    }
}

#[test]
fn reads_zip64_archive_with_more_than_65535_entries() {
    const PAGES: u32 = 70_000;
    let cbz = TempCbz::new("zip64-count");
    let mut zip = writer(cbz.path());
    for number in 0..PAGES {
        zip.start_file(format!("{number:06}.jpg"), stored()).unwrap();
        zip.write_all(&page_bytes(number, 8)).unwrap();
    }
    zip.finish().unwrap();

    let archive = Archive::open(cbz.path()).unwrap();
    assert_eq!(archive.pages().len(), PAGES as usize);
    for number in [0, 1, 65_534, 65_535, 65_536, PAGES - 1] {
        assert_eq!(archive.read_page(number as usize).unwrap(), page_bytes(number, 8));
    }
}

#[test]
fn reads_archive_with_data_prepended() {
    let inner = TempCbz::new("inner");
    let mut zip = writer(inner.path());
    for number in 1..=3u32 {
        zip.start_file(format!("{number}.jpg"), stored()).unwrap();
        zip.write_all(&page_bytes(number, 32)).unwrap();
    }
    zip.finish().unwrap();

    let cbz = TempCbz::new("prefixed");
    let mut bytes = vec![0xAAu8; 1000];
    bytes.extend(fs::read(inner.path()).unwrap());
    fs::write(cbz.path(), bytes).unwrap();

    let archive = Archive::open(cbz.path()).unwrap();
    assert_eq!(archive.pages().len(), 3);
    assert_eq!(archive.read_page(2).unwrap(), page_bytes(3, 32));
}

#[test]
fn rejects_files_that_are_not_zip() {
    let cbz = TempCbz::new("garbage");
    fs::write(cbz.path(), vec![0x42u8; 4096]).unwrap();
    assert!(matches!(Archive::open(cbz.path()), Err(ArchiveError::Invalid(_))));

    let empty = TempCbz::new("empty");
    fs::write(empty.path(), b"").unwrap();
    assert!(matches!(Archive::open(empty.path()), Err(ArchiveError::Invalid(_))));
}

#[test]
fn rejects_truncated_archive() {
    let cbz = TempCbz::new("truncated");
    let mut zip = writer(cbz.path());
    for number in 1..=20u32 {
        zip.start_file(format!("{number}.jpg"), stored()).unwrap();
        zip.write_all(&page_bytes(number, 1000)).unwrap();
    }
    zip.finish().unwrap();
    let bytes = fs::read(cbz.path()).unwrap();
    fs::write(cbz.path(), &bytes[..bytes.len() - 200]).unwrap();

    assert!(Archive::open(cbz.path()).is_err());
}

#[test]
fn reports_page_out_of_range() {
    let cbz = TempCbz::new("range");
    let mut zip = writer(cbz.path());
    zip.start_file("1.jpg", stored()).unwrap();
    zip.write_all(&page_bytes(1, 16)).unwrap();
    zip.finish().unwrap();

    let archive = Archive::open(cbz.path()).unwrap();
    assert!(matches!(
        archive.read_page(1),
        Err(ArchiveError::PageOutOfRange { index: 1, pages: 1 })
    ));
}

#[test]
fn pages_can_be_read_from_many_threads() {
    let cbz = TempCbz::new("threads");
    let mut zip = writer(cbz.path());
    for number in 0..200u32 {
        zip.start_file(format!("{number:04}.jpg"), stored()).unwrap();
        zip.write_all(&page_bytes(number, 20_000)).unwrap();
    }
    zip.finish().unwrap();

    let archive = Archive::open(cbz.path()).unwrap();
    std::thread::scope(|scope| {
        for worker in 0..8u32 {
            let archive = &archive;
            scope.spawn(move || {
                for number in (0..200u32).filter(|n| n % 8 == worker) {
                    assert_eq!(archive.read_page(number as usize).unwrap(), page_bytes(number, 20_000));
                }
            });
        }
    });
}

/// ARCHITECTURE → Testing: 2,000 stored pages × 3 MB ≈ 6 GB, which forces ZIP64 offsets.
#[test]
#[ignore = "writes a 6 GB file; run with --ignored"]
fn benchmark_6gb_cbz() {
    const PAGES: u32 = 2_000;
    const PAGE_LEN: usize = 3 * 1024 * 1024;

    let cbz = TempCbz::new("6gb");
    let started = Instant::now();
    let mut zip = writer(cbz.path());
    let mut page = page_bytes(0, PAGE_LEN);
    for number in 0..PAGES {
        page[4..8].copy_from_slice(&number.to_le_bytes());
        zip.start_file(format!("{number:04}.jpg"), stored()).unwrap();
        zip.write_all(&page).unwrap();
    }
    zip.finish().unwrap();
    let file_len = fs::metadata(cbz.path()).unwrap().len();
    println!(
        "generated {:.2} GB in {:.1} s",
        file_len as f64 / (1u64 << 30) as f64,
        started.elapsed().as_secs_f64()
    );
    assert!(file_len > 4 * (1u64 << 30), "file must be past the 4 GB ZIP64 boundary");

    // Open: best of several runs, plus the first (coldest) run.
    let mut open_ms = Vec::new();
    for _ in 0..10 {
        let t = Instant::now();
        let archive = Archive::open(cbz.path()).unwrap();
        open_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(archive.pages().len(), PAGES as usize);
    }
    let first_open = open_ms[0];
    let best_open = open_ms.iter().copied().fold(f64::INFINITY, f64::min);
    println!("open: first {first_open:.2} ms, best {best_open:.2} ms");

    let archive = Archive::open(cbz.path()).unwrap();
    let t = Instant::now();
    let first = archive.read_page(0).unwrap();
    println!("first page read: {:.2} ms", t.elapsed().as_secs_f64() * 1000.0);
    assert_eq!(first.len(), PAGE_LEN);

    // Pages on both sides of the 4 GB boundary, and the last one.
    for number in [0u32, 1, 1_365, 1_366, 1_400, PAGES - 1] {
        let t = Instant::now();
        let bytes = archive.read_page(number as usize).unwrap();
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(bytes.len(), PAGE_LEN);
        assert_eq!(&bytes[..4], &[0xFF, 0xD8, 0xFF, 0xE0]);
        assert_eq!(&bytes[4..8], &number.to_le_bytes(), "page {number}");
        println!("page {number}: {ms:.2} ms");
    }

    assert!(first_open < 300.0, "open took {first_open:.2} ms; target is under 300 ms");
}
