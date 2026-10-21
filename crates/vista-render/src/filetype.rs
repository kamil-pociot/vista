//! Deciding how to show a file.

use std::path::Path;

/// Text files larger than this aren't loaded into the viewer.
pub const MAX_TEXT_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Text,
    Markdown,
    Image,
    Binary,
    TooLarge,
}

const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "tif", "tiff", "qoi", "tga",
];
const MARKDOWN_EXTS: &[&str] = &["md", "markdown", "mdown", "mkd", "mdx"];

fn ext(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
}

pub fn is_image_path(path: &Path) -> bool {
    IMAGE_EXTS.contains(&ext(path).as_str())
}

pub fn is_markdown_path(path: &Path) -> bool {
    MARKDOWN_EXTS.contains(&ext(path).as_str())
}

/// Looks like binary data: a NUL byte in the first 8 KiB.
pub fn looks_binary(head: &[u8]) -> bool {
    head.iter().take(8192).any(|b| *b == 0)
}

/// Classifies a file from its path, size and first bytes.
pub fn classify(path: &Path, size: u64, head: &[u8]) -> FileKind {
    if is_image_path(path) {
        return FileKind::Image;
    }
    if looks_binary(head) {
        return FileKind::Binary;
    }
    if size > MAX_TEXT_BYTES {
        return FileKind::TooLarge;
    }
    if is_markdown_path(path) {
        FileKind::Markdown
    } else {
        FileKind::Text
    }
}

/// Classic hex dump (`offset  hex bytes  ascii`), 16 bytes per line.
pub fn hex_dump(data: &[u8], max_lines: usize) -> Vec<String> {
    data.chunks(16)
        .take(max_lines)
        .enumerate()
        .map(|(i, chunk)| {
            let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
            let ascii: String = chunk
                .iter()
                .map(|b| {
                    if b.is_ascii_graphic() || *b == b' ' {
                        *b as char
                    } else {
                        '.'
                    }
                })
                .collect();
            format!("{:08x}  {:<47}  {ascii}", i * 16, hex.join(" "))
        })
        .collect()
}

/// `1.2 MB`-style sizes.
pub fn human_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u + 1 < UNITS.len() {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_files() {
        assert_eq!(classify(Path::new("a.PNG"), 10, b"\x89PNG"), FileKind::Image);
        assert_eq!(classify(Path::new("README.md"), 10, b"# hi"), FileKind::Markdown);
        assert_eq!(classify(Path::new("main.rs"), 10, b"fn main"), FileKind::Text);
        assert_eq!(classify(Path::new("app"), 10, b"\x7fELF\0\0"), FileKind::Binary);
        assert_eq!(
            classify(Path::new("big.log"), MAX_TEXT_BYTES + 1, b"text"),
            FileKind::TooLarge
        );
    }

    #[test]
    fn hex_dumps() {
        let d = hex_dump(b"Hello\0World", 4);
        assert_eq!(d.len(), 1);
        assert!(d[0].starts_with("00000000  48 65 6c 6c 6f 00 57"));
        assert!(d[0].ends_with("Hello.World"));
    }

    #[test]
    fn human_sizes() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(25 * 1024 * 1024), "25.0 MB");
    }
}
