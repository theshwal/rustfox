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

Still missing in P2: `write`, `edit`, `shell`, `pty`, and the
`PathAccessDeniedError` / confirmation flow that path-security drives.

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
