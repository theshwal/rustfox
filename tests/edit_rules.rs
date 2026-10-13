//! Replays contract/edit-rules.json against the Rust port.
//!
//! The line-ending fixtures were produced by executing the original's own
//! helper source, so this compares the port against openfox's real code rather
//! than against a description of it.

use rustfox::editing::{
    ReplaceOutcome, count_occurrences, detect_line_ending, normalize_to_lf, replace_string,
};
use serde_json::Value;

fn contract() -> Value {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/contract/edit-rules.json"
    ))
    .expect("contract/edit-rules.json must exist");
    serde_json::from_str(&raw).expect("contract must be valid JSON")
}

#[test]
fn line_endings_match_the_original() {
    let v = contract();
    let cases = v["lineEndingCases"]
        .as_array()
        .expect("lineEndingCases array");
    let mut mismatches = Vec::new();

    for case in cases {
        let name = case["name"].as_str().unwrap_or("?");
        let input = case["input"].as_str().unwrap_or("");

        let want_ending = case["expect"]["lineEnding"].as_str().unwrap_or("lf");
        let want_lf = case["expect"]["normalizedLF"].as_str().unwrap_or("");

        let got = detect_line_ending(input);
        if got.name() != want_ending {
            mismatches.push(format!("{name}: {} vs {want_ending}", got.name()));
        }
        if normalize_to_lf(input) != want_lf {
            mismatches.push(format!(
                "{name}: normalize_to_lf gave {:?} vs {want_lf:?}",
                normalize_to_lf(input)
            ));
        }
    }

    assert!(
        mismatches.is_empty(),
        "line-ending behaviour disagrees with the original:\n  {}",
        mismatches.join("\n  ")
    );
}

#[test]
fn replacement_semantics_match_the_original() {
    let v = contract();
    let cases = v["replaceCases"].as_array().expect("replaceCases array");

    for case in cases {
        let name = case["name"].as_str().unwrap_or("?");
        let haystack = case["input"]["haystack"].as_str().unwrap_or("");
        let needle = case["input"]["needle"].as_str().unwrap_or("");
        let replacement = case["input"]["replacement"].as_str().unwrap_or("");
        let replace_all = case["input"]["replaceAll"].as_bool().unwrap_or(false);

        let want_count = case["expect"]["count"].as_i64().unwrap_or(0);
        let want_result = case["expect"]["result"].as_str().unwrap_or("");

        assert_eq!(
            count_occurrences(haystack, needle),
            want_count,
            "count_occurrences disagrees for {name}"
        );

        let got = replace_string(haystack, needle, replacement, replace_all);
        match got {
            ReplaceOutcome::Replaced { result, .. } => assert_eq!(
                result, want_result,
                "replacement result disagrees for {name}"
            ),
            ReplaceOutcome::NotFound => assert_eq!(
                want_result, haystack,
                "{name} should have been left unchanged"
            ),
            ReplaceOutcome::EmptyNeedle => assert!(
                needle.is_empty(),
                "{name} reported an empty needle unexpectedly"
            ),
        }
    }
}

#[test]
fn every_edit_case_is_exercised() {
    let v = contract();
    assert!(v["lineEndingCases"].as_array().unwrap().len() >= 14);
    assert!(v["replaceCases"].as_array().unwrap().len() >= 6);

    // The asymmetry that makes detectLineEnding non-obvious: a document with
    // any CRLF is crlf even when it also contains bare LF.
    let cases = v["lineEndingCases"].as_array().unwrap();
    let mixed = cases
        .iter()
        .find(|c| c["name"] == "mixed-crlf-lf")
        .expect("mixed-crlf-lf fixture missing");
    assert_eq!(mixed["expect"]["lineEnding"], "crlf");
}
