//! Reads a ZIP / ZIP64 central directory without touching the file data.
//!
//! Opening an archive is: one read of the file tail (End of Central Directory),
//! at most two small reads for the ZIP64 records, and one read of the central
//! directory itself. Cost depends on the entry count, not the file size.

use std::fs::File;

use super::{read_exact_at, ArchiveError, Result};

const SIG_EOCD: u32 = 0x0605_4b50;
const SIG_ZIP64_LOCATOR: u32 = 0x0706_4b50;
const SIG_ZIP64_EOCD: u32 = 0x0606_4b50;
const SIG_CENTRAL_ENTRY: u32 = 0x0201_4b50;

const EOCD_LEN: usize = 22;
const ZIP64_LOCATOR_LEN: usize = 20;
const ZIP64_EOCD_LEN: usize = 56;
const CENTRAL_ENTRY_LEN: usize = 46;
const MAX_COMMENT_LEN: usize = u16::MAX as usize;
/// Refuse central directories larger than this (a corrupt size field would otherwise allocate it).
const MAX_CENTRAL_DIR_LEN: u64 = 1 << 30;

/// One central-directory record, as stored in the archive.
#[derive(Debug, Clone)]
pub struct RawEntry {
    pub name: String,
    pub flags: u16,
    pub method: u16,
    pub crc32: u32,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    /// Offset of the entry's local file header from the start of the file.
    pub header_offset: u64,
    /// Position of this record in the central directory.
    pub cd_index: usize,
}

fn le16(buf: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([buf[at], buf[at + 1]])
}

fn le32(buf: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]])
}

fn le64(buf: &[u8], at: usize) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&buf[at..at + 8]);
    u64::from_le_bytes(bytes)
}

fn invalid(message: &'static str) -> ArchiveError {
    ArchiveError::Invalid(message.to_string())
}

/// Where the central directory lives, after resolving ZIP64.
struct Directory {
    entries: u64,
    size: u64,
    /// Absolute offset in the file.
    start: u64,
    /// Added to every local header offset. Non-zero when data was prepended to the archive.
    base: u64,
    zip64: bool,
}

fn locate_directory(file: &File, file_len: u64) -> Result<Directory> {
    if file_len < EOCD_LEN as u64 {
        return Err(invalid("file is too small to be a ZIP archive"));
    }

    let tail_len = file_len.min((EOCD_LEN + MAX_COMMENT_LEN) as u64) as usize;
    let tail_start = file_len - tail_len as u64;
    let mut tail = vec![0u8; tail_len];
    read_exact_at(file, &mut tail, tail_start)?;

    let eocd_at = (0..=tail_len - EOCD_LEN)
        .rev()
        .find(|&i| {
            le32(&tail, i) == SIG_EOCD && i + EOCD_LEN + le16(&tail, i + 20) as usize <= tail_len
        })
        .ok_or_else(|| invalid("not a ZIP archive (no end-of-central-directory record)"))?;
    let eocd_pos = tail_start + eocd_at as u64;

    // ZIP64: a locator sits directly before the EOCD record.
    if eocd_pos >= ZIP64_LOCATOR_LEN as u64 {
        let mut locator = [0u8; ZIP64_LOCATOR_LEN];
        read_exact_at(file, &mut locator, eocd_pos - ZIP64_LOCATOR_LEN as u64)?;
        if le32(&locator, 0) == SIG_ZIP64_LOCATOR {
            let record_pos = le64(&locator, 8);
            if record_pos
                .checked_add(ZIP64_EOCD_LEN as u64)
                .is_none_or(|end| end > file_len)
            {
                return Err(invalid("ZIP64 record offset is outside the file"));
            }
            let mut record = [0u8; ZIP64_EOCD_LEN];
            read_exact_at(file, &mut record, record_pos)?;
            if le32(&record, 0) != SIG_ZIP64_EOCD {
                return Err(invalid("ZIP64 end-of-central-directory record is missing"));
            }
            let size = le64(&record, 40);
            let offset = le64(&record, 48);
            let base = offset
                .checked_add(size)
                .and_then(|end| record_pos.checked_sub(end))
                .ok_or_else(|| invalid("ZIP64 central directory does not fit in the file"))?;
            return Ok(Directory {
                entries: le64(&record, 32),
                size,
                start: offset + base,
                base,
                zip64: true,
            });
        }
    }

    let size = le32(&tail, eocd_at + 12) as u64;
    let offset = le32(&tail, eocd_at + 16) as u64;
    let base = eocd_pos
        .checked_sub(offset + size)
        .ok_or_else(|| invalid("central directory does not fit in the file"))?;
    Ok(Directory {
        entries: le16(&tail, eocd_at + 10) as u64,
        size,
        start: offset + base,
        base,
        zip64: false,
    })
}

fn decode_name(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(name) => name.to_string(),
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Reads every record of the central directory, in directory order.
/// Also says whether the archive uses ZIP64 records.
pub fn read_central_directory(file: &File, file_len: u64) -> Result<(Vec<RawEntry>, bool)> {
    let dir = locate_directory(file, file_len)?;
    if dir.size > MAX_CENTRAL_DIR_LEN || dir.start.checked_add(dir.size).is_none_or(|end| end > file_len) {
        return Err(invalid("central directory size is not plausible"));
    }

    let mut cd = vec![0u8; dir.size as usize];
    read_exact_at(file, &mut cd, dir.start)?;

    let capacity = dir.entries.min(dir.size / CENTRAL_ENTRY_LEN as u64) as usize;
    let mut entries = Vec::with_capacity(capacity);
    let mut pos = 0usize;

    while (entries.len() as u64) < dir.entries {
        if pos + CENTRAL_ENTRY_LEN > cd.len() || le32(&cd, pos) != SIG_CENTRAL_ENTRY {
            return Err(invalid("central directory is truncated or corrupt"));
        }
        let flags = le16(&cd, pos + 8);
        let method = le16(&cd, pos + 10);
        let crc32 = le32(&cd, pos + 16);
        let mut compressed_size = le32(&cd, pos + 20) as u64;
        let mut uncompressed_size = le32(&cd, pos + 24) as u64;
        let name_len = le16(&cd, pos + 28) as usize;
        let extra_len = le16(&cd, pos + 30) as usize;
        let comment_len = le16(&cd, pos + 32) as usize;
        let mut header_offset = le32(&cd, pos + 42) as u64;

        let name_at = pos + CENTRAL_ENTRY_LEN;
        let extra_at = name_at + name_len;
        let next = extra_at + extra_len + comment_len;
        if next > cd.len() {
            return Err(invalid("central directory entry runs past the directory"));
        }

        // ZIP64 extra field (id 0x0001): holds, in this order, only the values
        // whose 32-bit field above is saturated.
        let extra = &cd[extra_at..extra_at + extra_len];
        let mut at = 0usize;
        while at + 4 <= extra.len() {
            let id = le16(extra, at);
            let len = le16(extra, at + 2) as usize;
            let body_at = at + 4;
            if body_at + len > extra.len() {
                break;
            }
            if id == 0x0001 {
                let body = &extra[body_at..body_at + len];
                let mut field = 0usize;
                let mut take = |target: &mut u64| {
                    if *target == u32::MAX as u64 && field + 8 <= body.len() {
                        *target = le64(body, field);
                        field += 8;
                    }
                };
                take(&mut uncompressed_size);
                take(&mut compressed_size);
                take(&mut header_offset);
                break;
            }
            at = body_at + len;
        }

        entries.push(RawEntry {
            name: decode_name(&cd[name_at..extra_at]),
            flags,
            method,
            crc32,
            compressed_size,
            uncompressed_size,
            // A corrupt 64-bit offset must not overflow.
            header_offset: header_offset
                .checked_add(dir.base)
                .ok_or_else(|| invalid("entry offset is outside the file"))?,
            cd_index: entries.len(),
        });
        pos = next;
    }

    Ok((entries, dir.zip64))
}
