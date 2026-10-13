p = 'PROGRESS.md'
s = open(p).read()

anchor = "Still missing in P2: `write`, `edit` file I/O and diagnostics, `shell`, `pty`."

new = """### shell (pure helpers)

`src/shellrules.rs` implements `detectGitMutation`, `formatDuration` and
`hasBackgroundAmpersand`. Pinned by `contract/shell-rules.json` (30 git, 26
duration, 17 ampersand cases) and `tests/shell_rules.rs`.

What only the fixtures reveal:

- **the quote guard is load-bearing.** `git "checkout" main` is detected and
  the returned fragment is `git "checkout`, quote included and operand dropped.
  The regex deliberately runs on the raw command; stripping quotes first would
  let the quoted verb slip past. The port matches that and reproduces the
  truncated fragment, since callers display it rather than re-execute it.
- the fragment starts at `git`, so `sudo git pull` yields `git pull` and not
  `sudo git pull`
- a word boundary is required on both sides: `mygit checkout` and
  `git-checkout` are **not** detected, while `echo checkout main` is not either
- `git branch` needs a mutating flag; `git branch --list` is not a mutation
- `formatDuration` has no hour format: 3,600,000 ms renders as `60m 0s` and
  86,399,000 ms as `1439m 59s`. A seconds-rounding that lands on 60 carries into
  the next minute instead of printing `1m 60s`.
- a bare `&` backgrounds a command but `&&` does not; `2>&1`, `&>`, `|&` and
  `>&` are all rewritten first. Quoted spans and escaped `\\&` are stripped
  first, so `echo "a & b"` is correctly not a background operator.

Still missing in P2: `write`, `edit` file I/O and diagnostics, `pty`."""

assert anchor in s, 'anchor not found'
s = s.replace(anchor, new)
open(p, 'w').write(s)
print('documented')
