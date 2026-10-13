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

## Phases 2–6 — not started

- P2 tools (`read`/`write`/`edit`/`path-security`/`shell`/pty) — ~18,700 lines
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
