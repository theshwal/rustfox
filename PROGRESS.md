# Progress

Honest status against the original. The original is ~120,500 lines of
production code; this port is ~2,500. Numbers below are measured, not estimated.

## Phase 0 — contract extraction: **done**

All three contract artifacts are generated from the running original, not
transcribed:

| Artifact | Content | Verified by |
|---|---|---|
| `contract/db-schema.sql` | 12 tables, 9 indexes | applied to a fresh DB and dumped |
| `contract/rest-routes.json` | 209 REST routes | extracted from route modules + mount table |
| `contract/ws-protocol.json` | 22 inbound WS types | each classified by the real server's reply |

Findings that only came out of running the original, and that a human reading
the source would likely have got wrong:

- **13 of 22 WebSocket message types must be rejected** with
  `DEPRECATED_MESSAGE_TYPE`. They are not "unimplemented" — returning anything
  else is a contract violation.
- `session.load` validates its payload *before* the session check, so an empty
  payload yields `INVALID_PAYLOAD`, not `NO_SESSION`.
- The database schema is **not idempotent** as emitted by the original (bare
  `CREATE TABLE`), so it is rewritten to `IF NOT EXISTS` before being applied.
- Column names differ from the obvious guess: `projects.workdir` (not `path`),
  `sessions.title` (not `name`), `created_at` is TEXT ISO-8601, not an integer.

## Phase 1 — server core: **partial**

Working and covered by tests:

- axum server, CORS, graceful shutdown
- SQLite via `rusqlite`, schema applied verbatim from the contract, idempotent
  re-runs, no data migration of an existing openfox database
- project CRUD (create/read/update/delete/star), including the `workdir` UNIQUE
  conflict returning 409
- session CRUD, favourite, fork, replay, truncate, provider and mode/prompt
  settings
- settings read/write with upsert
- event log append with the `(session_id, seq)` ordering the original relies on
- WebSocket endpoint implementing the extracted vocabulary and its three
  response classes

**Not yet implemented:** ~140 of the 209 REST routes. The unimplemented ones are
concentrated in `plugins` (23), `skills`/`agents`/`commands`/`workflows` (40),
`mcp` (10), `dev-server` (9), `tasks` (13), `terminals` (4), `auto-update` (3).

## Phase 2 — tools: **started**

Two tool modules ported and pinned against the original's real behaviour.

### path-security

`src/pathsec.rs` implements `isSensitivePath`, `isPathWithinSandbox`,
`resolveRelativeTraversals`, `extractDangerousPatterns`, `extractGitNoVerify`
and `extractSensitivePathsFromCommand`. Pinned by
`contract/path-rules.json` (53 fixtures) and `tests/path_rules.rs`.

What only the fixtures reveal:

- `rm -rf /` is **not** flagged; only `rm -rf ~` is. The pattern list has a gap.
- `.envrc` is **not** sensitive, and `.env.example` is explicitly excluded.
- `resolveRelativeTraversals` tracks `cd`, so `cd ../sibling` resolves against
  the moved cwd, and `ls ./src` yields nothing because only `..` counts as a
  traversal.
- `extractSensitivePathsFromCommand` returns the token **as written** (`cat .env`
  yields `.env`), not a resolved path.
- `-n` only means `--no-verify` for `git commit` / `git am`; elsewhere git gives
  it a harmless meaning, so flagging it would be a false positive.

### read (file-type detection)

`src/iotools.rs` implements `detectImageType` and `formatSize`, pinned by
`contract/io-rules.json` (14 fixtures built from real encoded bytes) and
`tests/io_rules.rs`.

What only the fixtures reveal:

- SVG detection accepts a document starting with `<svg`, but **rejects** one
  that starts with `<?xml`, even though both are valid SVG. The original only
  checks `startsWith('<?xml')` after trimming, and the XML case falls through
  because `file-type` reports it as generic XML.
- `application/pdf` and `application/zip` are identified but never treated as
  images.
- A truncated PNG header is not detected at all — no minimum-length check.

### encoding detection

`src/encoding.rs` implements `detectEncoding` and `normalizeEncoding`, pinned by
`contract/encoding-rules.json` (14 detect fixtures) and
`tests/encoding_rules.rs`. All 14 agree with the original.

What only the fixtures reveal:

- A 4-byte little-endian BOM is reported as `utf-32`, **not** `utf-32le`, while
  the big-endian one is `utf-32be`. The asymmetry comes from the BOM branch
  returning before `normalizeEncoding` runs.
- Empty input is `utf-8` at confidence **1.0**; any other valid UTF-8 is
  `utf-8` at **0.95**.
- jschardet resolves single-byte legacy text in a way a structural heuristic
  does not: `café naïve` (é then ï) is `windows-1252`, while `naïve résumé`
  (ï then é) is `ISO-8859-2` at 0.9596. The order of the two accented bytes is
  what separates them, and the port reproduces it explicitly.

`statistical_detect` is still an approximation of jschardet's frequency tables.
It agrees on every pinned fixture, and `KNOWN_GAPS` in `tests/encoding_rules.rs`
is the place any future divergence must be recorded; the test fails if an entry
becomes stale, so the list cannot rot into an excuse.

### edit (line endings and replacement)

`src/editing.rs` implements `detectLineEnding`, `normalizeToLF` and the
replacement semantics, pinned by `contract/edit-rules.json` and
`tests/edit_rules.rs`.

`detectLineEnding` and `normalizeToLF` are private in the original, so the
fixtures were produced by recovering both function bodies verbatim from
`edit.ts`, stripping the type annotations and executing them. The contract is
therefore generated from the original's own code text, not from a
re-implementation of it.

What only the fixtures reveal:

- **CRLF wins outright.** A document containing any `\r\n` is `crlf` even when
  it also contains bare `\n` or bare `\r`, because the CRLF check runs first.
  Mixed-ending files are silently normalised to CRLF on write.
- A document with no newline at all, and an empty one, both report `lf`.
- `\r\r` is `cr`, not a CRLF.
- Replacement reports the occurrence count alongside the result, which is what
  lets the tool reject an ambiguous edit.

The port adds one deliberate difference: an empty needle returns an explicit
`EmptyNeedle` rather than reproducing JavaScript's `String.replace('', x)`
splice-at-every-position behaviour. The original guards against that before
reaching the replacement, so the two paths never meet in practice; the port
makes the refusal explicit instead of relying on an upstream guard.

### file-tracker (read-before-write)

`src/filetrack.rs` implements `computeFileHash` and `validateFileForWrite`,
which is the guard that stops the agent overwriting a file it never saw or that
changed under it. Covered by `tests/file_tracker.rs` (9 cases).

The rules are subtler than "was it read":

- a file that does not exist yet may be created without being read
- a file that exists must have been read **at its current hash**, otherwise it
  is `FileChangedExternallyError` rather than `FileNotReadError`
- with a `baseDir`, an identical hash recorded under the same repo-relative path
  from a different workspace also satisfies the check, so switching workspaces
  does not force a re-read
- but that fallback is skipped when the path escapes the base directory: two
  unrelated directories can both yield the string `../config.json`

`computeFileHash` swallows only ENOENT. Any other filesystem error is
propagated, so a permission problem is never mistaken for a missing file.

The SHA-256 implementation is self-contained rather than a crate dependency,
so it can be read next to the original's `createHash('sha256')`; it is checked
against the NIST vectors including the 448-bit message.

### checkPathsAccess (the gate everything passes through)

`src/pathaccess.rs` implements `checkPathsAccess`, which read, write, edit and
shell all funnel through before touching a path. Pinned by
`contract/path-access.json` (17 fixtures captured against a real workdir) and
`tests/path_access.rs`.

What only the fixtures reveal:

- a path can be **both** denied and sensitive, and then appears in both lists.
  `needsConfirmation` only asks whether either list is non-empty, so the two
  categories are not exclusive
- the sensitivity check runs **independently** of the sandbox verdict, so a
  sensitive file inside the workdir still asks for confirmation
- `/dev/null` is listed among the "safe device paths" in the source, yet it is
  still reported in `deniedPaths` when the workdir is elsewhere: the safe list
  feeds a different check
- relative inputs resolve against the **process cwd**, not the workdir. That is
  why a recorded fixture for `../outside.txt` came back allowed on the
  extraction machine: it landed in `/tmp`, an always-allowed root. The port
  agrees with the original for absolute paths; that one relative fixture is
  exempted in the test with the reason stated inline, because its verdict
  describes a path that does not exist on the machine running the test.

### path confirmation and the allowlist

`src/confirmation.rs` implements the session allowlist, the pending-confirmation
registry and `PathAccessDeniedError`. Pinned by `contract/confirmation.json`
(11 allowlist scenarios, 5 denial reasons) and `tests/confirmation.rs`.

What only the fixtures reveal:

- the allowlist normalises on **both** insertion and lookup, so `/etc/./passwd`
  and `/etc/shadow/../passwd` both match an entry for `/etc/passwd`
- a **trailing slash does not match**: Node's `path.normalize` preserves
  `/etc/passwd/` as-is, so an entry for `/etc/passwd` does not cover it. This
  looked cosmetic and is not: the contract fixture caught the port dropping the
  trailing separator, which would have widened every allowlist entry. The fix
  went into `pathsec::normalize`, with a unit test pinning both behaviours.
- `dangerous_command` has **no branch** in the message ternary chain, so it falls
  through to the `outside_workdir` wording even though the reason is recorded on
  the error. A caller reading only the message is told the wrong thing.
- a custom message replaces the generated one entirely: neither the reason
  phrase nor the path list survives, while `error.reason` still holds the truth.

### write (content encoding and tool schemas)

`src/encoding_io.rs` implements `encodeContent`/`decodeContent`. Pinned by
`contract/write-rules.json` (308 encode cases, 28 round trips) and
`tests/write_rules.rs`. The three tool JSON Schemas the model actually sees
(`read_file`, `write_file`, `edit_file`) are captured in the same contract: the
schema is part of the interface, not an implementation detail.

Two real bugs in the port, both caught by the contract:

- UTF-32 little-endian shared the two-byte UTF-16 BOM, writing 2 bytes where the
  original writes 4 (`FF FE 00 00`). The result is a file whose first character
  reads back as U+0000.
- the lossy substitution into a single-byte table is counted in **UTF-16 code
  units**, not characters and not UTF-8 bytes: a CJK ideograph becomes one `?`
  while an emoji becomes two, because iconv converts through UTF-16 and sees a
  surrogate pair. Two wrong implementations were written before the behaviour
  was measured rather than guessed.

What the fixtures reveal about the original:

- single-byte encodings are lossy and **do not error**. Writing an emoji to a
  Latin-1 file succeeds and silently corrupts the character, so an agent
  honouring a project's Latin-1 convention loses non-Latin text with no signal.
- an unrecognised encoding name is an error, never a silent UTF-8 fallback
- names are normalised before dispatch, so `utf8`, `ascii` and `UTF-8` share a
  branch and `UTF-16` resolves to UTF-16LE

### background processes

`src/procs.rs` implements the background-process store: the per-session cap,
lifecycle transitions, log buffering and pagination. Pinned by
`contract/process-rules.json` and `tests/process_rules.rs`.

What only the fixtures reveal:

- **the cap counts live processes, not stored ones.** `MAX_PER_SESSION` is 5,
  yet a session can hold 8 entries once some have exited: the full listing is 8
  while the live count is 3. A caller that decides the session is full by
  counting entries rather than live ones refuses work it should allow.
- **a zero-length page still advances the cursor.** `maxLines = 0` returns no
  lines with `nextOffset = totalLines` and `hasMore = true`, so a caller looping
  on `hasMore` stops after one iteration.
- log offsets are dense and shared across streams, so offset 2 in the fixtures
  is a stderr line.
- `get_logs` follows JavaScript's `slice`, not a clamp: offset `-5` with limit 3
  returns the three lines ending five from the tail. My first implementation
  clamped at zero and returned nothing; the contract caught it.
- processes are keyed by `(processId, sessionId)`, so the same id in another
  session is invisible even though `getProcessById` finds it.

### P2 coverage

Ported and pinned against the original's behaviour: `path-security`,
`checkPathsAccess`, the confirmation allowlist, `file-tracker`, `encoding`
detection, `encodeContent`, the shell helpers, and the background-process store.

Not ported: the actual process spawning, since that is `node-pty` bound to a
live terminal rather than a rule that can be pinned to fixtures.

## Phases 3–6 — not started

- P3 agent loop, LLM providers, event store, session manager — ~42,000 lines
- P4 MCP, plugins, skills, sub-agents, workflows, tasks — ~22,000 lines
- P5 Svelte client replacing 597 React files — ~103,000 lines
- P6 replay of the original's 491 test files — gated on the above

## LSP

Explicitly **out of scope** until proven. The original embeds four language
servers (`pyright`, `typescript-language-server`, `yaml-language-server`,
`bash-language-server`). Replacing them is the most expensive and least
substitutable part of the port; leaving them out is reversible, leaving them in
half-built is not.

## Bugs the tests caught

Recorded because they are the point of having a harness:

1. `migrate_is_idempotent` — the extracted DDL is not idempotent.
2. `query_map` accepted SQL with `?n` placeholders but bound no parameters, so
   every `WHERE id = ?1` silently matched nothing.
3. `exists()` read the first column as `i64`, so any check against a TEXT column
   always returned false — every existence check in the server was wrong, and
   nothing surfaced it until an HTTP test exercised a real round-trip.
4. The regex engine anchored matches at offset 0 while `RegExp.test()` searches
   the whole string, so `>\s*/dev/sd` failed to match `dd > /dev/sda`.

## Running the original without destabilising the host

The reference server spawns watchers and plugin subprocesses, and killing the
parent takes that whole tree down with it. Everything here therefore runs
**in-process** instead:

- contract extraction imports the original's modules as a library
  (`npx tsx scripts/extract-*.ts`), which starts no server and exits normally
- the Rust side is exercised through `cargo test` against an in-memory router
  (`tests/http.rs`), never a bound port

No background process is ever started, so there is never anything to kill.
