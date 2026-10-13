//! Replays contract/io-rules.json against the Rust port.
//!
//! The fixtures were produced by feeding real encoded bytes to the original's
//! `detectImageType`, so this checks agreement with openfox rather than with an
//! assumption about what file types should look like.

use rustfox::iotools::detect_image_type;
use serde_json::Value;

fn base64_decode(s: &str) -> Vec<u8> {
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

fn cases() -> Vec<Value> {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/contract/io-rules.json"
    ))
    .expect("contract/io-rules.json must exist");
    let v: Value = serde_json::from_str(&raw).expect("contract must be valid JSON");
    v["detectImageTypeCases"]
        .as_array()
        .expect("detectImageTypeCases array")
        .clone()
}

#[test]
fn detect_image_type_matches_the_original() {
    let mut checked = 0;
    for case in cases() {
        let name = case["name"].as_str().unwrap_or("?");
        let path = case["path"].as_str().unwrap_or("");
        let bytes = base64_decode(case["data"].as_str().unwrap_or(""));

        let expected: Option<String> = match &case["expect"] {
            Value::Null => None,
            Value::String(s) => Some(s.clone()),
            other => panic!("unexpected expectation {other} for {name}"),
        };
        let expected_ref = expected.as_deref();

        assert_eq!(
            detect_image_type(&bytes, path),
            expected_ref,
            "detect_image_type({name}, {path}) disagreed with the original"
        );
        checked += 1;
    }
    assert!(checked >= 14, "only replayed {checked} cases");
}

#[test]
fn base64_decoder_produces_expected_prefix() {
    let png = base64_decode(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=",
    );
    assert_eq!(
        &png[..8],
        &[0x89u8, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
    );
}

#[test]
fn every_fixture_is_accounted_for() {
    let names: Vec<String> = cases()
        .iter()
        .map(|c| c["name"].as_str().unwrap_or("").to_string())
        .collect();
    for expected in [
        "png-real-header",
        "svg-root",
        "svg-xml",
        "truncated-png",
        "pdf",
        "zip",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "fixture {expected} missing from the contract"
        );
    }
}
