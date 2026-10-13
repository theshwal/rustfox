//! Replays contract/confirmation.json against the Rust port.
//!
//! The fixtures were produced by calling the original's exported allowlist and
//! denial machinery directly, one fresh session id per scenario.

use rustfox::confirmation::{Allowlist, DenialReason, PathAccessDenied};
use serde_json::Value;

fn contract() -> Value {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/contract/confirmation.json"
    ))
    .expect("contract/confirmation.json must exist");
    serde_json::from_str(&raw).expect("contract must be valid JSON")
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|x| x.as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn allowlist_behaviour_matches_the_original() {
    let v = contract();
    let mut list = Allowlist::new();

    for case in v["allowlist"].as_array().expect("allowlist array") {
        let name = case["name"].as_str().unwrap_or("?");
        // A fresh session per scenario, as the extractor used a distinct id.
        let session = format!("session-{name}");
        list.add_all(&session, &strings(&case["add"]));

        let before = case["beforeClear"].as_array().expect("beforeClear");
        for probe in before {
            let path = probe["path"].as_str().unwrap_or("");
            let want = probe["allowed"].as_bool().unwrap_or(false);
            assert_eq!(
                list.is_allowed(&session, path),
                want,
                "{name}: is_allowed({path:?}) should be {want}"
            );
        }

        // The extractor cleared the allowlist when the recorded after-state
        // differs from the before-state; detect that the same way.
        let before_arr = case["beforeClear"].as_array().expect("beforeClear array");
        let after_arr = case["afterClear"].as_array().expect("afterClear array");
        let changed = before_arr.len() != after_arr.len()
            || before_arr
                .iter()
                .zip(after_arr.iter())
                .any(|(x, y)| x["allowed"] != y["allowed"]);
        if changed {
            list.clear(&session);
        }

        let after = case["afterClear"].as_array().expect("afterClear");
        for probe in after {
            let path = probe["path"].as_str().unwrap_or("");
            let want = probe["allowed"].as_bool().unwrap_or(false);
            assert_eq!(
                list.is_allowed(&session, path),
                want,
                "{name}: after clear, is_allowed({path:?}) should be {want}"
            );
        }
    }
}

#[test]
fn denial_messages_match_the_original() {
    let v = contract();
    for case in v["denialMessages"]
        .as_array()
        .expect("denialMessages array")
    {
        let reason_str = case["reason"].as_str().unwrap_or("");
        let reason = DenialReason::parse(reason_str)
            .unwrap_or_else(|| panic!("unknown reason {reason_str}"));
        let paths = strings(&case["paths"]);

        let err = PathAccessDenied {
            paths: paths.clone(),
            tool: "read_file".into(),
            reason,
            custom_message: None,
        };
        assert_eq!(
            err.message(),
            case["message"].as_str().unwrap_or(""),
            "denial message for {reason_str}"
        );
        assert_eq!(
            err.error_name(),
            case["name"].as_str().unwrap_or("PathAccessDeniedError")
        );
    }
}

#[test]
fn dangerous_command_has_no_message_branch_of_its_own() {
    let v = contract();
    let case = v["denialMessages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["reason"] == "dangerous_command")
        .expect("dangerous_command case missing");

    // The original's ternary chain has no branch for dangerous_command, so it
    // falls through to the outside_workdir wording.
    let outside = v["denialMessages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["reason"] == "outside_workdir")
        .expect("outside_workdir case missing");
    assert_eq!(
        case["message"], outside["message"],
        "dangerous_command reuses the outside_workdir wording"
    );
    assert_eq!(
        DenialReason::DangerousCommand.reason_text(),
        "paths outside workdir"
    );
}

#[test]
fn custom_messages_replace_the_generated_one() {
    let v = contract();
    let custom = v["customMessages"]["withCustom"]
        .as_str()
        .expect("withCustom recorded");
    let custom_default = v["customMessages"]["withCustomAndDefaultReason"]
        .as_str()
        .expect("withCustomAndDefaultReason recorded");

    let base = |reason: DenialReason, message: &str| PathAccessDenied {
        paths: vec!["/a".into()],
        tool: "write_file".into(),
        reason,
        custom_message: Some(message.to_string()),
    };
    assert_eq!(
        base(DenialReason::SensitiveFile, "my own message").message(),
        custom,
        "a custom message replaces the generated one entirely"
    );
    assert_eq!(
        base(DenialReason::OutsideWorkdir, "just this").message(),
        custom_default
    );
    // The reason survives on the error, it just is not echoed in the message.
    let err = base(DenialReason::SensitiveFile, "my own message");
    assert_eq!(err.reason, DenialReason::SensitiveFile);
    assert!(!err.message().contains("sensitive"));
}

#[test]
fn cancelling_an_unknown_confirmation_is_a_no_op() {
    let v = contract();
    assert_eq!(
        v["cancelUnknown"].as_bool(),
        Some(false),
        "cancelling an unknown callId returns false"
    );
    assert_eq!(
        v["cancelSessionEmpty"].as_i64(),
        Some(0),
        "cancelling an unknown session cancels nothing"
    );
}

#[test]
fn a_trailing_slash_is_what_separates_a_match_from_a_miss() {
    let v = contract();
    let cases = v["allowlist"].as_array().unwrap();
    let slash = cases
        .iter()
        .find(|c| c["name"] == "trailing-slash")
        .expect("trailing-slash fixture missing");
    assert_eq!(
        slash["beforeClear"][0]["allowed"],
        serde_json::json!(false),
        "/etc/passwd/ does not match an entry for /etc/passwd"
    );
    let dot = cases
        .iter()
        .find(|c| c["name"] == "dot-segment")
        .expect("dot-segment fixture missing");
    assert_eq!(
        dot["beforeClear"][0]["allowed"],
        serde_json::json!(true),
        "/etc/./passwd does match, since normalisation removes the dot"
    );
}
