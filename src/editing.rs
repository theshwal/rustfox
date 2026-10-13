//! Line-ending and replacement semantics for the edit tool, ported from
//! `src/server/tools/edit.ts`.
//!
//! Pinned by contract/edit-rules.json, which was produced by recovering the
//! original's private helpers verbatim from the source and executing them.

/// The three line endings the edit tool distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Crlf,
    Lf,
    Cr,
}

impl LineEnding {
    pub fn as_str(self) -> &'static str {
        match self {
            LineEnding::Crlf => "\r\n",
            LineEnding::Lf => "\n",
            LineEnding::Cr => "\r",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            LineEnding::Crlf => "crlf",
            LineEnding::Lf => "lf",
            LineEnding::Cr => "cr",
        }
    }
}

/// Port of `detectLineEnding`.
///
/// The order of the checks is what makes this behave like the original: a
/// document containing any CRLF anywhere is classified `crlf` even if it also
/// contains bare LF or bare CR, because CRLF is tested first. A document with no
/// newline at all, and an empty one, both fall back to `lf`.
pub fn detect_line_ending(content: &str) -> LineEnding {
    if content.contains("\r\n") {
        return LineEnding::Crlf;
    }
    if content.contains('\n') {
        return LineEnding::Lf;
    }
    if content.contains('\r') {
        return LineEnding::Cr;
    }
    LineEnding::Lf
}

/// Port of `normalizeToLF`: collapse CRLF first, then any remaining bare CR.
pub fn normalize_to_lf(content: &str) -> String {
    content.replace("\r\n", "\n").replace('\r', "\n")
}

/// How many times `needle` occurs in `haystack`.
///
/// An empty needle is reported as `-1` rather than the `usize::MAX` a naive
/// split would give, matching what the edit tool needs to reject the call.
pub fn count_occurrences(haystack: &str, needle: &str) -> i64 {
    if needle.is_empty() {
        return -1;
    }
    haystack.matches(needle).count() as i64
}

/// The outcome of a replacement attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplaceOutcome {
    /// Replaced; `occurrences` tells how many times the needle appeared.
    Replaced { result: String, occurrences: i64 },
    /// The needle was absent, so the content is unchanged.
    NotFound,
    /// The needle was empty, which the edit tool refuses.
    EmptyNeedle,
}

/// Replace the first occurrence of `needle`, or all of them when `replace_all`.
///
/// An empty needle never matches: JavaScript's `String.replace('', x)` would
/// splice the replacement at every position, and the edit tool guards against
/// that before reaching here, so the port reports it explicitly instead of
/// reproducing the splice.
pub fn replace_string(
    haystack: &str,
    needle: &str,
    replacement: &str,
    replace_all: bool,
) -> ReplaceOutcome {
    if needle.is_empty() {
        return ReplaceOutcome::EmptyNeedle;
    }
    let occurrences = haystack.matches(needle).count() as i64;
    if occurrences == 0 {
        return ReplaceOutcome::NotFound;
    }
    let result = if replace_all {
        haystack.replace(needle, replacement)
    } else {
        haystack.replacen(needle, replacement, 1)
    };
    ReplaceOutcome::Replaced {
        result,
        occurrences,
    }
}

/// Rewrite normalised content back to the file's original line ending.
///
/// The edit tool normalises to LF while matching so that a CRLF file can be
/// edited with LF needles, then converts back on write. This is that step.
pub fn restore_line_ending(content: &str, ending: LineEnding) -> String {
    match ending {
        LineEnding::Lf => content.to_string(),
        LineEnding::Crlf => content.replace('\n', "\r\n"),
        LineEnding::Cr => content.replace('\n', "\r"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlf_wins_over_bare_lf_and_cr() {
        // The original checks CRLF first, so a mixed document is crlf.
        assert_eq!(detect_line_ending("a\r\nb\nc"), LineEnding::Crlf);
        assert_eq!(detect_line_ending("a\r\nb\rc"), LineEnding::Crlf);
    }

    #[test]
    fn no_newline_falls_back_to_lf() {
        assert_eq!(detect_line_ending("abc"), LineEnding::Lf);
        assert_eq!(detect_line_ending(""), LineEnding::Lf);
    }

    #[test]
    fn lone_newlines_are_distinguished() {
        assert_eq!(detect_line_ending("a\nb"), LineEnding::Lf);
        assert_eq!(detect_line_ending("a\rb"), LineEnding::Cr);
        assert_eq!(detect_line_ending("\r\r"), LineEnding::Cr);
    }

    #[test]
    fn normalisation_collapses_both_forms() {
        assert_eq!(normalize_to_lf("a\r\nb\rc\nd"), "a\nb\nc\nd");
        assert_eq!(normalize_to_lf(""), "");
        // CRLF is collapsed in one pass, so it does not become two newlines.
        assert_eq!(normalize_to_lf("a\r\nb"), "a\nb");
    }

    #[test]
    fn counting_rejects_an_empty_needle() {
        assert_eq!(count_occurrences("aXaXa", "X"), 2);
        assert_eq!(count_occurrences("aaa", "a"), 3);
        assert_eq!(count_occurrences("abc", "zzz"), 0);
        assert_eq!(count_occurrences("abc", ""), -1);
    }

    #[test]
    fn replacement_semantics() {
        assert_eq!(
            replace_string("a\nb\na\nb", "a", "X", false),
            ReplaceOutcome::Replaced {
                result: "X\nb\na\nb".to_string(),
                occurrences: 2
            }
        );
        assert_eq!(
            replace_string("a\nb\na\nb", "a", "X", true),
            ReplaceOutcome::Replaced {
                result: "X\nb\nX\nb".to_string(),
                occurrences: 2
            }
        );
        assert_eq!(
            replace_string("abc", "zzz", "X", false),
            ReplaceOutcome::NotFound
        );
        assert_eq!(
            replace_string("abc", "", "X", false),
            ReplaceOutcome::EmptyNeedle
        );
    }

    #[test]
    fn line_endings_round_trip() {
        for ending in [LineEnding::Crlf, LineEnding::Lf, LineEnding::Cr] {
            let original = match ending {
                LineEnding::Crlf => "a\r\nb\r\n",
                LineEnding::Lf => "a\nb\n",
                LineEnding::Cr => "a\rb\r",
            };
            let detected = detect_line_ending(original);
            assert_eq!(detected, ending);
            let restored = restore_line_ending(&normalize_to_lf(original), detected);
            assert_eq!(restored, original, "round trip failed for {ending:?}");
        }
    }
}
