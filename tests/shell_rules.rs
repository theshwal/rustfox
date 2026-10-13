//! Replays contract/shell-rules.json against the Rust port.
//!
//! All three helpers are pure and exported, so every fixture was produced by
//! calling the original directly.

use rustfox::shellrules::{detect_git_mutation, format_duration, has_background_ampersand};
use serde_json::Value;

fn contract() -> Value {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/contract/shell-rules.json"
    ))
    .expect("contract/shell-rules.json must exist");
    serde_json::from_str(&raw).expect("contract must be valid JSON")
}

#[test]
fn git_mutation_detection_matches_the_original() {
    let v = contract();
    let mut mismatches = Vec::new();

    for case in v["gitCases"].as_array().expect("gitCases array") {
        let input = case["input"].as_str().unwrap_or("");
        let want = case["expect"].as_str().map(str::to_string);
        let got = detect_git_mutation(input);
        if got != want {
            mismatches.push(format!("{input:?}: got {got:?}, want {want:?}"));
        }
    }

    assert!(
        mismatches.is_empty(),
        "detectGitMutation disagrees with the original:\n  {}",
        mismatches.join("\n  ")
    );
}

#[test]
fn duration_formatting_matches_the_original() {
    let v = contract();
    for case in v["durationCases"].as_array().expect("durationCases array") {
        let input = case["input"].as_f64().expect("numeric input");
        let want = case["expect"].as_str().unwrap_or("");
        assert_eq!(
            format_duration(input),
            want,
            "formatDuration({input}) disagrees"
        );
    }
}

#[test]
fn background_ampersand_matches_the_original() {
    let v = contract();
    for case in v["ampCases"].as_array().expect("ampCases array") {
        let input = case["input"].as_str().unwrap_or("");
        let want = case["expect"].as_bool().unwrap_or(false);
        assert_eq!(
            has_background_ampersand(input),
            want,
            "hasBackgroundAmpersand({input:?}) disagrees"
        );
    }
}

#[test]
fn every_shell_case_is_exercised() {
    let v = contract();
    assert!(v["gitCases"].as_array().unwrap().len() >= 30);
    assert!(v["durationCases"].as_array().unwrap().len() >= 26);
    assert!(v["ampCases"].as_array().unwrap().len() >= 17);
}

#[test]
fn the_quote_bypass_is_closed_in_both_directions() {
    let v = contract();
    let cases = v["gitCases"].as_array().unwrap();
    let find = |n: &str| {
        cases
            .iter()
            .find(|c| c["input"] == n)
            .expect("fixture missing")
    };

    // A quoted verb is still detected, and the fragment keeps the quote: the
    // regex runs on the raw command precisely so stripping quotes cannot let
    // this through.
    for cmd in ["git \"checkout\" main", "git 'switch' main"] {
        let expect = find(cmd)["expect"].as_str().expect("expectation recorded");
        assert_eq!(detect_git_mutation(cmd), Some(expect.to_string()));
    }

    // A bare && is not a background operator, but a lone & is.
    let amps = v["ampCases"].as_array().unwrap();
    let find_amp = |n: &str| {
        amps.iter()
            .find(|c| c["input"] == n)
            .expect("fixture missing")
    };
    assert_eq!(find_amp("&&")["expect"], serde_json::json!(false));
    assert_eq!(find_amp("&")["expect"], serde_json::json!(true));
}

#[test]
fn durations_have_no_hour_format() {
    let v = contract();
    let cases = v["durationCases"].as_array().unwrap();
    let hour = cases
        .iter()
        .find(|c| c["input"].as_f64() == Some(3_600_000.0))
        .expect("one-hour fixture missing");
    assert_eq!(
        hour["expect"], "60m 0s",
        "an hour is reported in minutes, not as 1h"
    );
    // Rounding that lands exactly on 60s carries into the next minute.
    let carry = cases
        .iter()
        .find(|c| c["input"].as_f64() == Some(119_999.0))
        .expect("rounding fixture missing");
    assert_eq!(carry["expect"], "2m 0s");
}
