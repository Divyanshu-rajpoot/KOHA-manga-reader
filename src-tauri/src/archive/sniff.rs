//! Decides which archive entries are pages, and what image type a page is.

const IMAGE_EXTENSIONS: [&str; 8] = ["jpg", "jpeg", "jpe", "png", "webp", "gif", "bmp", "avif"];

/// True when an entry name looks like a page image.
/// Skips directories, `__MACOSX` resource forks and hidden files.
pub fn is_page_name(name: &str) -> bool {
    if name.ends_with('/') || name.ends_with('\\') {
        return false;
    }
    let mut segments = name.split(['/', '\\']);
    let file_name = segments.next_back().unwrap_or("");
    if file_name.starts_with('.') || segments.any(|dir| dir == "__MACOSX") {
        return false;
    }
    match file_name.rsplit_once('.') {
        Some((stem, ext)) => {
            !stem.is_empty() && IMAGE_EXTENSIONS.iter().any(|known| ext.eq_ignore_ascii_case(known))
        }
        None => false,
    }
}

/// MIME type from the first bytes of the image. The file extension is not trusted.
pub fn mime_from_bytes(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" && &bytes[8..12] == b"avif" {
        "image/avif"
    } else if bytes.starts_with(b"BM") {
        "image/bmp"
    } else {
        "application/octet-stream"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_page_images() {
        for name in ["001.jpg", "Vol 1/Chapter 2/p010.PNG", "a\\b\\c.webp", "x.jpeg", "p.avif"] {
            assert!(is_page_name(name), "{name}");
        }
    }

    #[test]
    fn rejects_non_pages() {
        for name in [
            "ComicInfo.xml",
            "folder/",
            "__MACOSX/._001.jpg",
            "__MACOSX/Vol 1/001.jpg",
            ".hidden.jpg",
            "dir/.DS_Store",
            "Thumbs.db",
            "noextension",
            ".jpg",
        ] {
            assert!(!is_page_name(name), "{name}");
        }
    }

    #[test]
    fn sniffs_mime_types() {
        assert_eq!(mime_from_bytes(&[0xFF, 0xD8, 0xFF, 0xE0]), "image/jpeg");
        assert_eq!(mime_from_bytes(&[0x89, b'P', b'N', b'G', 0x0D]), "image/png");
        assert_eq!(mime_from_bytes(b"GIF89a"), "image/gif");
        assert_eq!(mime_from_bytes(b"RIFF\0\0\0\0WEBPVP8 "), "image/webp");
        assert_eq!(mime_from_bytes(b"\0\0\0\x1cftypavif"), "image/avif");
        assert_eq!(mime_from_bytes(b"BM\0\0"), "image/bmp");
        assert_eq!(mime_from_bytes(b"hello"), "application/octet-stream");
        assert_eq!(mime_from_bytes(&[]), "application/octet-stream");
    }
}
