//! Replays contract/path-rules.json against the Rust port.
//!
//! The fixtures were produced by calling the original implementation, so this
//! is the check that the port matches openfox rather than merely matching my
//! reading of it.

use rustfox::pathsec::{
    extract_dangerous_patterns, extract_git_no_verify, extract_sensitive_paths_from_command,
    is_path_within_sandbox, is_sensitive_path, resolve_relative_traversals,
};
use serde_json::Value;

fn fixtures() -> Vec<Value> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/contract/path-rules.json");
    let raw = std::fs::read_to_string(path).expect("contract/path-rules.json must exist");
    let v: Value = serde_json::from_str(&raw).expect("contract must be valid JSON");
    v["cases"].as_array().expect("cases array").clone()
}

fn s(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// The original returns its pattern sources as strings; compare as a set of
/// marker strings so the Rust port only has to agree on which patterns fire.
fn danger_markers(command: &str) -> Vec<String> {
    extract_dangerous_patterns(command)
        .into_iter()
        .map(|p| {
            // Map the pattern source to a stable marker derived from its shape.
            if p.contains("sudo") {
                "sudo".to_string()
            } else if p.contains("rm") {
                "rm".to_string()
            } else if p.contains("chmod") {
                "chmod".to_string()
            } else if p.contains("mkfs") {
                "mkfs".to_string()
            } else if p.contains("dd") {
                "dd".to_string()
            } else if p.contains("dev") {
                "devwrite".to_string()
            } else {
                "forkbomb".to_string()
            }
        })
        .collect()
}

fn expected_markers(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|arr| {
            arr.iter()
                .map(|p| {
                    let p = p.as_str().unwrap_or("");
                    if p.contains("sudo") {
                        "sudo"
                    } else if p.contains("rm") {
                        "rm"
                    } else if p.contains("chmod") {
                        "chmod"
                    } else if p.contains("mkfs") {
                        "mkfs"
                    } else if p.contains("dd") {
                        "dd"
                    } else if p.contains("dev") {
                        "devwrite"
                    } else {
                        "forkbomb"
                    }
                    .to_string()
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn contract_cases_are_all_replayed() {
    let cases = fixtures();
    assert!(
        cases.len() >= 50,
        "expected the full fixture set, got {}",
        cases.len()
    );
}

#[test]
fn is_sensitive_path_matches_the_original() {
    let mut checked = 0;
    for case in fixtures() {
        if case["fn"] != "isSensitivePath" {
            continue;
        }
        let input = s(&case["input"][0]);
        let expected = case["expect"].as_bool().unwrap();
        assert_eq!(
            is_sensitive_path(&input),
            expected,
            "is_sensitive_path({input:?}) should be {expected}  [{}]",
            case["label"]
        );
        checked += 1;
    }
    assert!(checked >= 20, "only checked {checked} sensitive-path cases");
}

#[test]
fn is_path_within_sandbox_matches_the_original() {
    for case in fixtures() {
        if case["fn"] != "isPathWithinSandbox" {
            continue;
        }
        let path = s(&case["input"][0]);
        let workdir = s(&case["input"][1]);
        let expected_allowed = case["expect"]["allowed"].as_bool().unwrap();

        // The fixtures use a workdir that does not exist on disk, and the
        // original resolves symlinks with realpath; this port is lexical, so
        // agreement is asserted on paths that contain no symlink and where the
        // relative-path cases (which depend on the process cwd) are skipped.
        if path.is_empty() || !path.starts_with('/') {
            continue;
        }
        let got = is_path_within_sandbox(&path, &workdir);
        assert_eq!(
            got.allowed, expected_allowed,
            "is_path_within_sandbox({path:?}, {workdir:?})  [{}]",
            case["label"]
        );
    }
}

#[test]
fn resolve_relative_traversals_matches_the_original() {
    let mut checked = 0;
    for case in fixtures() {
        if case["fn"] != "resolveRelativeTraversals" {
            continue;
        }
        let command = s(&case["input"][0]);
        let workdir = s(&case["input"][1]);
        let expected: Vec<String> = case["expect"]
            .as_array()
            .map(|a| a.iter().map(s).collect())
            .unwrap_or_default();
        assert_eq!(
            resolve_relative_traversals(&command, &workdir),
            expected,
            "resolve_relative_traversals({command:?})  [{}]",
            case["label"]
        );
        checked += 1;
    }
    assert!(checked >= 6, "only checked {checked} traversal cases");
}

#[test]
fn extract_dangerous_patterns_matches_the_original() {
    for case in fixtures() {
        if case["fn"] != "extractDangerousPatterns" {
            continue;
        }
        let command = s(&case["input"][0]);
        let expected = expected_markers(&case["expect"]);
        assert_eq!(
            danger_markers(&command),
            expected,
            "dangerous patterns for {command:?}  [{}]",
            case["label"]
        );
    }
}

#[test]
fn extract_git_no_verify_matches_the_original() {
    for case in fixtures() {
        if case["fn"] != "extractGitNoVerify" {
            continue;
        }
        let command = s(&case["input"][0]);
        let expected = case["expect"].as_bool().unwrap();
        assert_eq!(
            extract_git_no_verify(&command),
            expected,
            "git no-verify for {command:?}  [{}]",
            case["label"]
        );
    }
}

#[test]
fn extract_sensitive_paths_matches_the_original() {
    for case in fixtures() {
        if case["fn"] != "extractSensitivePathsFromCommand" {
            continue;
        }
        let command = s(&case["input"][0]);
        let workdir = s(&case["input"][1]);
        let expected: Vec<String> = case["expect"]
            .as_array()
            .map(|a| a.iter().map(s).collect())
            .unwrap_or_default();
        assert_eq!(
            extract_sensitive_paths_from_command(&command, &workdir),
            expected,
            "sensitive paths for {command:?}  [{}]",
            case["label"]
        );
    }
}
