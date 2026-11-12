//! PDF engine tests. PDFs are generated in the temp folder and removed afterwards.

use std::fs;
use std::sync::Arc;

use crate::archive::{dimensions, mime_from_bytes, Archive, ArchiveError};
use crate::testutil::{test_jpeg, write_pdf, PdfTestPage, TempDir};

fn jpeg_page(width: u32, height: u32, shade: u8) -> (PdfTestPage, Vec<u8>) {
    let bytes = test_jpeg(width, height, shade);
    (PdfTestPage::Jpeg { bytes: bytes.clone(), width, height }, bytes)
}

#[test]
fn jpeg_scans_come_out_byte_for_byte() {
    let dir = TempDir::new("pdf-scans");
    let path = dir.path().join("Tidebreaker c012.pdf");
    let (first, first_bytes) = jpeg_page(800, 1200, 10);
    let (second, second_bytes) = jpeg_page(720, 4000, 200);
    write_pdf(&path, &[first, second]);

    let archive = Archive::open(&path).unwrap();
    assert_eq!(archive.page_count(), 2);
    assert_eq!(archive.describe(), "PDF · scans");
    assert_eq!(archive.page_size(0), Some((800, 1200)));
    assert_eq!(archive.page_size(1), Some((720, 4000)));
    assert_eq!(archive.read_page(0).unwrap(), first_bytes);
    assert_eq!(archive.read_page(1).unwrap(), second_bytes);
    assert!(archive.read_comic_info().is_none());
}

#[test]
fn other_pages_are_drawn_as_png() {
    let dir = TempDir::new("pdf-drawn");
    let path = dir.path().join("mixed.pdf");
    write_pdf(&path, &[PdfTestPage::Vector, PdfTestPage::RawGray { width: 400, height: 600 }]);

    let archive = Archive::open(&path).unwrap();
    assert_eq!(archive.describe(), "PDF · rendered");

    // No images: 300 × 400 pt at 200 DPI, in colour.
    let vector = archive.read_page(0).unwrap();
    assert_eq!(mime_from_bytes(&vector), "image/png");
    assert_eq!(dimensions(&vector), Some((833, 1111)));
    assert_eq!(archive.page_size(0), Some((833, 1111)));
    let vector = image::load_from_memory(&vector).unwrap().into_rgb8();
    assert_eq!(vector.get_pixel(10, 10).0, [255, 255, 255], "paper is white");
    assert_eq!(vector.get_pixel(416, 555).0, [255, 0, 0], "the rectangle is red");

    // An image-only page is drawn at the image's own resolution and stays greyscale.
    let scan = archive.read_page(1).unwrap();
    assert_eq!(dimensions(&scan), Some((400, 600)));
    let scan = image::load_from_memory(&scan).unwrap();
    assert_eq!(scan.color(), image::ColorType::L8);
    // Lossless: the pixels are the ones in the PDF (i % 251).
    let scan = scan.into_luma8();
    assert_eq!(scan.get_pixel(7, 0).0, [7]);
    assert_eq!(scan.get_pixel(3, 2).0, [((2 * 400 + 3) % 251) as u8]);
}

#[test]
fn previews_are_drawn_small() {
    let dir = TempDir::new("pdf-preview");
    let path = dir.path().join("preview.pdf");
    let (scan, _) = jpeg_page(600, 900, 50);
    write_pdf(&path, &[PdfTestPage::Vector, scan]);

    let archive = Archive::open(&path).unwrap();
    let drawn = archive.page_preview(0, 200, 400).unwrap();
    assert_eq!((drawn.width(), drawn.height()), (400, 533));
    // A JPEG scan is decoded as it is.
    let decoded = archive.page_preview(1, 200, 400).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (600, 900));
}

#[test]
fn the_content_decides_the_format_not_the_extension() {
    let dir = TempDir::new("pdf-sniff");
    let path = dir.path().join("really-a-pdf.cbz");
    let (scan, bytes) = jpeg_page(300, 450, 90);
    write_pdf(&path, &[scan]);

    let archive = Archive::open(&path).unwrap();
    assert!(matches!(archive, Archive::Pdf(_)));
    assert_eq!(archive.read_page(0).unwrap(), bytes);
}

#[test]
fn rejects_broken_pdfs_and_missing_pages() {
    let dir = TempDir::new("pdf-broken");
    let broken = dir.path().join("broken.pdf");
    fs::write(&broken, b"%PDF-1.7\nthis is not really a pdf\n").unwrap();
    assert!(matches!(Archive::open(&broken), Err(ArchiveError::Invalid(_))));

    let path = dir.path().join("one.pdf");
    write_pdf(&path, &[PdfTestPage::Vector]);
    let archive = Archive::open(&path).unwrap();
    assert!(matches!(
        archive.read_page(1),
        Err(ArchiveError::PageOutOfRange { index: 1, pages: 1 })
    ));
    assert_eq!(archive.page_size(1), None);
}

#[test]
fn pages_can_be_read_from_many_threads() {
    let dir = TempDir::new("pdf-threads");
    let path = dir.path().join("threads.pdf");
    let (pages, bytes): (Vec<_>, Vec<_>) = (0..24u8).map(|n| jpeg_page(200, 300, n * 10)).unzip();
    write_pdf(&path, &pages);

    let archive = Arc::new(Archive::open(&path).unwrap());
    let bytes = Arc::new(bytes);
    let handles: Vec<_> = (0..8)
        .map(|thread| {
            let (archive, bytes) = (archive.clone(), bytes.clone());
            std::thread::spawn(move || {
                for round in 0..24 {
                    let index = (thread * 5 + round) % 24;
                    assert_eq!(archive.read_page(index).unwrap(), bytes[index]);
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
}
