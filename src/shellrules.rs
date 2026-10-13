//! Pure shell helpers from `src/server/tools/shell.ts`.
//!
//! Pinned by contract/shell-rules.json, captured by calling the original.

/// Git subcommands and flag forms that mutate branch or workspace state.
///
/// The list is matched against the **raw** command, with quotes left in place.
/// That is deliberate: stripping quotes first would let `git "checkout" main`
/// slip past the guard, so the match sees the quote and still fires.
const GIT_MUTATION_HEADS: &[&str] = &[
    "checkout",
    "switch",
    "merge",
    "rebase",
    "reset",
    "cherry-pick",
    "worktree",
    "clone",
    "pull",
    "push",
    "fetch",
    "update-ref",
    "symbolic-ref",
];

/// `git branch` is only a mutation when one of these follows it.
const BRANCH_MUTATION_FLAGS: &[&str] = &[
    "-d", "-D", "-m", "-M", "-c", "-C", "--delete", "--move", "--copy", "--force",
];

/// Port of `detectGitMutation`: the matched fragment, or `None`.
///
/// The result is the literal text that matched, trimmed — so for
/// `git "checkout" main` it is `git "checkout`, with the quote included and the
/// operand dropped. Callers use it for display, not for re-execution.
pub fn detect_git_mutation(command: &str) -> Option<String> {
    let chars: Vec<char> = command.chars().collect();

    // Find a word-boundary `git`, then optional quote, then the subcommand.
    let mut i = 0usize;
    while i < chars.len() {
        if !chars[i].is_whitespace() && !is_word_boundary(&chars, i) {
            i += 1;
            continue;
        }
        if starts_word_at(&chars, i, "git") {
            let mut j = i + 3;
            // Require whitespace between `git` and the subcommand.
            let mut saw_ws = false;
            while j < chars.len() && chars[j].is_whitespace() {
                saw_ws = true;
                j += 1;
            }
            if saw_ws {
                if let Some(found) = match_git_verb(&chars, j) {
                    // The regex begins at `git`, so anything before it
                    // (`sudo`, an env assignment) is not part of the fragment.
                    return Some(
                        chars[i..found]
                            .iter()
                            .collect::<String>()
                            .trim()
                            .to_string(),
                    );
                }
            }
        }
        i += 1;
    }
    None
}

fn is_word_boundary(chars: &[char], i: usize) -> bool {
    i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_' || chars[i - 1] == '-')
}

fn starts_word_at(chars: &[char], i: usize, word: &str) -> bool {
    let w: Vec<char> = word.chars().collect();
    if i + w.len() > chars.len() {
        return false;
    }
    if chars[i..i + w.len()] != w[..] {
        return false;
    }
    // Must end on a boundary too, so `git-checkout` does not match.
    let after = i + w.len();
    after >= chars.len() || !(chars[after].is_alphanumeric() || chars[after] == '_')
}

/// Match the subcommand at `j`, returning the end index of the matched text.
fn match_git_verb(chars: &[char], j: usize) -> Option<usize> {
    // The original allows one optional quote between `git` and the verb.
    let mut start = j;
    if start < chars.len() && (chars[start] == '\'' || chars[start] == '"') {
        start += 1;
    }

    for verb in GIT_MUTATION_HEADS {
        if starts_word_at(chars, start, verb) {
            return Some(start + verb.chars().count());
        }
    }

    if starts_word_at(chars, start, "branch") {
        return match_branch_flag(chars, start + 6);
    }

    None
}

/// `git branch` counts as a mutation only with a flag, or with `-b <name>`.
fn match_branch_flag(chars: &[char], mut j: usize) -> Option<usize> {
    while j < chars.len() && chars[j].is_whitespace() {
        j += 1;
    }
    if j >= chars.len() {
        return None;
    }

    for flag in BRANCH_MUTATION_FLAGS {
        if starts_word_at(chars, j, flag) {
            return Some(j + flag.chars().count());
        }
    }

    // `-b <name>` / `-B <name>`: a new branch.
    if j + 2 <= chars.len() && chars[j] == '-' && matches!(chars.get(j + 1), Some('b') | Some('B'))
    {
        let mut k = j + 2;
        if k < chars.len() && chars[k].is_whitespace() {
            k += 1;
            let name_start = k;
            while k < chars.len() && !chars[k].is_whitespace() {
                k += 1;
            }
            if k > name_start {
                return Some(k);
            }
        }
    }

    // A bare flag glued to the word, e.g. `git branch-D`.
    if chars[j] == '-' {
        let mut k = j;
        while k < chars.len() && chars[k] != ' ' && chars[k] != '\t' {
            k += 1;
        }
        let glued: String = chars[j..k].iter().collect();
        for flag in BRANCH_MUTATION_FLAGS {
            if glued.starts_with(flag) {
                return Some(k);
            }
        }
    }

    None
}

/// Port of `formatDuration`.
///
/// The minute branch rounds the seconds, and a rounding that lands on 60 is
/// carried into the next minute rather than printed as `1m 60s`. There is no
/// hour formatting: an hour is reported as `60m`.
pub fn format_duration(ms: f64) -> String {
    if ms < 1000.0 {
        return format!("{ms}ms");
    }
    if ms < 60_000.0 {
        return format!("{:.1}s", ms / 1000.0);
    }
    let mut m = (ms / 60_000.0).floor();
    let mut s = (ms % 60_000.0 / 1000.0).round();
    if s == 60.0 {
        m += 1.0;
        s = 0.0;
    }
    format!("{}m {}s", m as i64, s as i64)
}

/// Port of `hasBackgroundAmpersand`.
///
/// A bare `&` backgrounds a command; the operators that merely contain one do
/// not. Quoted spans and escaped characters are removed first so `echo "a & b"`
/// is not mistaken for a background operator.
pub fn has_background_ampersand(command: &str) -> bool {
    let mut processed = strip_quoted_spans(command);
    processed = strip_escaped(&processed);

    for op in ["&&", "|&", "&>", ">&"] {
        processed = processed.replace(op, "  ");
    }
    // fd redirections such as 2>&1 leave a bare & once >& is gone.
    let mut chars: Vec<char> = processed.chars().collect();
    for i in 0..chars.len() {
        if chars[i] == '&' && i > 0 && chars[i - 1].is_ascii_digit() {
            chars[i] = ' ';
        }
    }
    // Also drop an & that immediately follows a > that survived.
    let joined: String = chars.into_iter().collect();
    joined.contains('&')
}

/// Replace the contents of single- and double-quoted spans with spaces.
fn strip_quoted_spans(command: &str) -> String {
    let mut out = String::with_capacity(command.len());
    let mut quote: Option<char> = None;
    for ch in command.chars() {
        match quote {
            Some(q) if ch == q => {
                quote = None;
                out.push(' ');
            }
            Some(_) => out.push(' '),
            None if ch == '\'' || ch == '"' => {
                quote = Some(ch);
                out.push(' ');
            }
            None => out.push(ch),
        }
    }
    out
}

/// Remove a backslash and the character it escapes.
fn strip_escaped(command: &str) -> String {
    let chars: Vec<char> = command.chars().collect();
    let mut out = String::with_capacity(command.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            out.push(' ');
            out.push(' ');
            i += 2;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutating_git_subcommands_are_detected() {
        for cmd in [
            "git checkout main",
            "git switch main",
            "git merge develop",
            "git rebase main",
            "git reset --hard",
            "git cherry-pick abc",
            "git worktree add ../wt",
            "git clone https://x/y.git",
            "git pull",
            "git push origin main",
            "git fetch",
            "git update-ref refs/heads/x",
            "git symbolic-ref HEAD",
        ] {
            assert!(
                detect_git_mutation(cmd).is_some(),
                "{cmd} should be detected"
            );
        }
    }

    #[test]
    fn read_only_git_subcommands_are_not_detected() {
        for cmd in [
            "git status",
            "git log",
            "git commit -m x",
            "git add .",
            "git diff",
        ] {
            assert_eq!(detect_git_mutation(cmd), None, "{cmd} must not be detected");
        }
    }

    #[test]
    fn quotes_do_not_bypass_detection() {
        // The whole point of matching the raw command: a quoted verb is still
        // caught, and the returned fragment keeps the quote.
        assert_eq!(
            detect_git_mutation("git \"checkout\" main"),
            Some("git \"checkout".to_string())
        );
        assert_eq!(
            detect_git_mutation("git 'switch' main"),
            Some("git 'switch".to_string())
        );
    }

    #[test]
    fn a_word_boundary_is_required() {
        assert_eq!(detect_git_mutation("mygit checkout main"), None);
        assert_eq!(detect_git_mutation("git-checkout main"), None);
        assert_eq!(detect_git_mutation("echo checkout main"), None);
        // But a prefix like `sudo` still reaches the real git.
        assert_eq!(
            detect_git_mutation("sudo git pull"),
            Some("git pull".to_string())
        );
    }

    #[test]
    fn branch_needs_a_mutating_flag() {
        assert_eq!(
            detect_git_mutation("git branch -d old"),
            Some("git branch -d".to_string())
        );
        assert_eq!(
            detect_git_mutation("git branch --delete old"),
            Some("git branch --delete".to_string())
        );
        assert_eq!(
            detect_git_mutation("git branch -M name"),
            Some("git branch -M".to_string())
        );
        assert_eq!(detect_git_mutation("git branch"), None);
        assert_eq!(detect_git_mutation("git branch --list"), None);
    }

    #[test]
    fn durations_render_like_the_original() {
        assert_eq!(format_duration(0.0), "0ms");
        assert_eq!(format_duration(999.0), "999ms");
        assert_eq!(format_duration(1000.0), "1.0s");
        assert_eq!(format_duration(59_999.0), "60.0s");
        assert_eq!(format_duration(60_000.0), "1m 0s");
        assert_eq!(format_duration(90_000.0), "1m 30s");
        assert_eq!(format_duration(119_999.0), "2m 0s");
        // Rounding that lands on 60 carries into the next minute.
        assert_eq!(format_duration(119_500.0), "2m 0s");
        // There is no hour formatting.
        assert_eq!(format_duration(3_600_000.0), "60m 0s");
        assert_eq!(format_duration(86_399_000.0), "1439m 59s");
        assert_eq!(format_duration(-1.0), "-1ms");
    }

    #[test]
    fn a_bare_ampersand_backgrounds() {
        assert!(has_background_ampersand("sleep 10 &"));
        assert!(has_background_ampersand("sleep 10 & wait"));
        assert!(has_background_ampersand("a & b & c"));
        assert!(has_background_ampersand("&"));
    }

    #[test]
    fn operators_containing_an_ampersand_do_not() {
        assert!(!has_background_ampersand("cmd1 && cmd2"));
        assert!(!has_background_ampersand("&&"));
        assert!(!has_background_ampersand("cmd 2>&1"));
        assert!(!has_background_ampersand("cmd &> file"));
        assert!(!has_background_ampersand("cmd |& other"));
        assert!(!has_background_ampersand("cmd >&2"));
    }

    #[test]
    fn quotes_and_escapes_hide_the_ampersand() {
        assert!(!has_background_ampersand("echo \"a & b\""));
        assert!(!has_background_ampersand("echo 'a & b'"));
        assert!(!has_background_ampersand("echo \\& not-background"));
        assert!(!has_background_ampersand("echo \"quote with && inside\""));
        assert!(!has_background_ampersand("no ampersand"));
        assert!(!has_background_ampersand(""));
    }
}
