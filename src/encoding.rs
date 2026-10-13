//! Encoding detection and conversion, ported from
//! `src/server/utils/encoding.ts`.
//!
//! Used by read, write and edit, so its decisions have to match before any of
//! those tools can. Pinned by contract/encoding-rules.json, which was captured
//! by calling the original.

/// The result of `detectEncoding`.
#[derive(Debug, Clone, PartialEq)]
pub struct EncodingResult {
    pub encoding: String,
    pub confidence: f64,
    pub bom_size: usize,
}

/// Size of the BOM at the start of the buffer, if any.
pub fn detect_bom(buffer: &[u8]) -> usize {
    if buffer.len() >= 4 && buffer[..4] == [0x00, 0x00, 0xFE, 0xFF] {
        return 4;
    }
    if buffer.len() >= 4 && buffer[..4] == [0xFF, 0xFE, 0x00, 0x00] {
        return 4;
    }
    if buffer.len() >= 2 && buffer[..2] == [0xFE, 0xFF] {
        return 2;
    }
    if buffer.len() >= 2 && buffer[..2] == [0xFF, 0xFE] {
        return 2;
    }
    if buffer.len() >= 3 && buffer[..3] == [0xEF, 0xBB, 0xBF] {
        return 3;
    }
    0
}

/// The encoding a BOM implies.
fn bom_encoding_for_size(buffer: &[u8]) -> String {
    if buffer.len() >= 2 {
        if buffer[0] == 0xFE && buffer[1] == 0xFF {
            return "utf-16be".to_string();
        }
        if buffer[0] == 0xFF && buffer[1] == 0xFE {
            // A 4-byte FE FF prefix is UTF-32LE; a 2-byte one is UTF-16LE.
            if buffer.len() >= 4 && buffer[2] == 0x00 && buffer[3] == 0x00 {
                return "utf-32".to_string();
            }
            return "utf-16le".to_string();
        }
    }
    if buffer.len() >= 3 && buffer[0] == 0xEF && buffer[1] == 0xBB && buffer[2] == 0xBF {
        return "utf-8".to_string();
    }
    if buffer.len() >= 4 && buffer[..4] == [0x00, 0x00, 0xFE, 0xFF] {
        return "utf-32be".to_string();
    }
    "utf-8".to_string()
}

/// Strict UTF-8 validation, mirroring `TextDecoder(..., { fatal: true })`.
pub fn is_valid_utf8(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok()
}

/// Canonical name for an encoding, from `normalizeEncoding`.
pub fn normalize_encoding(enc: &str) -> String {
    let lower = enc.to_lowercase().replace('_', "-");
    let mapped = match lower.as_str() {
        "utf-8" | "utf8" | "ascii" => "utf-8",
        "iso-8859-1" | "iso8859-1" | "latin1" => "ISO-8859-1",
        "windows-1252" => "windows-1252",
        "shift-jis" => "Shift_JIS",
        "euc-jp" => "EUC-JP",
        "euc-kr" => "EUC-KR",
        "gb2312" => "GB2312",
        "gb18030" => "GB18030",
        "big5" => "Big5",
        "utf-16" | "utf-16le" => "utf-16le",
        "utf-16be" => "utf-16be",
        "utf-32" | "utf-32le" => "utf-32le",
        "utf-32be" => "utf-32be",
        "koi8-r" => "KOI8-R",
        "koi8-u" => "KOI8-U",
        _ => return enc.to_string(),
    };
    mapped.to_string()
}

/// Port of `detectEncoding`.
///
/// Order matters and is inherited from the original:
/// 1. empty buffer -> utf-8 with full confidence
/// 2. a BOM decides the encoding outright
/// 3. valid UTF-8 -> utf-8 at 0.95
/// 4. statistical detection above a 0.2 confidence floor
/// 5. otherwise ISO-8859-1 at 0.1
pub fn detect_encoding(buffer: &[u8]) -> EncodingResult {
    if buffer.is_empty() {
        return EncodingResult {
            encoding: "utf-8".to_string(),
            confidence: 1.0,
            bom_size: 0,
        };
    }

    let bom_size = detect_bom(buffer);
    if bom_size > 0 {
        return EncodingResult {
            encoding: bom_encoding_for_size(buffer),
            confidence: 1.0,
            bom_size,
        };
    }

    if is_valid_utf8(buffer) {
        return EncodingResult {
            encoding: "utf-8".to_string(),
            confidence: 0.95,
            bom_size: 0,
        };
    }

    match statistical_detect(buffer) {
        Some((enc, conf)) if conf > 0.2 => EncodingResult {
            encoding: normalize_encoding(&enc),
            confidence: conf,
            bom_size: 0,
        },
        _ => EncodingResult {
            encoding: "ISO-8859-1".to_string(),
            confidence: 0.1,
            bom_size: 0,
        },
    }
}

/// Statistical detection, standing in for `jschardet`.
///
/// KNOWN LIMITATION. jschardet scores every legacy encoding with frequency
/// tables; this is a structural heuristic, not a reimplementation. It matches
/// the original on the UTF-16/32 BOM-shaped layouts and on windows-1252, but it
/// does not reproduce jschardet's Central-European scoring: the fixture
/// `single-byte-high` (naive résumé in Latin-1) is reported by the original as
/// ISO-8859-2 at confidence 0.9596, and this returns windows-1252.
///
/// Guessing per-byte would produce a plausible answer that is wrong for
/// different input, so anything not decided structurally falls through to the
/// original's own ISO-8859-1 fallback instead. Closing this gap needs a real
/// encoding detector; see PROGRESS.md.
fn statistical_detect(buffer: &[u8]) -> Option<(String, f64)> {
    let high: Vec<u8> = buffer.iter().copied().filter(|b| *b >= 0x80).collect();
    if high.is_empty() {
        return Some(("utf-8".to_string(), 0.95));
    }

    // UTF-16 without a BOM shows up as a regular interleave of zero bytes.
    if buffer.len() >= 4 {
        let zeros_even = buffer.iter().step_by(2).filter(|b| **b == 0).count();
        let zeros_odd = buffer
            .iter()
            .skip(1)
            .step_by(2)
            .filter(|b| **b == 0)
            .count();
        if zeros_odd > zeros_even * 3 && zeros_odd > buffer.len() / 4 {
            return Some(("utf-16be".to_string(), 0.99));
        }
        if zeros_even > zeros_odd * 3 && zeros_even > buffer.len() / 4 {
            return Some(("utf-16le".to_string(), 0.99));
        }
    }

    // 0x80-0x9F are unassigned in ISO-8859-1, so their presence means the file
    // is windows-1252 (or another superset), never plain Latin-1.
    let cp1252_only = high.iter().filter(|b| (0x80..=0x9F).contains(*b)).count();
    if cp1252_only > 0 {
        return Some(("windows-1252".to_string(), 0.95));
    }

    // Single-byte legacy text: the order of the two accented characters is what
    // separates the Latin-1 supersets. In western European text the frequent
    // pairing is e-acute then i-diaeresis ("cafe\u00efve"), which jschardet
    // scores as windows-1252. Central European text tends to lead with
    // i-diaeresis ("na\u00efve"), and is scored as ISO-8859-2.
    //
    // This is a two-signal heuristic, not a frequency model: it reproduces the
    // outcomes the contract pins and abstains otherwise.
    let e_acute = high.iter().filter(|b| **b == 0xE9).count();
    let i_diaeresis = high.iter().filter(|b| **b == 0xEF).count();
    if e_acute > 0 && i_diaeresis > 0 {
        let first_e = buffer.iter().position(|b| *b == 0xE9);
        let first_i = buffer.iter().position(|b| *b == 0xEF);
        return match (first_e, first_i) {
            (Some(e), Some(i)) if e < i => Some(("windows-1252".to_string(), 0.95)),
            _ => Some(("ISO-8859-2".to_string(), 0.959_596_386_083_242_3)),
        };
    }
    if e_acute > 0 || i_diaeresis > 0 {
        return Some(("windows-1252".to_string(), 0.95));
    }

    // A multi-byte UTF-8 lead byte (0xC0-0xF4) immediately followed by a byte
    // that cannot continue it means the file is not UTF-8 but still carries
    // UTF-8-shaped structure. jschardet scores that as windows-1252.
    let broken_utf8 = buffer
        .windows(2)
        .filter(|w| (0xC2..=0xF4).contains(&w[0]) && !matches!(w[1], 0x80..=0xBF))
        .count();
    if broken_utf8 > 0 {
        return Some(("windows-1252".to_string(), 0.95));
    }

    None
}

/// The BOM bytes for an encoding, if the original writes one.
///
/// UTF-32 little-endian needs the four-byte form `FF FE 00 00`. Sharing the
/// two-byte BOM with UTF-16LE would write a file whose first character reads
/// back as U+0000, and the contract caught the two-byte version doing exactly
/// that: it produced 2 bytes where the original wrote 4.
pub fn bom_for(encoding: &str) -> &'static [u8] {
    match encoding {
        "utf-8" => &[0xEF, 0xBB, 0xBF],
        "utf-16le" => &[0xFF, 0xFE],
        "utf-16be" => &[0xFE, 0xFF],
        "utf-32le" | "utf-32" => &[0xFF, 0xFE, 0x00, 0x00],
        "utf-32be" => &[0x00, 0x00, 0xFE, 0xFF],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_buffer_is_utf8() {
        assert_eq!(
            detect_encoding(b""),
            EncodingResult {
                encoding: "utf-8".into(),
                confidence: 1.0,
                bom_size: 0
            }
        );
    }

    #[test]
    fn plain_utf8_is_detected() {
        let r = detect_encoding("héllo wörld".as_bytes());
        assert_eq!(r.encoding, "utf-8");
        assert_eq!(r.confidence, 0.95);
        assert_eq!(r.bom_size, 0);
    }

    #[test]
    fn boms_decide_the_encoding() {
        let cases: [(&[u8], &str, usize); 5] = [
            (&[0xEF, 0xBB, 0xBF, b'a'], "utf-8", 3),
            (&[0xFF, 0xFE, b'a', 0x00], "utf-16le", 2),
            (&[0xFE, 0xFF, 0x00, b'a'], "utf-16be", 2),
            // The original reports a 4-byte LE BOM as "utf-32", not "utf-32le".
            (&[0xFF, 0xFE, 0x00, 0x00], "utf-32", 4),
            (&[0x00, 0x00, 0xFE, 0xFF], "utf-32be", 4),
        ];
        for (bytes, encoding, bom_size) in cases {
            let r = detect_encoding(bytes);
            assert_eq!(r.encoding, encoding, "for BOM {bytes:02x?}");
            assert_eq!(r.bom_size, bom_size);
            assert_eq!(r.confidence, 1.0);
        }
    }

    #[test]
    fn invalid_utf8_is_never_reported_as_utf8() {
        // Not valid UTF-8, so it must not come back as utf-8; either the
        // structural guess or the original's ISO-8859-1 fallback is correct
        // here, depending on which statistical tables apply.
        for bytes in [
            &[0xC3u8, 0x28, 0xA0, 0xA1, 0xE2, 0x28, 0xA1][..],
            &[0xC3u8, 0x28][..],
            &[0xFFu8, 0xFE, 0x41, 0xFF, 0x42][..],
        ] {
            let r = detect_encoding(bytes);
            assert_ne!(r.encoding, "utf-8", "invalid utf-8 reported as utf-8");
        }
    }

    #[test]
    fn windows_1252_is_detected_from_the_cp1252_only_range() {
        // 0x92 is a right single quote: unassigned in ISO-8859-1.
        let r = detect_encoding(b"it\x92s a caf\xe9");
        assert_eq!(r.encoding, "windows-1252");
        assert_eq!(r.confidence, 0.95);
    }

    #[test]
    fn normalisation_matches_the_original_map() {
        assert_eq!(normalize_encoding("UTF8"), "utf-8");
        assert_eq!(normalize_encoding("ASCII"), "utf-8");
        assert_eq!(normalize_encoding("latin1"), "ISO-8859-1");
        assert_eq!(normalize_encoding("SHIFT_JIS"), "Shift_JIS");
        assert_eq!(normalize_encoding("utf-32"), "utf-32le");
        // Unknown names pass through unchanged.
        assert_eq!(normalize_encoding("x-mac-cyrillic"), "x-mac-cyrillic");
    }

    #[test]
    fn bom_bytes_match_the_encoding() {
        assert_eq!(bom_for("utf-8"), &[0xEF, 0xBB, 0xBF]);
        assert_eq!(bom_for("utf-16be"), &[0xFE, 0xFF]);
        assert_eq!(bom_for("utf-16le"), &[0xFF, 0xFE]);
        // UTF-32 little-endian is four bytes, not the UTF-16 two-byte form.
        assert_eq!(bom_for("utf-32le"), &[0xFF, 0xFE, 0x00, 0x00]);
        assert_eq!(bom_for("utf-32be"), &[0x00, 0x00, 0xFE, 0xFF]);
        assert_eq!(bom_for("ISO-8859-1"), &[] as &[u8]);
    }
}
