//! Path-security rules ported from `src/server/tools/path-security.ts`.
//!
//! Every behaviour here is pinned by contract/path-rules.json, which was
//! produced by calling the original implementation. The tests in
//! `tests/path_rules.rs` replay those fixtures; if this module and the original
//! ever disagree, the tests fail.

use std::path::{Component, Path, PathBuf};

/// Matches the original's SENSITIVE_FILE_PATTERNS, applied to the basename.
///
/// The original's regexes are:
///   ^\.env$
///   ^\.env\.(?!example)[a-zA-Z0-9_.-]+$
///   ^credentials\.json$          (case-insensitive)
///   ^secrets?\.(json|ya?ml|toml)$ (case-insensitive)
///   \.pem$                        (case-insensitive)
///   \.key$                        (case-insensitive)
///   ^id_(rsa|ed25519|ecdsa|dsa)
///   ^\.netrc$
pub fn is_sensitive_path(path: &str) -> bool {
    let name = basename(path);
    if name.is_empty() {
        return false;
    }
    let lower = name.to_ascii_lowercase();

    // ^\.env$
    if name == ".env" {
        return true;
    }

    // ^\.env\.(?!example)[a-zA-Z0-9_.-]+$
    if let Some(rest) = name.strip_prefix(".env.") {
        if !rest.is_empty()
            && !name.starts_with(".env.example")
            && rest
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
        {
            return true;
        }
    }

    // ^credentials\.json$
    if lower == "credentials.json" {
        return true;
    }

    // ^secrets?\.(json|ya?ml|toml)$
    for prefix in ["secret", "secrets"] {
        for ext in ["json", "yaml", "yml", "toml"] {
            if lower == format!("{prefix}.{ext}") {
                return true;
            }
        }
    }

    // \.pem$ / \.key$
    if lower.ends_with(".pem") || lower.ends_with(".key") {
        return true;
    }

    // ^id_rsa / ^id_ed25519 / ^id_ecdsa / ^id_dsa
    for prefix in ["id_rsa", "id_ed25519", "id_ecdsa", "id_dsa"] {
        if name.starts_with(prefix) {
            return true;
        }
    }

    // ^\.netrc$
    if name == ".netrc" {
        return true;
    }

    false
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// ALLOWED_ROOTS from the original: /tmp and /var/tmp are always permitted,
/// in addition to the workdir.
fn is_allowed_root(path: &str) -> bool {
    path.starts_with("/tmp") || path.starts_with("/var/tmp")
}

/// Lexical normalisation matching Node's `path.normalize`: resolve `.` and `..`
/// textually, collapsing separators, without touching the filesystem.
pub fn normalize(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let absolute = path.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if let Some(last) = out.last() {
                    if *last != ".." {
                        out.pop();
                        continue;
                    }
                }
                if !absolute {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    let joined = out.join("/");
    if absolute {
        format!("/{joined}")
    } else if joined.is_empty() {
        ".".to_string()
    } else {
        joined
    }
}

/// Join `base` with a possibly-relative `path` and normalise the result.
pub fn resolve_against(base: &str, path: &str) -> String {
    if path.starts_with('/') {
        normalize(path)
    } else {
        normalize(&format!("{}/{}", base.trim_end_matches('/'), path))
    }
}

/// Port of `isPathWithinSandbox` for the no-symlink case.
///
/// The original resolves symlinks via `realpath` first; this port is lexical,
/// so it agrees with the original whenever the path contains no symlink, which
/// is what the contract fixtures cover.
pub struct SandboxVerdict {
    pub allowed: bool,
    pub resolved_path: String,
}

pub fn is_path_within_sandbox(path: &str, workdir: &str) -> SandboxVerdict {
    let resolved = resolve_against(workdir, path);
    let normalized_workdir = normalize(workdir.trim_end_matches('/'));

    let inside =
        resolved == normalized_workdir || resolved.starts_with(&format!("{normalized_workdir}/"));
    let allowed = inside || is_allowed_root(&resolved);

    SandboxVerdict {
        allowed,
        resolved_path: resolved,
    }
}

/// TRAVERSAL_SEGMENT_RE: a `..` as a whole path segment.
fn has_traversal_segment(word: &str) -> bool {
    if word.starts_with('/') || word.starts_with('~') {
        return false;
    }
    word == ".." || word.starts_with("../") || word.ends_with("/..") || word.contains("/../")
}

/// Shell keywords that can follow `cd`/`pushd` without being a directory.
const CD_NON_TARGET_KEYWORDS: [&str; 9] = [
    "cd", "pushd", "popd", "echo", "exit", "return", "true", "false", ":",
];

/// Port of `resolveRelativeTraversals`: report the paths a command reaches by
/// walking out of the workdir, tracking `cd` so the cwd follows the command.
///
/// Only `..` segments count, which is why `ls ./src` produces nothing while
/// `cat ../../etc/passwd` resolves against the full path.
pub fn resolve_relative_traversals(command: &str, workdir: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cwd = normalize(workdir);
    let mut expect_cd_target = false;

    for raw in command.split_whitespace() {
        let word =
            raw.trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | '(' | ')' | ',' | ';'));
        if word.is_empty() {
            continue;
        }
        let is_traversal = has_traversal_segment(word);

        if expect_cd_target {
            expect_cd_target = false;
            if !CD_NON_TARGET_KEYWORDS.contains(&word) {
                if is_traversal {
                    let resolved = resolve_against(&cwd, word);
                    if !out.contains(&resolved) {
                        out.push(resolved);
                    }
                }
                cwd = resolve_against(&cwd, word);
            }
        } else if word == "cd" || word == "pushd" {
            expect_cd_target = true;
        } else if is_traversal {
            let resolved = resolve_against(&cwd, word);
            if !out.contains(&resolved) {
                out.push(resolved);
            }
        }
    }
    out
}

/// DANGEROUS_PATTERNS from the original, in source order.
///
/// Note `rm -rf /` is deliberately absent: the original only matches `rm -rf ~`.
/// That asymmetry was captured from the running code and is asserted by the
/// contract tests.
const DANGEROUS_PATTERNS: &[&str] = &[
    r"sudo\s",
    r"rm\s+(-rf?|--recursive)\s+[~]",
    r"chmod\s+777",
    r">\s*/dev/sd",
    r"mkfs\s",
    r"dd\s+if=",
    r":\(\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;",
];

/// Port of `extractDangerousPatterns`. The Windows-only patterns are omitted:
/// they must not run on Unix, where they would flag innocent searches.
pub fn extract_dangerous_patterns(command: &str) -> Vec<&'static str> {
    DANGEROUS_PATTERNS
        .iter()
        .copied()
        .filter(|p| regex_match(p, command))
        .collect()
}

/// Git subcommands where `-n` is the documented shorthand for `--no-verify`.
const GIT_N_NO_VERIFY_SUBCOMMANDS: [&str; 2] = ["commit", "am"];

/// Shell operators that separate one command from the next.
const COMMAND_SEPARATORS: [&str; 4] = [";", "&&", "||", "|"];

/// True when a quote opens without closing (escapes honoured).
fn has_dangling_quote(command: &str) -> bool {
    let mut quote: Option<char> = None;
    let mut chars = command.chars();
    while let Some(ch) = chars.next() {
        if let Some(q) = quote {
            if ch == '\\' && q != '\'' {
                chars.next();
                continue;
            }
            if ch == q {
                quote = None;
            }
            continue;
        }
        if ch == '\'' || ch == '"' || ch == '`' {
            quote = Some(ch);
        }
    }
    quote.is_some()
}

/// Split a command into segments on the shell operators the original uses.
fn split_segments(command: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut chars = command.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '&' && chars.peek() == Some(&'&') {
            chars.next();
            segments.push(std::mem::take(&mut current));
            continue;
        }
        if ch == '|' && chars.peek() == Some(&'|') {
            chars.next();
            segments.push(std::mem::take(&mut current));
            continue;
        }
        if COMMAND_SEPARATORS.contains(&ch.to_string().as_str()) {
            segments.push(std::mem::take(&mut current));
            continue;
        }
        current.push(ch);
    }
    segments.push(current);
    segments
}

/// Port of `gitSubHasNoVerify` + `extractGitNoVerify`.
///
/// `-n` is only `--no-verify` for `commit`/`am`; elsewhere git gives it a
/// harmless meaning, so flagging it there is a false positive.
pub fn extract_git_no_verify(command: &str) -> bool {
    if has_dangling_quote(command) {
        return false;
    }
    for segment in split_segments(command) {
        let sub: Vec<&str> = segment.split_whitespace().collect();
        let Some(git_index) = sub.iter().position(|t| *t == "git") else {
            continue;
        };
        let Some(sub_cmd) = sub.get(git_index + 1) else {
            continue;
        };
        if sub_cmd.starts_with('-') {
            continue;
        }
        let args = &sub[git_index + 2..];
        if args.contains(&"--no-verify") {
            return true;
        }
        if GIT_N_NO_VERIFY_SUBCOMMANDS.contains(sub_cmd) && args.contains(&"-n") {
            return true;
        }
    }
    false
}

/// Port of `extractSensitivePathsFromCommand`: tokens in the command whose
/// basename matches the sensitive-file rules.
///
/// The original returns the token exactly as written — `cat .env` yields
/// `.env`, not the absolute path — so this port does not resolve against the
/// workdir.
pub fn extract_sensitive_paths_from_command(command: &str, _workdir: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for token in command.split_whitespace() {
        let cleaned =
            token.trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | '(' | ')' | ',' | ';'));
        if cleaned.is_empty() {
            continue;
        }
        if is_sensitive_path(basename(cleaned)) && !out.iter().any(|o| o == cleaned) {
            out.push(cleaned.to_string());
        }
    }
    out
}

/// Index of the `)` closing the `(` at `open`.
fn matching_paren(p: &[char], open: usize) -> usize {
    let mut depth = 0;
    let mut i = open;
    while i < p.len() {
        match p[i] {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
        i += 1;
    }
    p.len()
}

/// Split a pattern fragment on top-level `|`, ignoring `|` inside groups.
fn split_alternatives(p: &[char]) -> Vec<Vec<char>> {
    let mut out = Vec::new();
    let mut current = Vec::new();
    let mut depth = 0;
    let mut i = 0;
    while i < p.len() {
        match p[i] {
            '(' => {
                depth += 1;
                current.push(p[i]);
            }
            ')' => {
                depth -= 1;
                current.push(p[i]);
            }
            '|' if depth == 0 => {
                out.push(std::mem::take(&mut current));
            }
            '\\' if i + 1 < p.len() => {
                current.push(p[i]);
                current.push(p[i + 1]);
                i += 1;
            }
            c => current.push(c),
        }
        i += 1;
    }
    out.push(current);
    out
}

/// A deliberately small regex subset: literals, `\s`, `\d`, character classes,
/// alternation, groups and `+` repetition. This covers the original's patterns
/// without pulling in a regex crate, and keeps the port auditable line by line.
fn regex_match(pattern: &str, text: &str) -> bool {
    let chars: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();

    fn match_here(p: &[char], pi: usize, t: &[char], ti: usize) -> bool {
        if pi >= p.len() {
            return true;
        }
        // `x+` / `x*` on an escaped atom (e.g. \s+, \s*) means repetition. This
        // must be checked before the single-atom case below, which would consume
        // the atom without honouring the quantifier.
        if p[pi] == '\\' && pi + 2 < p.len() && (p[pi + 2] == '+' || p[pi + 2] == '*') {
            let quant = p[pi + 2];
            let atom = p[pi + 1];
            let matches = |c: char| match atom {
                's' => c.is_whitespace(),
                'd' => c.is_ascii_digit(),
                other => c == other,
            };
            let mut n = ti;
            while n < t.len() && matches(t[n]) {
                n += 1;
            }
            let starts: Box<dyn Iterator<Item = usize>> = if quant == '+' {
                Box::new((ti + 1..=n).rev())
            } else {
                Box::new((ti..=n).rev())
            };
            for end in starts {
                if match_here(p, pi + 3, t, end) {
                    return true;
                }
            }
            return false;
        }
        // Alternation is only used at the top level here.
        if p[pi] == '\\' && pi + 1 < p.len() {
            let atom = match p[pi + 1] {
                's' => {
                    if ti < t.len() && t[ti].is_whitespace() {
                        // \s matches exactly one whitespace char.
                        match_here(p, pi + 2, t, ti + 1)
                    } else {
                        false
                    }
                }
                'd' => {
                    if ti < t.len() && t[ti].is_ascii_digit() {
                        match_here(p, pi + 2, t, ti + 1)
                    } else {
                        false
                    }
                }
                other => {
                    if ti < t.len() && t[ti] == other {
                        match_here(p, pi + 2, t, ti + 1)
                    } else {
                        false
                    }
                }
            };
            return atom;
        }
        if p[pi] == '(' {
            let close = matching_paren(p, pi);
            // Branch alternatives inside the group, skipping a leading `?:`.
            let mut body_start = pi + 1;
            if p.get(body_start) == Some(&'?') && p.get(body_start + 1) == Some(&':') {
                body_start += 2;
            }
            let group_body: Vec<char> = p[body_start..close].to_vec();
            let after: Vec<char> = p[close + 1..].to_vec();
            let quantified = after.first() == Some(&'+');

            for branch in split_alternatives(&group_body) {
                let mut next: Vec<char> = branch.clone();
                next.extend_from_slice(&after[if quantified { 1 } else { 0 }..]);
                if match_here(&next, 0, t, ti) {
                    return true;
                }
                if quantified {
                    // One repetition is enough for the patterns in use, but try
                    // two so `(?:x)+` against "xx" also matches.
                    let mut twice: Vec<char> = branch.clone();
                    twice.extend_from_slice(&branch);
                    twice.extend_from_slice(&after[1..]);
                    if match_here(&twice, 0, t, ti) {
                        return true;
                    }
                }
            }
            return false;
        }
        if p[pi] == '[' {
            // Parse the character class.
            let mut j = pi + 1;
            let negated = j < p.len() && p[j] == '^';
            if negated {
                j += 1;
            }
            let mut set = Vec::new();
            let mut first = true;
            while j < p.len() && (p[j] != ']' || first) {
                set.push(p[j]);
                first = false;
                j += 1;
            }
            let end = if j < p.len() { j + 1 } else { j };
            if ti < t.len() {
                let hit = set.contains(&t[ti]);
                if hit != negated {
                    return match_here(p, end, t, ti + 1);
                }
            }
            return false;
        }
        if ti < t.len() && p[pi] == t[ti] {
            // `?` makes the preceding element optional.
            if p.get(pi + 1) == Some(&'?') {
                return match_here(p, pi + 2, t, ti + 1) || match_here(p, pi + 2, t, ti);
            }
            // `+` one-or-more, `*` zero-or-more, of this single character.
            if matches!(p.get(pi + 1), Some(&'+') | Some(&'*')) {
                let quant = p[pi + 1];
                let mut n = ti;
                while n < t.len() && t[n] == t[ti] {
                    n += 1;
                }
                if quant == '+' {
                    for end in (ti + 1..=n).rev() {
                        if match_here(p, pi + 2, t, end) {
                            return true;
                        }
                    }
                    return false;
                }
                for end in (ti..=n).rev() {
                    if match_here(p, pi + 2, t, end) {
                        return true;
                    }
                }
                return false;
            }
            return match_here(p, pi + 1, t, ti + 1);
        }
        // `*` on a character class.
        if p[pi] == '[' {
            let mut j = pi + 1;
            let negated = j < p.len() && p[j] == '^';
            if negated {
                j += 1;
            }
            let mut set = Vec::new();
            let mut first = true;
            while j < p.len() && (p[j] != ']' || first) {
                set.push(p[j]);
                first = false;
                j += 1;
            }
            let class_end = if j < p.len() { j + 1 } else { j };
            let in_set = |c: char| set.contains(&c) != negated;
            if p.get(class_end) == Some(&'+') {
                let mut n = ti;
                while n < t.len() && in_set(t[n]) {
                    n += 1;
                }
                for end in (ti + 1..=n).rev() {
                    if match_here(p, class_end + 1, t, end) {
                        return true;
                    }
                }
                return false;
            }
            if p.get(class_end) == Some(&'*') {
                let mut n = ti;
                while n < t.len() && in_set(t[n]) {
                    n += 1;
                }
                for end in (ti..=n).rev() {
                    if match_here(p, class_end + 1, t, end) {
                        return true;
                    }
                }
                return false;
            }
            if ti < t.len() && in_set(t[ti]) {
                return match_here(p, class_end, t, ti + 1);
            }
            return false;
        }
        false
    }

    // Top-level alternation: split on `|` outside groups and brackets, then
    // look for a match at every start offset, since `pattern.test(cmd)` is a
    // search and not an anchored match.
    split_alternatives(&chars)
        .into_iter()
        .any(|alt| (0..=text.len()).any(|start| match_here(&alt, 0, &text, start)))
}

/// Normalise a `Path` lexically, used by callers holding real filesystem paths.
pub fn normalize_path(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_patterns_match_the_original() {
        for name in [
            ".env",
            ".env.local",
            ".env.production",
            "credentials.json",
            "secrets.json",
            "secret.yaml",
            "secrets.toml",
            "server.pem",
            "private.key",
            "id_rsa",
            "id_ed25519",
            ".netrc",
        ] {
            assert!(is_sensitive_path(name), "{name} should be sensitive");
        }
    }

    #[test]
    fn envrc_and_env_example_are_not_sensitive() {
        // Both exclusions are explicit in the original's regex list.
        assert!(!is_sensitive_path(".envrc"));
        assert!(!is_sensitive_path(".env.example"));
    }

    #[test]
    fn ordinary_files_are_not_sensitive() {
        for name in ["package.json", "index.ts", "README.md", "env.json"] {
            assert!(!is_sensitive_path(name), "{name} should not be sensitive");
        }
    }

    #[test]
    fn regex_engine_handles_the_original_pattern_shapes() {
        // Group with alternation followed by more pattern.
        assert!(regex_match(r"rm\s+(-rf?|--recursive)\s+[~]", "rm -rf ~"));
        assert!(regex_match(
            r"rm\s+(-rf?|--recursive)\s+[~]",
            "rm --recursive ~"
        ));
        assert!(!regex_match(r"rm\s+(-rf?|--recursive)\s+[~]", "rm -r ~"));
        // Literal + escape.
        assert!(regex_match(r"sudo\s", "sudo rm"));
        assert!(!regex_match(r"sudo\s", "pseudo"));
        // Character class.
        // Character class and zero-or-more on an escape.
        assert!(regex_match(r"\s*/dev", " /dev"));
        assert!(regex_match(r">\s*/dev/sd", "dd > /dev/sda"));
        assert!(regex_match(r">\s*/dev/sd", "dd >/dev/sda"));
        // Non-capturing group with +.
        assert!(regex_match(
            r":\(\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;",
            ":(){ :|:&};"
        ));
    }

    #[test]
    fn dangerous_patterns_follow_the_original_asymmetry() {
        // `rm -rf ~` is matched ...
        assert!(
            !extract_dangerous_patterns("rm -rf ~").is_empty(),
            "rm -rf ~ must be flagged, got {:?}",
            extract_dangerous_patterns("rm -rf ~")
        );
        // ... while `rm -rf /` is not, matching the original's pattern list.
        assert!(extract_dangerous_patterns("rm -rf /").is_empty());
        assert!(!extract_dangerous_patterns("sudo rm file").is_empty());
        assert!(!extract_dangerous_patterns("dd if=/dev/zero of=/dev/sda").is_empty());
        assert!(extract_dangerous_patterns("echo hello").is_empty());
    }

    #[test]
    fn git_no_verify_detection() {
        assert!(extract_git_no_verify("git commit --no-verify"));
        assert!(extract_git_no_verify("git commit -n"));
        assert!(!extract_git_no_verify("git status"));
        assert!(!extract_git_no_verify("git push"));
    }

    #[test]
    fn traversal_resolution_normalises_dotdot() {
        // The original resolves `..` against the full path.
        assert_eq!(
            resolve_relative_traversals("cat ../../etc/passwd", "/home/user/project"),
            vec!["/home/etc/passwd".to_string()]
        );
        assert_eq!(
            resolve_relative_traversals("cd ../sibling", "/home/user/project"),
            vec!["/home/user/sibling".to_string()]
        );
        assert!(resolve_relative_traversals("ls ./src", "/home/user/project").is_empty());
        assert!(resolve_relative_traversals("no paths here", "/home/user/project").is_empty());
    }

    #[test]
    fn sandbox_decision_matches_the_original() {
        let wd = "/home/user/project";
        assert!(is_path_within_sandbox("/home/user/project/src/index.ts", wd).allowed);
        assert!(is_path_within_sandbox(wd, wd).allowed);
        assert!(!is_path_within_sandbox("/etc/passwd", wd).allowed);
        assert!(!is_path_within_sandbox("/home/user/project/../../etc/passwd", wd).allowed);
        assert!(is_path_within_sandbox("/tmp/scratch", wd).allowed);
    }
}
