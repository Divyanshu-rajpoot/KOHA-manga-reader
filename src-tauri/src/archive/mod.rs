//! Volume files: CBZ / ZIP / ZIP64 archives and PDF documents.
//!
//! A volume is opened by reading its index only; pages are read one at a
//! time, on demand. A whole file is never loaded into memory.

mod cbz;
mod pdf;
mod registry;
mod sniff;
mod zip_index;

use std::fmt;
use std::fs::File;
use std::io;
use std::path::Path;

use image::DynamicImage;

use cbz::CbzArchive;
use pdf::PdfArchive;
pub use registry::ArchiveRegistry;
pub use sniff::{dimensions, mime_from_bytes};

/// A PDF's `%PDF-` header must sit within this many bytes of the file start.
const PDF_HEADER_WINDOW: usize = 1024;

#[derive(Debug)]
pub enum ArchiveError {
    Io(io::Error),
    /// The file is not a readable ZIP archive or PDF.
    Invalid(String),
    /// The file is valid but uses a feature KOMA does not read.
    Unsupported(String),
    PageOutOfRange { index: usize, pages: usize },
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "could not read the file: {err}"),
            Self::Invalid(message) => write!(f, "invalid file: {message}"),
            Self::Unsupported(message) => write!(f, "unsupported file: {message}"),
            Self::PageOutOfRange { index, pages } => {
                write!(f, "page {index} does not exist (the file has {pages} pages)")
            }
        }
    }
}

impl std::error::Error for ArchiveError {}

impl From<io::Error> for ArchiveError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

pub type Result<T> = std::result::Result<T, ArchiveError>;

/// Reads exactly `buf.len()` bytes at `offset` without a shared file cursor,
/// so any number of threads can read the same file at once.
pub(crate) fn read_exact_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut done = 0usize;
        while done < buf.len() {
            match file.seek_read(&mut buf[done..], offset + done as u64) {
                Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(n) => done += n,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                Err(err) => return Err(err),
            }
        }
        Ok(())
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        file.read_exact_at(buf, offset)
    }
}

/// True when the file starts like a PDF. The file extension is not trusted.
fn is_pdf(path: &Path) -> io::Result<bool> {
    let file = File::open(path)?;
    let len = (file.metadata()?.len().min(PDF_HEADER_WINDOW as u64)) as usize;
    let mut head = vec![0u8; len];
    read_exact_at(&file, &mut head, 0)?;
    Ok(head.windows(5).any(|window| window == b"%PDF-"))
}

/// An opened volume file. Cheap to share behind an `Arc`; reads take `&self`.
pub enum Archive {
    Cbz(CbzArchive),
    Pdf(PdfArchive),
}

impl Archive {
    /// Opens a volume by reading its index only.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if is_pdf(path)? {
            PdfArchive::open(path).map(Self::Pdf)
        } else {
            CbzArchive::open(path).map(Self::Cbz)
        }
    }

    /// Number of pages, in reading order.
    pub fn page_count(&self) -> usize {
        match self {
            Self::Cbz(cbz) => cbz.pages().len(),
            Self::Pdf(pdf) => pdf.page_count(),
        }
    }

    /// Short description for the files table, e.g. `ZIP64 · stored` or `PDF · scans`.
    pub fn describe(&self) -> String {
        match self {
            Self::Cbz(cbz) => cbz.describe(),
            Self::Pdf(pdf) => pdf.describe(),
        }
    }

    /// The bytes of a root-level `ComicInfo.xml`. PDFs never have one.
    pub fn read_comic_info(&self) -> Option<Vec<u8>> {
        match self {
            Self::Cbz(cbz) => cbz.read_comic_info(),
            Self::Pdf(_) => None,
        }
    }

    /// One page as an image file the webview can show.
    ///
    /// For a CBZ, and for a PDF page that is a single JPEG scan, these are the
    /// original bytes. Any other PDF page is drawn by PDFium and returned as PNG.
    pub fn read_page(&self, index: usize) -> Result<Vec<u8>> {
        match self {
            Self::Cbz(cbz) => cbz.read_page(index),
            Self::Pdf(pdf) => pdf.read_page(index),
        }
    }

    /// Width and height of a page in pixels, without decoding or drawing it.
    pub fn page_size(&self, index: usize) -> Option<(u32, u32)> {
        match self {
            Self::Cbz(cbz) => cbz.page_size(index),
            Self::Pdf(pdf) => pdf.page_size(index),
        }
    }

    /// A page decoded for a cover or thumbnail, no larger than it needs to be.
    /// The caller still scales it down to the exact size.
    pub fn page_preview(&self, index: usize, max_width: u32, max_height: u32) -> Result<DynamicImage> {
        if let Self::Pdf(pdf) = self {
            // Drawing a PDF page small is far cheaper than drawing it full size.
            if let Some(image) = pdf.render_preview(index, max_width, max_height)? {
                return Ok(image);
            }
        }
        let bytes = self.read_page(index)?;
        image::load_from_memory(&bytes)
            .map_err(|err| ArchiveError::Unsupported(format!("could not decode page {index}: {err}")))
    }
}
