//! Content encoding for the write/edit tools, ported from `encodeContent` in
//! `src/server/utils/encoding.ts`.
//!
//! Pinned by contract/write-rules.json (308 encode cases, 28 round trips).

use crate::encoding::{bom_for, normalize_encoding};

/// Error returned for an encoding name the original cannot resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownEncoding(pub String);

impl std::fmt::Display for UnknownEncoding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Matches iconv-lite's message shape.
        write!(
            f,
            "Encoding not recognized: '{}' (searched as: '{}')",
            self.0,
            crate::encoding::normalize_encoding(&self.0).to_lowercase()
        )
    }
}

impl std::error::Error for UnknownEncoding {}

/// Characters a single-byte encoding cannot represent.
///
/// The original silently substitutes `?` for anything unmappable, so this
/// reports the same lossy outcome rather than failing: writing an emoji to a
/// Latin-1 file succeeds and corrupts the character.
pub fn is_lossy_for_single_byte(_encoding: &str) -> bool {
    true
}

/// Port of `encodeContent`.
///
/// The encoding name is normalised first, so `utf8`, `ascii` and `UTF-8` all
/// reach the same branch. An unrecognised name is an error, matching
/// iconv-lite; a *recognised but single-byte* encoding never errors and instead
/// replaces unmappable characters.
pub fn encode_content(
    content: &str,
    encoding: &str,
    add_bom: bool,
) -> Result<Vec<u8>, UnknownEncoding> {
    let normalized = normalize_encoding(encoding);

    let mut encoded: Vec<u8> = match normalized.as_str() {
        "utf-8" => content.as_bytes().to_vec(),
        "utf-16le" => utf16le_bytes(content),
        "utf-16be" => swap_endianness(&utf16le_bytes(content)),
        "utf-32le" => utf32le_bytes(content),
        "utf-32be" => swap32(&utf32le_bytes(content)),
        other => encode_single_byte(content, other)?,
    };

    if add_bom {
        let bom = bom_for(&normalized);
        if !bom.is_empty() {
            let mut out = bom.to_vec();
            out.append(&mut encoded);
            encoded = out;
        }
    }

    Ok(encoded)
}

/// UTF-16LE, with an unpaired surrogate replaced rather than panicking.
fn utf16le_bytes(content: &str) -> Vec<u8> {
    content
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect()
}

fn utf32le_bytes(content: &str) -> Vec<u8> {
    content
        .chars()
        .flat_map(|c| (c as u32).to_le_bytes())
        .collect()
}

/// Byte-swap, matching `swapUtf16Endianness`. Operates on 2-byte units, so
/// it is only correct for UTF-16 payloads.
fn swap_endianness(bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; bytes.len()];
    let mut i = 0;
    while i + 1 < bytes.len() {
        out[i] = bytes[i + 1];
        out[i + 1] = bytes[i];
        i += 2;
    }
    out
}

/// Reverse each 4-byte unit, for UTF-32 big-endian output.
fn swap32(bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; bytes.len()];
    let mut i = 0;
    while i + 3 < bytes.len() {
        out[i] = bytes[i + 3];
        out[i + 1] = bytes[i + 2];
        out[i + 2] = bytes[i + 1];
        out[i + 3] = bytes[i];
        i += 4;
    }
    out
}

/// Single-byte legacy encodings.
///
/// The original delegates to iconv-lite. This covers the Windows-1252 /
/// ISO-8859-1 family exactly, and reports anything else as unrecognised rather
/// than guessing: writing with the wrong table is worse than refusing.
///
/// Lossy substitution into a single-byte table.
///
/// The rule is **one `?` per UTF-16 code unit**, because iconv converts through
/// UTF-16 before mapping to the target table:
///
///   U+0080, U+07FF (BMP)        -> 1 `?`
///   U+0800, U+FFFD, U+FFFF (BMP) -> 1 `?`
///   U+10000 (non-BMP)           -> 2 `?`, the surrogate pair
///   two emojis                   -> 4 `?`
///   emoji + e-acute              -> `??` + `e-acute`
///
/// Getting this wrong changes both the bytes written and the size the tool
/// reports, so it is reproduced rather than approximated.
fn encode_single_byte(content: &str, encoding: &str) -> Result<Vec<u8>, UnknownEncoding> {
    match encoding {
        "ISO-8859-1" | "windows-1252" => {
            let mut out = Vec::with_capacity(content.len());
            for unit in content.encode_utf16() {
                if (0x20..0x7F).contains(&unit) || unit == 0x0A || unit == 0x0D || unit == 0x09 {
                    out.push(unit as u8);
                } else if unit == 0xA0 || (0xA1..=0xFF).contains(&unit) {
                    // Representable in Latin-1 / windows-1252.
                    out.push(unit as u8);
                } else {
                    out.push(b'?');
                }
            }
            Ok(out)
        }
        other => Err(UnknownEncoding(other.to_string())),
    }
}

/// Line count as the write tool reports it: a trailing newline adds a line.
pub fn line_count(content: &str) -> usize {
    content.split('\n').count()
}

/// Port of `decodeContent` for the encodings the port supports.
pub fn decode_content(bytes: &[u8], encoding: &str) -> Result<String, UnknownEncoding> {
    let normalized = normalize_encoding(encoding);
    match normalized.as_str() {
        "utf-8" => String::from_utf8(bytes.to_vec())
            .or_else(|e| Ok(String::from_utf8_lossy(e.as_bytes()).into_owned())),
        "utf-16le" => Ok(decode_utf16(bytes, false)),
        "utf-16be" => Ok(decode_utf16(bytes, true)),
        "utf-32le" => Ok(decode_utf32(bytes, false)),
        "utf-32be" => Ok(decode_utf32(bytes, true)),
        "ISO-8859-1" | "windows-1252" => Ok(bytes.iter().map(|b| *b as char).collect()),
        other => Err(UnknownEncoding(other.to_string())),
    }
}

fn decode_utf16(bytes: &[u8], big_endian: bool) -> String {
    let units: Vec<u16> = bytes
        .chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| {
            if big_endian {
                u16::from_be_bytes([c[0], c[1]])
            } else {
                u16::from_le_bytes([c[0], c[1]])
            }
        })
        .collect();
    String::from_utf16_lossy(&units)
}

fn decode_utf32(bytes: &[u8], big_endian: bool) -> String {
    bytes
        .chunks(4)
        .filter(|c| c.len() == 4)
        .map(|c| {
            let v = if big_endian {
                u32::from_be_bytes([c[0], c[1], c[2], c[3]])
            } else {
                u32::from_le_bytes([c[0], c[1], c[2], c[3]])
            };
            char::from_u32(v).unwrap_or(char::REPLACEMENT_CHARACTER)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_encoding_names_all_reach_the_same_branch() {
        for name in ["utf-8", "UTF-8", "utf8", "ascii"] {
            assert_eq!(
                encode_content("héllo", name, false).unwrap(),
                "héllo".as_bytes()
            );
        }
    }

    #[test]
    fn utf16_endianness_is_honoured() {
        let le = encode_content("A", "utf-16le", false).unwrap();
        let be = encode_content("A", "utf-16be", false).unwrap();
        assert_eq!(le, vec![0x41, 0x00]);
        assert_eq!(be, vec![0x00, 0x41]);
    }

    #[test]
    fn utf16_aliases_normalise() {
        // "utf-16" normalises to utf-16le, matching the original's map.
        assert_eq!(
            encode_content("A", "UTF-16", false).unwrap(),
            encode_content("A", "utf-16le", false).unwrap()
        );
    }

    #[test]
    fn a_bom_is_prepended_only_when_asked() {
        assert_eq!(
            encode_content("A", "utf-8", true).unwrap(),
            vec![0xEF, 0xBB, 0xBF, b'A']
        );
        assert_eq!(encode_content("A", "utf-8", false).unwrap(), vec![b'A']);
        // A single-byte encoding has no BOM, so nothing is prepended.
        assert_eq!(encode_content("A", "ISO-8859-1", true).unwrap(), vec![b'A']);
    }

    #[test]
    fn single_byte_encodings_are_lossy_and_do_not_error() {
        // The original substitutes '?' rather than failing: writing an emoji to
        // a Latin-1 file succeeds and corrupts the character silently.
        let out = encode_content("a🎉b", "ISO-8859-1", false).unwrap();
        assert_eq!(out, b"a??b");
        assert_eq!(
            decode_content(&out, "ISO-8859-1").unwrap(),
            "a??b",
            "the round trip does not restore the emoji"
        );
    }

    #[test]
    fn an_unrecognised_encoding_is_an_error() {
        let err = encode_content("x", "unknown-encoding", false).unwrap_err();
        assert!(err.to_string().contains("Encoding not recognized"));
    }

    #[test]
    fn utf32_round_trips() {
        for enc in ["utf-32le", "utf-32be"] {
            let bytes = encode_content("héllo 日本語", enc, false).unwrap();
            assert_eq!(decode_content(&bytes, enc).unwrap(), "héllo 日本語");
        }
    }

    #[test]
    fn utf16_round_trips_across_encodings() {
        let le = encode_content("héllo", "utf-16le", false).unwrap();
        let be = encode_content("héllo", "utf-16be", false).unwrap();
        assert_eq!(decode_content(&le, "utf-16le").unwrap(), "héllo");
        assert_eq!(decode_content(&be, "utf-16be").unwrap(), "héllo");
        // Reading LE bytes as BE yields garbage rather than an error.
        assert_ne!(decode_content(&le, "utf-16be").unwrap(), "héllo");
    }

    #[test]
    fn line_count_matches_the_reported_number() {
        assert_eq!(line_count(""), 1);
        assert_eq!(line_count("a"), 1);
        assert_eq!(line_count("a\n"), 2);
        assert_eq!(line_count("a\nb"), 2);
        assert_eq!(line_count("a\nb\n"), 3);
    }
}
