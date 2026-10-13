//! Path access aggregation, ported from `checkPathsAccess` in
//! `src/server/tools/path-security.ts`.
//!
//! This is the single gate read, write, edit and shell funnel through, so its
//! aggregation rules are load-bearing. Pinned by contract/path-access.json.

use std::collections::HashSet;

use crate::pathsec::{is_path_within_sandbox, is_sensitive_path};

/// One path's decision, before aggregation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathDecision {
    /// False when the path resolves outside the workdir.
    pub allowed: bool,
    /// The resolved path, which is what gets reported.
    pub resolved_path: String,
    /// True when the basename matches the sensitive-file rules.
    pub sensitive: bool,
}

/// The result of `checkPathsAccess`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathAccessResult {
    pub needs_confirmation: bool,
    /// Paths outside the sandbox, deduplicated, in first-seen order.
    pub denied_paths: Vec<String>,
    /// Sensitive paths, deduplicated, in first-seen order.
    pub sensitive_paths: Vec<String>,
}

impl PathAccessResult {
    fn empty() -> Self {
        PathAccessResult {
            needs_confirmation: false,
            denied_paths: Vec::new(),
            sensitive_paths: Vec::new(),
        }
    }
}

/// Port of `checkPathsAccess`.
///
/// Two subtleties, both captured from the original:
///
/// - a path can be **both** denied and sensitive, and then appears in both
///   lists; `needsConfirmation` only asks whether either list is non-empty
/// - a sensitive check runs regardless of the sandbox verdict, so a sensitive
///   file inside the workdir still triggers confirmation
///
/// Deduplication preserves first-seen order, as the original's `[...new Set()]`
/// does.
pub fn check_paths_access(paths: &[String], workdir: &str) -> PathAccessResult {
    if paths.is_empty() {
        return PathAccessResult::empty();
    }

    let mut denied: Vec<String> = Vec::new();
    let mut sensitive: Vec<String> = Vec::new();

    for path in paths {
        let verdict = is_path_within_sandbox(path, workdir);
        let decision = PathDecision {
            allowed: verdict.allowed,
            resolved_path: verdict.resolved_path,
            sensitive: is_sensitive_path(path),
        };

        if !decision.allowed && !denied.contains(&decision.resolved_path) {
            denied.push(decision.resolved_path.clone());
        }

        // Sensitivity is checked independently of the sandbox verdict.
        if decision.sensitive && !sensitive.contains(&decision.resolved_path) {
            sensitive.push(decision.resolved_path.clone());
        }
    }

    PathAccessResult {
        needs_confirmation: !denied.is_empty() || !sensitive.is_empty(),
        denied_paths: denied,
        sensitive_paths: sensitive,
    }
}

/// Whether a command touches any path that would need confirmation.
///
/// Combines the path scan with the dangerous-command patterns, which is how the
/// shell tool decides between running and asking.
pub fn command_needs_confirmation(command: &str, workdir: &str) -> (bool, Vec<String>) {
    use crate::pathsec::{extract_dangerous_patterns, extract_sensitive_paths_from_command};

    let mut reasons = Vec::new();

    let dangerous = extract_dangerous_patterns(command);
    if !dangerous.is_empty() {
        reasons.push(format!("dangerous_command:{}", dangerous.join(",")));
    }

    for candidate in extract_path_candidates(command) {
        let result = check_paths_access(std::slice::from_ref(&candidate), workdir);
        for p in result.denied_paths {
            reasons.push(format!("outside_workdir:{p}"));
        }
        for p in result.sensitive_paths {
            reasons.push(format!("sensitive_file:{p}"));
        }
    }

    for p in extract_sensitive_paths_from_command(command, workdir) {
        reasons.push(format!("sensitive_file:{p}"));
    }

    reasons.dedup();
    (!reasons.is_empty(), reasons)
}

/// Path-like tokens in a command, which `checkPaths_access` then judges.
fn extract_path_candidates(command: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for token in command.split_whitespace() {
        let cleaned =
            token.trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | '(' | ')' | ',' | ';'));
        if cleaned.is_empty() {
            continue;
        }
        if (cleaned.starts_with('/') || cleaned.starts_with('.'))
            && !out.iter().any(|p| p == cleaned)
        {
            out.push(cleaned.to_string());
        }
    }
    out
}

/// Convenience: the unique set of denied paths across several calls, which is
/// how a caller batches results.
pub fn unique_denied(results: &[PathAccessResult]) -> HashSet<String> {
    results
        .iter()
        .flat_map(|r| r.denied_paths.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn empty_input_needs_no_confirmation() {
        let r = check_paths_access(&[], "/home/u/project");
        assert!(!r.needs_confirmation);
        assert!(r.denied_paths.is_empty());
        assert!(r.sensitive_paths.is_empty());
    }

    #[test]
    fn an_inside_path_passes_silently() {
        let r = check_paths_access(&paths(&["/home/u/project/src/index.ts"]), "/home/u/project");
        assert!(!r.needs_confirmation);
        assert!(r.denied_paths.is_empty());
    }

    #[test]
    fn a_sensitive_file_inside_the_workdir_still_asks() {
        let r = check_paths_access(&paths(&["/home/u/project/.env"]), "/home/u/project");
        assert!(r.needs_confirmation);
        assert!(r.denied_paths.is_empty(), "it is inside the sandbox");
        assert_eq!(r.sensitive_paths, vec!["/home/u/project/.env".to_string()]);
    }

    #[test]
    fn a_sensitive_file_outside_appears_in_both_lists() {
        let r = check_paths_access(&paths(&["/root/.env"]), "/home/u/project");
        assert!(r.needs_confirmation);
        assert_eq!(r.denied_paths, vec!["/root/.env".to_string()]);
        assert_eq!(
            r.sensitive_paths,
            vec!["/root/.env".to_string()],
            "a path can be denied and sensitive at once"
        );
    }

    #[test]
    fn duplicates_are_collapsed() {
        let r = check_paths_access(
            &paths(&["/etc/passwd", "/etc/passwd", "/etc/../etc/passwd"]),
            "/home/u/project",
        );
        assert_eq!(r.denied_paths, vec!["/etc/passwd".to_string()]);
    }

    #[test]
    fn an_allowed_root_never_asks() {
        let r = check_paths_access(&paths(&["/tmp/whatever"]), "/home/u/project");
        assert!(!r.needs_confirmation);
        assert!(r.denied_paths.is_empty());
    }

    #[test]
    fn dev_null_is_denied_despite_being_a_device() {
        // /dev/null is listed as a safe path in the source, but the sandbox
        // check is what produces deniedPaths and it does not exempt it.
        let r = check_paths_access(&paths(&["/dev/null"]), "/home/u/project");
        assert!(r.needs_confirmation);
        assert_eq!(r.denied_paths, vec!["/dev/null".to_string()]);
    }

    #[test]
    fn mixed_input_reports_each_category_once() {
        let r = check_paths_access(
            &paths(&[
                "/home/u/project/src/index.ts",
                "/home/u/project/config.json",
                "/etc/shadow",
                "/home/u/project/.env",
            ]),
            "/home/u/project",
        );
        assert!(r.needs_confirmation);
        assert_eq!(r.denied_paths, vec!["/etc/shadow".to_string()]);
        assert_eq!(r.sensitive_paths, vec!["/home/u/project/.env".to_string()]);
    }

    #[test]
    fn sibling_of_the_workdir_is_denied() {
        let r = check_paths_access(&paths(&["/home/u/sibling.txt"]), "/home/u/project");
        assert_eq!(r.denied_paths, vec!["/home/u/sibling.txt".to_string()]);
    }
}
