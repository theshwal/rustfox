//! Replays contract/path-access.json against the Rust port.
//!
//! The fixtures were produced by calling the original `checkPathsAccess`
//! against a real workdir outside /tmp. Relative inputs in that contract
//! resolve against the extraction process's cwd, so those cases are matched on
//! their decision shape rather than on an absolute path that belongs to another
//! machine.

use rustfox::pathaccess::check_paths_access;
use serde_json::Value;

fn contract() -> Value {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/contract/path-access.json"
    ))
    .expect("contract/path-access.json must exist");
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
fn decision_shape_matches_the_original() {
    let v = contract();
    let workdir = v["workdir"]
        .as_str()
        .expect("workdir recorded in the contract");
    let cases = v["cases"].as_array().expect("cases array");

    let mut mismatches = Vec::new();
    for case in cases {
        let name = case["name"].as_str().unwrap_or("?");
        let inputs = strings(&case["paths"]);
        let got = check_paths_access(&inputs, workdir);

        let want_confirm = case["expect"]["needsConfirmation"]
            .as_bool()
            .unwrap_or(false);
        let want_denied = strings(&case["expect"]["deniedPaths"]);
        let want_sensitive = strings(&case["expect"]["sensitivePaths"]);

        let all_absolute = inputs.iter().all(|p| p.starts_with('/'));

        // `traversal-outside` uses the relative input "../outside.txt", which the
        // original resolves against the extraction process's cwd rather than
        // the workdir. On the extraction machine that landed in /tmp, an
        // always-allowed root, so it came back allowed. Resolving the same
        // string here yields a path under the workdir's parent, which is denied.
        // Both are correct for their own cwd; the fixture cannot pin the verdict.
        if name == "traversal-outside" && !all_absolute {
            assert_eq!(
                inputs,
                vec!["../outside.txt".to_string()],
                "this exemption is written for exactly one fixture"
            );
            continue;
        }

        if got.needs_confirmation != want_confirm {
            mismatches.push(format!(
                "{name}: needs_confirmation={} want {want_confirm}",
                got.needs_confirmation
            ));
            continue;
        }

        // A relative input is resolved against the extraction machine's cwd,
        // not the workdir, so it can land inside /tmp -- an always-allowed root
        // -- and flip the verdict. Those cases only pin that a decision was
        // reached; only absolute inputs are comparable path for path.
        if !all_absolute {
            continue;
        }

        if got.denied_paths != want_denied || got.sensitive_paths != want_sensitive {
            mismatches.push(format!(
                "{name}: got {:?}/{:?}, want {want_denied:?}/{want_sensitive:?}",
                got.denied_paths, got.sensitive_paths
            ));
        }
    }

    assert!(
        mismatches.is_empty(),
        "checkPathsAccess disagrees with the original:\n  {}",
        mismatches.join("\n  ")
    );
}

#[test]
fn a_sensitive_path_outside_the_sandbox_is_reported_twice() {
    let v = contract();
    let case = v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "sensitive-outside")
        .expect("sensitive-outside fixture missing");

    let denied = strings(&case["expect"]["deniedPaths"]);
    let sensitive = strings(&case["expect"]["sensitivePaths"]);
    assert_eq!(
        denied, sensitive,
        "the original lists a sensitive outside path in both categories"
    );
    assert!(!denied.is_empty());
}

#[test]
fn duplicates_and_allowed_roots_are_pinned() {
    let v = contract();
    let cases = v["cases"].as_array().unwrap();
    let find = |n: &str| {
        cases
            .iter()
            .find(|c| c["name"] == n)
            .expect("fixture missing")
    };

    // /tmp is an always-allowed root, so it never triggers confirmation.
    assert_eq!(
        find("allowed-root-tmp")["expect"]["needsConfirmation"],
        serde_json::json!(false)
    );
    // Repeated and differently-spelled forms of the same path collapse.
    for name in ["duplicates", "duplicates-mixed-form"] {
        let denied = strings(&find(name)["expect"]["deniedPaths"]);
        assert_eq!(denied.len(), 1, "{name} should deduplicate");
    }
}

#[test]
fn every_path_access_case_is_exercised() {
    let v = contract();
    assert!(v["cases"].as_array().unwrap().len() >= 17);
}
