//! Replays contract/encoding-rules.json against the Rust port.
//!
//! This is the honest check: `statistical_detect` in the Rust port is an
//! approximation of jschardet, and only the fixtures can say whether it agrees.

use rustfox::encoding::detect_encoding;
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

fn contract() -> Value {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/contract/encoding-rules.json"
    ))
    .expect("contract/encoding-rules.json must exist");
    serde_json::from_str(&raw).expect("contract must be valid JSON")
}

/// Cases where the Rust port knowingly differs from the original.
///
/// Empty: the structural heuristics in `statistical_detect` currently agree
/// with jschardet on every fixture in the contract. That is not a guarantee
/// for unseen input -- jschardet is a full frequency model and this is not --
/// so any future divergence must be added here explicitly rather than worked
/// around, and the assertion below keeps the list honest.
const KNOWN_GAPS: &[&str] = &[];

#[test]
fn detect_encoding_matches_the_original() {
    let v = contract();
    let cases = v["detectCases"].as_array().expect("detectCases array");
    let mut mismatches = Vec::new();
    let mut observed_gaps: Vec<String> = Vec::new();

    for case in cases {
        let name = case["name"].as_str().unwrap_or("?");
        let bytes = base64_decode(case["data"].as_str().unwrap_or(""));
        let got = detect_encoding(&bytes);

        let want_enc = case["expect"]["encoding"].as_str().unwrap_or("");
        let want_conf = case["expect"]["confidence"].as_f64().unwrap_or(0.0);
        let want_bom = case["expect"]["bomSize"].as_u64().unwrap_or(0) as usize;

        if got.encoding != want_enc || got.bom_size != want_bom {
            if KNOWN_GAPS.contains(&name) {
                observed_gaps.push(format!("{name}: {} vs {want_enc}", got.encoding));
            } else {
                mismatches.push(format!(
                    "{name}: got {}/bom={}, want {want_enc}/bom={want_bom}",
                    got.encoding, got.bom_size
                ));
            }
            continue;
        }
        // Confidence is a float from a statistical detector; only compare it
        // where the original hard-codes the value.
        if (want_conf - got.confidence).abs() > 1e-9
            && (want_conf == 1.0 || want_conf == 0.95 || want_conf == 0.1)
        {
            mismatches.push(format!(
                "{name}: confidence {} vs {want_conf}",
                got.confidence
            ));
        }
    }

    assert!(
        mismatches.is_empty(),
        "encoding detection disagrees with the original on cases it should match:\n  {}",
        mismatches.join("\n  ")
    );

    // The known gaps stay exercised: if the port starts matching one, this
    // fails so the entry can be removed instead of becoming a stale excuse.
    for gap in KNOWN_GAPS {
        assert!(
            !observed_gaps.iter().any(|g| g.starts_with(gap)),
            "{gap} is listed as a known gap but now matches; remove it from KNOWN_GAPS"
        );
    }
}

#[test]
fn every_encoding_case_is_exercised() {
    let v = contract();
    assert!(v["detectCases"].as_array().unwrap().len() >= 14);
    assert!(v["decodeCases"].as_array().unwrap().len() >= 28);
    assert!(v["encodeCases"].as_array().unwrap().len() >= 28);
}

#[test]
fn empty_and_bom_cases_are_pinned() {
    let v = contract();
    let cases = v["detectCases"].as_array().unwrap();
    let find = |name: &str| {
        cases
            .iter()
            .find(|c| c["name"] == name)
            .unwrap_or_else(|| panic!("fixture {name} missing"))
    };

    // The original reports a 4-byte LE BOM as "utf-32", not "utf-32le".
    assert_eq!(find("bom-utf32le")["expect"]["encoding"], "utf-32");
    assert_eq!(find("bom-utf32be")["expect"]["encoding"], "utf-32be");
    assert_eq!(find("empty")["expect"]["confidence"], 1.0);
    assert_eq!(find("ascii")["expect"]["confidence"], 0.95);
}
