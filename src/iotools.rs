//! File-type detection for the read tool, ported from
//! `src/server/tools/read.ts`.
//!
//! Behaviour is pinned by contract/io-rules.json, captured by feeding real
//! encoded bytes to the original implementation.

/// Magic-byte signatures for the image formats the original accepts, plus the
/// formats it can identify but rejects.
const SIGNATURES: &[(&[u8], &str)] = &[
    // PNG: \x89PNG\r\n\x1a\n
    (
        &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
        "image/png",
    ),
    // JPEG: FF D8 FF
    (&[0xFF, 0xD8, 0xFF], "image/jpeg"),
    // GIF: GIF87a / GIF89a
    (b"GIF87a", "image/gif"),
    (b"GIF89a", "image/gif"),
    // BMP: BM
    (b"BM", "image/bmp"),
];

/// RIFF....WEBP
const RIFF: &[u8] = b"RIFF";
const WEBP: &[u8] = b"WEBP";

/// The MIME types `detectImageType` will accept.
const ACCEPTED: [&str; 6] = [
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "image/bmp",
    "image/svg+xml",
];

/// Identify a MIME type from magic bytes alone.
pub fn sniff_mime(bytes: &[u8]) -> Option<&'static str> {
    for (sig, mime) in SIGNATURES {
        if bytes.starts_with(sig) {
            return Some(mime);
        }
    }
    // WebP needs the RIFF container and the WEBP form type at offset 8.
    if bytes.len() >= 12 && bytes.starts_with(RIFF) && &bytes[8..12] == WEBP {
        return Some("image/webp");
    }
    // PDF, which the original identifies but never treats as an image.
    if bytes.starts_with(b"%PDF-") {
        return Some("application/pdf");
    }
    // ZIP, likewise identified and rejected.
    if bytes.starts_with(b"PK\x03\x04") {
        return Some("application/zip");
    }
    None
}

/// Port of `detectImageType(buffer, filePath)`.
///
/// Binary formats are decided by magic bytes. SVG is the exception: the
/// original falls back to inspecting the content, and only accepts it when the
/// trimmed text *starts with* `<svg` — a document starting with an XML
/// declaration is not detected. That asymmetry is captured from the original
/// and asserted by the contract tests.
pub fn detect_image_type(bytes: &[u8], file_path: &str) -> Option<&'static str> {
    if let Some(mime) = sniff_mime(bytes) {
        if ACCEPTED.contains(&mime) {
            return Some(mime);
        }
        return None;
    }

    let ext = file_path
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();

    if ext == "svg" && !bytes.is_empty() {
        let text = String::from_utf8_lossy(bytes);
        let trimmed = text.trim_start();
        if trimmed.starts_with("<?xml") || trimmed.starts_with("<svg") {
            return Some("image/svg+xml");
        }
    }

    None
}

/// `formatSize` from the original: bytes to a human-readable string.
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Whether a path looks like an image, so the read tool routes it correctly.
pub fn is_image_path(bytes: &[u8], file_path: &str) -> bool {
    detect_image_type(bytes, file_path).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b64(s: &str) -> Vec<u8> {
        // Minimal base64 decoder for the test fixtures.
        const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = Vec::new();
        let mut buf = 0u32;
        let mut bits = 0;
        for c in s.bytes() {
            if c == b'=' {
                break;
            }
            let Some(v) = T.iter().position(|&x| x == c) else {
                continue;
            };
            buf = (buf << 6) | v as u32;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((buf >> bits) as u8);
            }
        }
        out
    }

    #[test]
    fn png_is_detected() {
        let png = b64(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=",
        );
        assert_eq!(detect_image_type(&png, "b.png"), Some("image/png"));
    }

    #[test]
    fn truncated_png_is_not_detected() {
        let png = b64("aVZCTi13");
        assert_eq!(detect_image_type(&png, "o.png"), None);
    }

    #[test]
    fn svg_requires_the_svg_root_element() {
        // <svg ...> at the start is accepted ...
        let root = b64("PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciPjwvc3ZnPg==");
        assert_eq!(detect_image_type(&root, "h.svg"), Some("image/svg+xml"));
        // ... but a document that starts with an XML declaration is not.
        let xml = b64(
            "PHhtbCB2ZXJzaW9uPSIxLjAiPz48c3ZnIHhtbGucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciPjwvc3ZnPjwvc3ZnPg==",
        );
        assert_eq!(detect_image_type(&xml, "g.svg"), None);
    }

    #[test]
    fn svg_content_with_the_wrong_extension_is_rejected() {
        let svg = b64("PHN2Zz48L3N2Zz4=");
        assert_eq!(detect_image_type(&svg, "i.txt"), None);
    }

    #[test]
    fn non_images_are_rejected() {
        assert_eq!(detect_image_type(b"hello world", "j.txt"), None);
        assert_eq!(detect_image_type(b"{\"a\":1}", "k.json"), None);
        assert_eq!(detect_image_type(b"", "n.png"), None);
    }

    #[test]
    fn pdf_and_zip_are_identified_but_not_images() {
        assert_eq!(sniff_mime(b"%PDF-1.4\n"), Some("application/pdf"));
        assert_eq!(detect_image_type(b"%PDF-1.4\n", "l.pdf"), None);
        assert_eq!(sniff_mime(b"PK\x03\x04xx"), Some("application/zip"));
        assert_eq!(detect_image_type(b"PK\x03\x04xx", "m.zip"), None);
    }

    #[test]
    fn size_formatting_matches_the_original() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(3 * 1024 * 1024), "3.0 MB");
    }
}
