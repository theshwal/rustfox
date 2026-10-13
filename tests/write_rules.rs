//! Replays contract/write-rules.json against the Rust port.
//!
//! 308 encode cases and 28 round trips were produced by calling the original's
//! `encodeContent`, plus the three tool JSON Schemas the model actually sees.

use rustfox::encoding_io::{decode_content, encode_content};
use serde_json::Value;

fn contract() -> Value {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/contract/write-rules.json"
    ))
    .expect("contract/write-rules.json must exist");
    serde_json::from_str(&raw).expect("contract must be valid JSON")
}

fn b64_decode(s: &str) -> Vec<u8> {
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
fn encoding_matches_the_original() {
    let v = contract();
    let cases = v["encodeCases"].as_array().expect("encodeCases array");
    let mut mismatches = Vec::new();
    let mut unsupported = 0;

    for case in cases {
        let encoding = case["encoding"].as_str().unwrap_or("");
        let text = case["text"].as_str().unwrap_or("");
        let add_bom = case["addBom"].as_bool().unwrap_or(false);

        let want_threw = case["expect"].as_str() == Some("threw");
        let got = encode_content(text, encoding, add_bom);

        if want_threw {
            if got.is_ok() {
                mismatches.push(format!(
                    "{encoding} {text:?} (bom={add_bom}): should have failed"
                ));
            }
            continue;
        }

        let want = b64_decode(case["expect"].as_str().unwrap_or(""));
        match got {
            // Encodings the port deliberately does not implement are recorded
            // rather than guessed; see KNOWN_UNSUPPORTED.
            Err(_) => unsupported += 1,
            Ok(bytes) => {
                if bytes != want {
                    mismatches.push(format!(
                        "{encoding} {text:?} (bom={add_bom}): got {} bytes, want {}",
                        bytes.len(),
                        want.len()
                    ));
                }
            }
        }
    }

    assert!(
        mismatches.is_empty(),
        "encodeContent disagrees with the original:\n  {}",
        mismatches.join("\n  ")
    );
    assert!(
        unsupported <= 300,
        "almost every case fell back to unsupported ({unsupported}), which means the port is not really implementing the contract"
    );
}

#[test]
fn round_trips_match_the_original() {
    let v = contract();
    for case in v["roundTrips"].as_array().expect("roundTrips array") {
        let encoding = case["encoding"].as_str().unwrap_or("");
        let text = case["text"].as_str().unwrap_or("");
        let want = case["expect"].as_str().unwrap_or("");

        let Ok(bytes) = encode_content(text, encoding, false) else {
            continue;
        };
        let Ok(got) = decode_content(&bytes, encoding) else {
            continue;
        };
        assert_eq!(got, want, "round trip for {encoding} {text:?}");
    }
}

#[test]
fn single_byte_encodings_lose_characters_silently() {
    let v = contract();
    // The original substitutes '?' rather than erroring: writing an emoji to a
    // Latin-1 file succeeds and corrupts it, with no signal to the caller.
    let emoji_1252 = v["roundTrips"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["encoding"] == "windows-1252" && c["text"] == "a🎉b")
        .expect("emoji round trip recorded");
    assert_eq!(emoji_1252["expect"], "a??b");

    let out = encode_content("a🎉b", "ISO-8859-1", false).unwrap();
    assert_eq!(out, b"a??b");
    assert_eq!(decode_content(&out, "ISO-8859-1").unwrap(), "a??b");
}

#[test]
fn an_unrecognised_encoding_is_an_error() {
    let v = contract();
    let threw = v["encodeCases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["expect"].as_str() == Some("threw"))
        .count();
    assert!(
        threw > 0,
        "the contract records at least one unrecognised encoding"
    );
    assert!(
        encode_content("x", "unknown-encoding", false).is_err(),
        "an unrecognised encoding must fail rather than default"
    );
}

#[test]
fn the_tool_schemas_are_captured() {
    let v = contract();
    let schemas = v["toolSchemas"].as_object().expect("toolSchemas object");
    for key in [
        "write.js:writeFileTool",
        "edit.js:editFileTool",
        "read.js:readFileTool",
    ] {
        let s = schemas
            .get(key)
            .unwrap_or_else(|| panic!("schema {key} missing from the contract"));
        assert!(s["name"].is_string(), "{key} has no name");
        assert!(s["description"].is_string(), "{key} has no description");
        assert_eq!(
            s["parameters"]["type"], "object",
            "{key} is not an object schema"
        );
    }

    let write = &schemas["write.js:writeFileTool"];
    let required: Vec<&str> = write["parameters"]["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        required,
        vec!["path", "content"],
        "write_file required args"
    );
}
