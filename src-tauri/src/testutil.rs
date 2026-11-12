//! Shared helpers for tests.

use std::fs;
use std::path::{Path, PathBuf};

/// A temp folder that deletes itself.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("koma-test-{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// One page of a generated test PDF.
pub enum PdfTestPage {
    /// A JPEG scan filling the page, the way manga PDFs are usually made.
    Jpeg { bytes: Vec<u8>, width: u32, height: u32 },
    /// An uncompressed greyscale image filling a page half its size in points.
    RawGray { width: u32, height: u32 },
    /// A 300 × 400 pt page with a red rectangle and no images.
    Vector,
}

/// Writes a small, valid PDF with a classic cross-reference table.
pub fn write_pdf(path: &Path, pages: &[PdfTestPage]) {
    fn stream(dict: &str, data: &[u8]) -> Vec<u8> {
        let mut object = format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
        object.extend_from_slice(data);
        object.extend_from_slice(b"\nendstream");
        object
    }

    // Object 1 is the catalog, 2 the page tree; each page then takes three numbers.
    let mut objects: Vec<Vec<u8>> = vec![Vec::new(), Vec::new()];
    let mut kids = String::new();
    for page in pages {
        let page_number = objects.len() + 1;
        kids.push_str(&format!("{page_number} 0 R "));
        let (box_w, box_h, content, image) = match page {
            PdfTestPage::Jpeg { bytes, width, height } => {
                let dict = format!(
                    "/Type /XObject /Subtype /Image /Width {width} /Height {height} \
                     /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode"
                );
                (*width, *height, format!("q {width} 0 0 {height} 0 0 cm /Im0 Do Q"), Some(stream(&dict, bytes)))
            }
            PdfTestPage::RawGray { width, height } => {
                let pixels: Vec<u8> = (0..width * height).map(|i| (i % 251) as u8).collect();
                let dict = format!(
                    "/Type /XObject /Subtype /Image /Width {width} /Height {height} \
                     /ColorSpace /DeviceGray /BitsPerComponent 8"
                );
                let (w, h) = (width / 2, height / 2);
                (w, h, format!("q {w} 0 0 {h} 0 0 cm /Im0 Do Q"), Some(stream(&dict, &pixels)))
            }
            PdfTestPage::Vector => (300, 400, "1 0 0 rg 50 50 200 300 re f".to_string(), None),
        };
        let resources = if image.is_some() {
            format!("<< /XObject << /Im0 {} 0 R >> >>", page_number + 2)
        } else {
            "<< >>".to_string()
        };
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {box_w} {box_h}] \
                 /Resources {resources} /Contents {} 0 R >>",
                page_number + 1
            )
            .into_bytes(),
        );
        objects.push(stream("", content.as_bytes()));
        objects.push(image.unwrap_or_else(|| b"null".to_vec()));
    }
    objects[0] = b"<< /Type /Catalog /Pages 2 0 R >>".to_vec();
    objects[1] = format!("<< /Type /Pages /Kids [{kids}] /Count {} >>", pages.len()).into_bytes();

    let mut pdf = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        pdf.extend_from_slice(object);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes(),
    );
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, pdf).unwrap();
}

/// A real, decodable colour JPEG.
pub fn test_jpeg(width: u32, height: u32, shade: u8) -> Vec<u8> {
    let image = image::RgbImage::from_fn(width, height, |x, y| image::Rgb([shade, (x % 256) as u8, (y % 256) as u8]));
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Jpeg)
        .unwrap();
    bytes
}
