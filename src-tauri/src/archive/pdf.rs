//! PDF volumes, read through PDFium (`pdfium.dll`, shipped next to the app).
//!
//! PDFium reads the file on demand, so a large merged PDF is never loaded
//! into memory. Each page is handled one of two ways:
//!
//! - A page that is a single JPEG scan (the usual manga or manhwa PDF) is
//!   handed out as the embedded JPEG file, untouched: original quality.
//! - Any other page (PNG-style images, several images, text, vectors) is
//!   drawn by PDFium at the resolution of its images and returned as PNG.
//!
//! PDFium is not thread-safe. `pdfium-render` serializes every call into it,
//! and each document sits behind its own mutex so one page operation is never
//! interleaved with another on the same document.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{DynamicImage, GrayImage, RgbImage};
use pdfium_render::prelude::*;

use super::{ArchiveError, Result};

/// Pixels per PDF point (1/72 inch) for pages without images: 200 DPI.
const DEFAULT_SCALE: f32 = 200.0 / 72.0;
const MIN_SCALE: f32 = 1.0;
const MAX_SCALE: f32 = 12.0;
/// Limits for a drawn page, so one odd page cannot exhaust memory.
const MAX_SIDE: f32 = 30_000.0;
const MAX_PIXELS: f32 = 40_000_000.0;
/// Pages with more objects than this are not inspected one by one.
const MAX_INSPECTED_OBJECTS: usize = 256;
/// Previews are drawn this much larger than asked, then scaled down smoothly.
const PREVIEW_OVERSAMPLE: u32 = 2;

static PDFIUM: OnceLock<std::result::Result<Pdfium, String>> = OnceLock::new();

/// Where `pdfium.dll` may be: next to the app, or in the source tree during development.
fn library_candidates() -> Vec<PathBuf> {
    let name = Pdfium::pdfium_platform_library_name();
    let mut candidates = Vec::new();
    if let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)) {
        candidates.push(dir.join(&name));
        candidates.push(dir.join("resources").join(&name));
    }
    #[cfg(debug_assertions)]
    candidates.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("pdfium").join(&name));
    candidates
}

/// The PDFium library, loaded the first time a PDF is opened.
fn pdfium() -> Result<&'static Pdfium> {
    PDFIUM
        .get_or_init(|| {
            let path = library_candidates()
                .into_iter()
                .find(|path| path.is_file())
                .ok_or("PDF support is not installed (pdfium.dll is missing)")?;
            Pdfium::bind_to_library(&path)
                .map(Pdfium::new)
                .map_err(|err| format!("could not load pdfium.dll: {err}"))
        })
        .as_ref()
        .map_err(|message| ArchiveError::Unsupported(message.clone()))
}

fn pdf_error(err: PdfiumError) -> ArchiveError {
    match err {
        PdfiumError::IoError(err) => ArchiveError::Io(err),
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            ArchiveError::Unsupported("the PDF is password-protected".into())
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::SecurityError) => {
            ArchiveError::Unsupported("the PDF uses an unsupported kind of protection".into())
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError) => {
            ArchiveError::Invalid("not a readable PDF".into())
        }
        other => ArchiveError::Invalid(format!("PDF error: {other}")),
    }
}

/// How one page is turned into an image, and how large that image is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Plan {
    /// The page is one JPEG scan: hand out the embedded file as it is.
    Jpeg { width: u32, height: u32 },
    /// Draw the page with PDFium at this size.
    Render { width: u32, height: u32 },
}

impl Plan {
    fn size(self) -> (u32, u32) {
        match self {
            Self::Jpeg { width, height } | Self::Render { width, height } => (width, height),
        }
    }
}

struct Inner {
    document: PdfDocument<'static>,
    /// Filled in the first time each page is looked at.
    plans: Vec<Option<Plan>>,
}

/// An opened PDF. Cheap to share behind an `Arc`; reads take `&self`.
pub struct PdfArchive {
    inner: Mutex<Inner>,
    pages: usize,
}

impl PdfArchive {
    /// Opens a PDF. PDFium reads the cross-reference table and page tree only.
    pub fn open(path: &Path) -> Result<Self> {
        let document = pdfium()?.load_pdf_from_file(path, None).map_err(pdf_error)?;
        let pages = document.pages().len().max(0) as usize;
        Ok(Self {
            inner: Mutex::new(Inner { document, plans: vec![None; pages] }),
            pages,
        })
    }

    pub fn page_count(&self) -> usize {
        self.pages
    }

    /// Short description for the files table: `PDF · scans` or `PDF · rendered`.
    pub fn describe(&self) -> String {
        let mut inner = self.lock();
        match inner.page(0).map(|(_, plan)| plan) {
            Ok(Plan::Jpeg { .. }) => "PDF · scans".into(),
            _ => "PDF · rendered".into(),
        }
    }

    /// The size in pixels of the image `read_page` returns.
    pub fn page_size(&self, index: usize) -> Option<(u32, u32)> {
        self.lock().page(index).ok().map(|(_, plan)| plan.size())
    }

    /// One page as a JPEG (the embedded scan) or a PNG (drawn by PDFium).
    pub fn read_page(&self, index: usize) -> Result<Vec<u8>> {
        let mut inner = self.lock();
        let (page, mut plan) = inner.page(index)?;
        if matches!(plan, Plan::Jpeg { .. }) {
            if let Some(jpeg) = embedded_jpeg(&page) {
                return Ok(jpeg);
            }
            // The stream was not a plain JPEG file after all: draw the page from now on.
            plan = render_plan(&page);
            inner.plans[index] = Some(plan);
        }
        let (width, height) = plan.size();
        let image = draw(&page, width, height)?;
        drop(page);
        drop(inner);

        // PNG is lossless; the fast setting keeps a page turn quick.
        let mut png = Vec::new();
        image
            .write_with_encoder(PngEncoder::new_with_quality(&mut png, CompressionType::Fast, FilterType::Adaptive))
            .map_err(|err| ArchiveError::Unsupported(format!("could not encode page {index}: {err}")))?;
        Ok(png)
    }

    /// Draws a page small, for a cover or thumbnail. `None` means the page is
    /// an embedded JPEG, which the caller decodes itself.
    pub fn render_preview(&self, index: usize, max_width: u32, max_height: u32) -> Result<Option<DynamicImage>> {
        let mut inner = self.lock();
        let (page, plan) = inner.page(index)?;
        let Plan::Render { width, height } = plan else {
            return Ok(None);
        };
        let fit = (max_width.saturating_mul(PREVIEW_OVERSAMPLE) as f64 / width as f64)
            .min(max_height.saturating_mul(PREVIEW_OVERSAMPLE) as f64 / height as f64)
            .min(1.0);
        let scaled = |side: u32| ((side as f64 * fit).round() as u32).max(1);
        draw(&page, scaled(width), scaled(height)).map(Some)
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Inner {
    /// Loads a page and decides (once) how it is turned into an image.
    fn page(&mut self, index: usize) -> Result<(PdfPage<'static>, Plan)> {
        let pages = self.plans.len();
        if index >= pages {
            return Err(ArchiveError::PageOutOfRange { index, pages });
        }
        let page = self.document.pages().get(index as PdfPageIndex).map_err(pdf_error)?;
        let plan = match self.plans[index] {
            Some(plan) => plan,
            None => {
                let plan = single_jpeg(&page).unwrap_or_else(|| render_plan(&page));
                self.plans[index] = Some(plan);
                plan
            }
        };
        Ok((page, plan))
    }
}

/// `Some` when the page is nothing but one upright JPEG image that fills it.
fn single_jpeg(page: &PdfPage) -> Option<Plan> {
    let objects = page.objects();
    if objects.len() != 1 || page.rotation().ok()? != PdfPageRenderRotation::None {
        return None;
    }
    let object = objects.get(0).ok()?;
    let image = object.as_image_object()?;

    // Only a stream that is a JPEG file as it stands, in a colour space every
    // image decoder shows correctly (grey or RGB; not CMYK), with no mask.
    let filters = image.filters();
    if filters.len() != 1 || filters.get(0).ok()?.name() != "DCTDecode" {
        return None;
    }
    if !matches!(image.bits_per_pixel().ok()?, 8 | 24) || object.has_transparency() {
        return None;
    }

    // Not mirrored, rotated or skewed.
    let matrix = image.matrix().ok()?;
    let skew = (matrix.b().abs() + matrix.c().abs()) / (matrix.a().abs() + matrix.d().abs()).max(f32::EPSILON);
    if matrix.a() <= 0.0 || matrix.d() <= 0.0 || skew > 0.001 {
        return None;
    }

    // Fills the page: margins are fine (they are simply left out), but an
    // image much larger than the page is being cropped by it.
    let bounds = object.bounds().ok()?;
    let (page_w, page_h) = (page.width().value, page.height().value);
    let (image_w, image_h) = (bounds.width().value, bounds.height().value);
    let fills = image_w >= page_w * 0.5 && image_h >= page_h * 0.5;
    let inside = image_w <= page_w * 1.02 && image_h <= page_h * 1.02;
    if !fills || !inside {
        return None;
    }

    let width = u32::try_from(image.width().ok()?).ok().filter(|&width| width > 0)?;
    let height = u32::try_from(image.height().ok()?).ok().filter(|&height| height > 0)?;
    Some(Plan::Jpeg { width, height })
}

/// The embedded JPEG file of a single-image page.
fn embedded_jpeg(page: &PdfPage) -> Option<Vec<u8>> {
    let object = page.objects().get(0).ok()?;
    let bytes = object.as_image_object()?.get_raw_image_data().ok()?;
    bytes.starts_with(&[0xFF, 0xD8, 0xFF]).then_some(bytes)
}

/// The size to draw a page at: the resolution of its sharpest image, so a
/// scan is not blurred or enlarged, or 200 DPI when it has no images.
fn render_plan(page: &PdfPage) -> Plan {
    let (page_w, page_h) = (page.width().value.max(1.0), page.height().value.max(1.0));
    let objects = page.objects();

    let mut image_scale: Option<f32> = None;
    let mut only_images = objects.len() > 0;
    if objects.len() <= MAX_INSPECTED_OBJECTS {
        for object in objects.iter() {
            let Some(image) = object.as_image_object() else {
                only_images = false;
                continue;
            };
            let (Ok(pixels), Ok(bounds)) = (image.width(), object.bounds()) else {
                continue;
            };
            let points = bounds.width().value;
            if pixels > 0 && points > 1.0 {
                let scale = pixels as f32 / points;
                image_scale = Some(image_scale.map_or(scale, |best| best.max(scale)));
            }
        }
    } else {
        only_images = false;
    }

    let scale = match image_scale {
        Some(scale) if only_images => scale,
        // Text and line art next to images still want to be sharp.
        Some(scale) => scale.max(DEFAULT_SCALE),
        None => DEFAULT_SCALE,
    };
    let scale = scale
        .clamp(MIN_SCALE, MAX_SCALE)
        .min(MAX_SIDE / page_w)
        .min(MAX_SIDE / page_h)
        .min((MAX_PIXELS / (page_w * page_h)).sqrt());
    Plan::Render {
        width: ((page_w * scale).round() as u32).max(1),
        height: ((page_h * scale).round() as u32).max(1),
    }
}

/// Draws a page on white. Greyscale pages come back with one channel.
fn draw(page: &PdfPage, width: u32, height: u32) -> Result<DynamicImage> {
    let config = PdfRenderConfig::new().set_fixed_size(width as Pixels, height as Pixels);
    let rgba = page.render_with_config(&config).map_err(pdf_error)?.as_rgba_bytes();
    if rgba.len() != width as usize * height as usize * 4 {
        return Err(ArchiveError::Unsupported("PDFium returned an unexpected bitmap".into()));
    }

    let pixels = rgba.chunks_exact(4);
    let image = if pixels.clone().all(|pixel| pixel[0] == pixel[1] && pixel[1] == pixel[2]) {
        GrayImage::from_raw(width, height, pixels.map(|pixel| pixel[0]).collect()).map(DynamicImage::ImageLuma8)
    } else {
        RgbImage::from_raw(width, height, pixels.flat_map(|pixel| [pixel[0], pixel[1], pixel[2]]).collect())
            .map(DynamicImage::ImageRgb8)
    };
    image.ok_or_else(|| ArchiveError::Unsupported("PDFium returned an unexpected bitmap".into()))
}

#[cfg(test)]
#[path = "pdf_tests.rs"]
mod tests;
