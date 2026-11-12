//! CBZ / ZIP / ZIP64 archive engine.
//!
//! An archive is opened by reading its index only; page bytes are read one
//! entry at a time, on demand. A whole archive is never loaded into memory.

mod registry;
mod sniff;
#[cfg(test)]
mod tests;
mod zip_index;

use std::fmt;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub use registry::ArchiveRegistry;
pub use sniff::mime_from_bytes;

const SIG_LOCAL_HEADER: u32 = 0x0403_4b50;
const LOCAL_HEADER_LEN: usize = 30;
const METHOD_STORED: u16 = 0;
const METHOD_DEFLATED: u16 = 8;
const FLAG_ENCRYPTED: u16 = 1;
/// Largest single page we are willing to hold in memory.
const MAX_PAGE_BYTES: u64 = 1 << 30;
const MAX_COMIC_INFO_BYTES: u64 = 1 << 20;

#[derive(Debug)]
pub enum ArchiveError {
    Io(io::Error),
    /// The file is not a readable ZIP archive.
    Invalid(String),
    /// The archive is valid but uses a feature KOMA does not read.
    Unsupported(String),
    PageOutOfRange { index: usize, pages: usize },
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "could not read the file: {err}"),
            Self::Invalid(message) => write!(f, "invalid archive: {message}"),
            Self::Unsupported(message) => write!(f, "unsupported archive: {message}"),
            Self::PageOutOfRange { index, pages } => {
                write!(f, "page {index} does not exist (the archive has {pages} pages)")
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

/// One page image inside an archive.
#[derive(Debug, Clone)]
pub struct PageEntry {
    /// Entry path inside the archive, with `/` separators.
    pub name: String,
    /// Size of the image file in bytes.
    pub size: u64,
    compressed_size: u64,
    method: u16,
    encrypted: bool,
    crc32: u32,
    header_offset: u64,
    cd_index: usize,
}

#[cfg(test)]
impl PageEntry {
    pub fn is_stored(&self) -> bool {
        self.method == METHOD_STORED
    }
}

type Inflater = zip::ZipArchive<BufReader<File>>;

/// An opened CBZ. Cheap to share behind an `Arc`; reads take `&self`.
pub struct Archive {
    path: PathBuf,
    file: File,
    file_size: u64,
    pages: Vec<PageEntry>,
    comic_info: Option<PageEntry>,
    zip64: bool,
    /// Only created if a deflated page is read. Most CBZ files store images uncompressed.
    inflater: Mutex<Option<Inflater>>,
}

impl Archive {
    /// Opens an archive by reading its central directory only.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = File::open(&path)?;
        let file_size = file.metadata()?.len();

        let mut comic_info = None;
        let mut pages = Vec::new();
        let (entries, zip64) = zip_index::read_central_directory(&file, file_size)?;
        for entry in entries {
            let is_page = sniff::is_page_name(&entry.name);
            let is_comic_info = entry.name.eq_ignore_ascii_case("ComicInfo.xml");
            if !is_page && !is_comic_info {
                continue;
            }
            let entry = PageEntry {
                name: entry.name.replace('\\', "/"),
                size: entry.uncompressed_size,
                compressed_size: entry.compressed_size,
                method: entry.method,
                encrypted: entry.flags & FLAG_ENCRYPTED != 0,
                crc32: entry.crc32,
                header_offset: entry.header_offset,
                cd_index: entry.cd_index,
            };
            if is_page {
                pages.push(entry);
            } else {
                comic_info = Some(entry);
            }
        }
        // Natural order: "page2" before "page10".
        pages.sort_by(|a, b| natord::compare_ignore_case(&a.name, &b.name));

        Ok(Self {
            path,
            file,
            file_size,
            pages,
            comic_info,
            zip64,
            inflater: Mutex::new(None),
        })
    }

    /// Short description for the files table, e.g. `ZIP64 · stored`.
    pub fn describe(&self) -> String {
        let format = if self.zip64 { "ZIP64" } else { "ZIP" };
        let stored = self.pages.iter().filter(|page| page.method == METHOD_STORED).count();
        let packing = if stored == self.pages.len() {
            "stored"
        } else if stored == 0 {
            "compressed"
        } else {
            "mixed"
        };
        format!("{format} · {packing}")
    }

    /// The bytes of a root-level `ComicInfo.xml`, if the archive has a readable one.
    pub fn read_comic_info(&self) -> Option<Vec<u8>> {
        let entry = self.comic_info.as_ref()?;
        if entry.size > MAX_COMIC_INFO_BYTES {
            return None;
        }
        self.read_entry(entry).ok()
    }

    /// Page images in reading order.
    pub fn pages(&self) -> &[PageEntry] {
        &self.pages
    }

    /// Reads the original bytes of one page.
    pub fn read_page(&self, index: usize) -> Result<Vec<u8>> {
        let page = self.pages.get(index).ok_or(ArchiveError::PageOutOfRange {
            index,
            pages: self.pages.len(),
        })?;
        self.read_entry(page)
    }

    fn read_entry(&self, page: &PageEntry) -> Result<Vec<u8>> {
        if page.encrypted {
            return Err(ArchiveError::Unsupported("the archive is password-protected".into()));
        }
        if page.size > MAX_PAGE_BYTES || page.compressed_size > MAX_PAGE_BYTES {
            return Err(ArchiveError::Unsupported(format!("{} is larger than 1 GB", page.name)));
        }
        match page.method {
            METHOD_STORED => self.read_stored(page),
            METHOD_DEFLATED => self.read_deflated(page),
            other => Err(ArchiveError::Unsupported(format!("compression method {other}"))),
        }
    }

    /// The first bytes of a page: enough to read the image header (its size)
    /// without reading the whole image. Compressed pages are read in full.
    pub fn read_page_head(&self, index: usize, max_len: usize) -> Result<Vec<u8>> {
        let page = self.pages.get(index).ok_or(ArchiveError::PageOutOfRange {
            index,
            pages: self.pages.len(),
        })?;
        if page.method != METHOD_STORED || page.encrypted {
            return self.read_entry(page);
        }
        let data_offset = self.stored_data_offset(page)?;
        let mut data = vec![0u8; (page.compressed_size as usize).min(max_len)];
        read_exact_at(&self.file, &mut data, data_offset)?;
        Ok(data)
    }

    /// A stored page is one small header read plus one data read.
    fn read_stored(&self, page: &PageEntry) -> Result<Vec<u8>> {
        let data_offset = self.stored_data_offset(page)?;
        let mut data = vec![0u8; page.compressed_size as usize];
        read_exact_at(&self.file, &mut data, data_offset)?;
        Ok(data)
    }

    fn stored_data_offset(&self, page: &PageEntry) -> Result<u64> {
        // The local header repeats the name and has its own extra field, whose
        // length can differ from the central directory's copy.
        let mut header = [0u8; LOCAL_HEADER_LEN];
        read_exact_at(&self.file, &mut header, page.header_offset)?;
        let signature = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        if signature != SIG_LOCAL_HEADER {
            return Err(ArchiveError::Invalid(format!("no local header for {}", page.name)));
        }
        let name_len = u16::from_le_bytes([header[26], header[27]]) as u64;
        let extra_len = u16::from_le_bytes([header[28], header[29]]) as u64;
        let data_offset = page.header_offset + LOCAL_HEADER_LEN as u64 + name_len + extra_len;
        if data_offset + page.compressed_size > self.file_size {
            return Err(ArchiveError::Invalid(format!("{} runs past the end of the file", page.name)));
        }
        Ok(data_offset)
    }

    /// Deflated pages are streamed through the `zip` crate's decoder.
    fn read_deflated(&self, page: &PageEntry) -> Result<Vec<u8>> {
        let mut guard = self.inflater.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if guard.is_none() {
            let reader = BufReader::new(File::open(&self.path)?);
            let archive = zip::ZipArchive::new(reader)
                .map_err(|err| ArchiveError::Invalid(err.to_string()))?;
            *guard = Some(archive);
        }
        let Some(archive) = guard.as_mut() else {
            unreachable!("inflater was just created");
        };

        let mut entry = archive
            .by_index(page.cd_index)
            .map_err(|err| ArchiveError::Invalid(err.to_string()))?;
        if entry.crc32() != page.crc32 || entry.size() != page.size {
            return Err(ArchiveError::Invalid(format!("index mismatch for {}", page.name)));
        }
        let mut data = Vec::with_capacity(page.size as usize);
        entry.read_to_end(&mut data)?;
        Ok(data)
    }
}
