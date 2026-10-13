//! Session path allowlist and the denial error, ported from
//! `src/server/tools/path-security.ts`.
//!
//! Pinned by contract/confirmation.json, captured by calling the original.

use std::collections::{HashMap, HashSet};

use crate::pathsec::normalize;

/// Why access was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenialReason {
    OutsideWorkdir,
    SensitiveFile,
    Both,
    DangerousCommand,
    GitNoVerify,
}

impl DenialReason {
    /// The human phrase the original embeds in its message.
    ///
    /// `dangerous_command` has no branch of its own and falls through to the
    /// `outside_workdir` wording; that is the original's behaviour and the
    /// contract pins it.
    pub fn reason_text(self) -> &'static str {
        match self {
            DenialReason::SensitiveFile => "sensitive files (may contain secrets)",
            DenialReason::Both => "paths outside workdir and sensitive files",
            DenialReason::GitNoVerify => "git commands with --no-verify",
            DenialReason::OutsideWorkdir | DenialReason::DangerousCommand => {
                "paths outside workdir"
            }
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            DenialReason::OutsideWorkdir => "outside_workdir",
            DenialReason::SensitiveFile => "sensitive_file",
            DenialReason::Both => "both",
            DenialReason::DangerousCommand => "dangerous_command",
            DenialReason::GitNoVerify => "git_no_verify",
        }
    }

    /// Parse a reason name; kept as an inherent method to mirror the
    /// original's string union without pulling in `FromStr`.
    #[allow(clippy::should_implement_trait)]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "outside_workdir" => DenialReason::OutsideWorkdir,
            "sensitive_file" => DenialReason::SensitiveFile,
            "both" => DenialReason::Both,
            "dangerous_command" => DenialReason::DangerousCommand,
            "git_no_verify" => DenialReason::GitNoVerify,
            _ => return None,
        })
    }
}

/// The error thrown when the user refuses access; it aborts the agent run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathAccessDenied {
    pub paths: Vec<String>,
    pub tool: String,
    pub reason: DenialReason,
    pub custom_message: Option<String>,
}

impl PathAccessDenied {
    /// Port of the `PathAccessDeniedError` constructor.
    ///
    /// A custom message replaces the generated one entirely, including the
    /// reason phrase and the path list.
    pub fn message(&self) -> String {
        match &self.custom_message {
            Some(m) => m.clone(),
            None => format!(
                "User denied access to {}: {}",
                self.reason.reason_text(),
                self.paths.join(", ")
            ),
        }
    }

    pub fn error_name(&self) -> &'static str {
        "PathAccessDeniedError"
    }
}

impl std::fmt::Display for PathAccessDenied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for PathAccessDenied {}

/// Per-session allowlist of paths the user has approved.
///
/// Paths are normalised on both insertion and lookup, so `/etc/./passwd` and
/// `/etc/shadow/../passwd` both match an entry for `/etc/passwd`. A trailing
/// slash does **not** match, because Node's `normalize` leaves it in place.
#[derive(Debug, Default)]
pub struct Allowlist {
    sessions: HashMap<String, HashSet<String>>,
}

impl Allowlist {
    pub fn new() -> Self {
        Allowlist::default()
    }

    /// Add one path, normalising it first.
    pub fn add(&mut self, session_id: &str, path: &str) {
        self.sessions
            .entry(session_id.to_string())
            .or_default()
            .insert(normalize(path));
    }

    /// Add several paths.
    pub fn add_all(&mut self, session_id: &str, paths: &[String]) {
        for p in paths {
            self.add(session_id, p);
        }
    }

    /// Whether the normalised path is allowed for this session.
    pub fn is_allowed(&self, session_id: &str, path: &str) -> bool {
        self.sessions
            .get(session_id)
            .map(|s| s.contains(&normalize(path)))
            .unwrap_or(false)
    }

    /// Drop a session's allowlist entirely, e.g. when the session is deleted.
    pub fn clear(&mut self, session_id: &str) {
        self.sessions.remove(session_id);
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    pub fn len_for(&self, session_id: &str) -> usize {
        self.sessions.get(session_id).map(|s| s.len()).unwrap_or(0)
    }
}

/// A pending confirmation awaiting the user's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingConfirmation {
    pub call_id: String,
    pub paths: Vec<String>,
    pub session_id: String,
    pub tool: String,
    pub workdir: String,
    pub reason: DenialReason,
}

/// Tracks confirmations so a session can be cancelled wholesale.
#[derive(Debug, Default)]
pub struct ConfirmationRegistry {
    pending: HashMap<String, PendingConfirmation>,
}

impl ConfirmationRegistry {
    pub fn new() -> Self {
        ConfirmationRegistry::default()
    }

    /// Register a pending confirmation; returns false if the id is taken, which
    /// the original also does by overwriting the map entry.
    pub fn register(&mut self, c: PendingConfirmation) -> bool {
        self.pending.insert(c.call_id.clone(), c).is_some()
    }

    /// Answer a pending confirmation. Returns the entry if one was waiting.
    pub fn provide(&mut self, call_id: &str, approved: bool) -> Option<PendingConfirmation> {
        let entry = self.pending.remove(call_id)?;
        Some(entry).inspect(|_e| {
            let _ = approved;
        })
    }

    pub fn get(&self, call_id: &str) -> Option<&PendingConfirmation> {
        self.pending.get(call_id)
    }

    /// Cancel one confirmation; false when the id is unknown.
    pub fn cancel(&mut self, call_id: &str) -> bool {
        self.pending.remove(call_id).is_some()
    }

    /// Cancel every confirmation for a session; returns how many were cancelled.
    pub fn cancel_for_session(&mut self, session_id: &str) -> usize {
        let to_remove: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, v)| v.session_id == session_id)
            .map(|(k, _)| k.clone())
            .collect();
        for k in &to_remove {
            self.pending.remove(k);
        }
        to_remove.len()
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_allowlist_allows_nothing() {
        let a = Allowlist::new();
        assert!(!a.is_allowed("s", "/etc/passwd"));
    }

    #[test]
    fn exact_matches_work() {
        let mut a = Allowlist::new();
        a.add("s", "/etc/passwd");
        assert!(a.is_allowed("s", "/etc/passwd"));
        assert!(!a.is_allowed("s", "/etc/shadow"));
    }

    #[test]
    fn dot_segments_are_normalised_on_both_sides() {
        let mut a = Allowlist::new();
        a.add("s", "/etc/passwd");
        // Both resolve to /etc/passwd, so both are allowed.
        assert!(a.is_allowed("s", "/etc/./passwd"));
        assert!(a.is_allowed("s", "/etc/shadow/../passwd"));
    }

    #[test]
    fn a_trailing_slash_does_not_match() {
        let mut a = Allowlist::new();
        a.add("s", "/etc/passwd");
        assert!(
            !a.is_allowed("s", "/etc/passwd/"),
            "Node's normalize keeps the trailing slash, so it does not match"
        );
    }

    #[test]
    fn clearing_removes_the_whole_session() {
        let mut a = Allowlist::new();
        a.add("s", "/etc/passwd");
        assert!(a.is_allowed("s", "/etc/passwd"));
        a.clear("s");
        assert!(!a.is_allowed("s", "/etc/passwd"));
    }

    #[test]
    fn sessions_are_independent() {
        let mut a = Allowlist::new();
        a.add("one", "/a");
        assert!(a.is_allowed("one", "/a"));
        assert!(!a.is_allowed("two", "/a"));
    }

    #[test]
    fn denial_messages_match_the_original() {
        let cases = [
            (DenialReason::OutsideWorkdir, "paths outside workdir"),
            (
                DenialReason::SensitiveFile,
                "sensitive files (may contain secrets)",
            ),
            (
                DenialReason::Both,
                "paths outside workdir and sensitive files",
            ),
            (DenialReason::GitNoVerify, "git commands with --no-verify"),
        ];
        for (reason, text) in cases {
            let e = PathAccessDenied {
                paths: vec!["/etc/passwd".into(), "/root/.env".into()],
                tool: "read_file".into(),
                reason,
                custom_message: None,
            };
            assert_eq!(
                e.message(),
                format!("User denied access to {text}: /etc/passwd, /root/.env")
            );
        }
    }

    #[test]
    fn dangerous_command_falls_back_to_the_outside_workdir_wording() {
        // The original has no branch for dangerous_command in its message.
        let e = PathAccessDenied {
            paths: vec!["/etc/passwd".into()],
            tool: "run_command".into(),
            reason: DenialReason::DangerousCommand,
            custom_message: None,
        };
        assert_eq!(
            e.message(),
            "User denied access to paths outside workdir: /etc/passwd"
        );
    }

    #[test]
    fn a_custom_message_replaces_everything() {
        let e = PathAccessDenied {
            paths: vec!["/a".into()],
            tool: "write_file".into(),
            reason: DenialReason::SensitiveFile,
            custom_message: Some("my own message".into()),
        };
        assert_eq!(e.message(), "my own message");
    }

    #[test]
    fn cancelling_reports_what_it_found() {
        let mut r = ConfirmationRegistry::new();
        assert!(!r.cancel("no-such-call"));
        assert_eq!(r.cancel_for_session("no-such-session"), 0);

        for i in 0..3 {
            r.register(PendingConfirmation {
                call_id: format!("c{i}"),
                paths: vec!["/a".into()],
                session_id: "s1".into(),
                tool: "read_file".into(),
                workdir: "/w".into(),
                reason: DenialReason::OutsideWorkdir,
            });
        }
        r.register(PendingConfirmation {
            call_id: "other".to_string(),
            paths: vec!["/b".into()],
            session_id: "s2".into(),
            tool: "read_file".into(),
            workdir: "/w".into(),
            reason: DenialReason::OutsideWorkdir,
        });

        assert_eq!(r.pending_count(), 4);
        assert_eq!(r.cancel_for_session("s1"), 3);
        assert_eq!(r.pending_count(), 1, "another session is untouched");
    }
}
