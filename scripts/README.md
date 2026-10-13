# Contract extractors

These scripts produce the files in `contract/`. They run **against a checkout of
`co-l/openfox`** and are the reason the Rust port can be checked against the real
implementation instead of against someone's reading of it.

Every extractor either:

- imports the original's modules **as a library** (no server is started), or
- boots the server **in the same process** and exits when it is done.

Nothing starts a background process, so a failed run leaves nothing to clean up.
That matters: the reference server spawns watchers and plugin subprocesses, and
killing the parent takes the whole tree with it.

## Regenerating everything

```bash
scripts/regenerate-contract.sh
```

This clones `co-l/openfox` (branch `develop` by default) into `/tmp`, installs
its dependencies, and runs each extractor in turn.

Override the source or destination:

```bash
OPENFOX_REF=/path/to/local/openfox OPENFOX_BRANCH=main \
  scripts/regenerate-contract.sh contract
```

## What each one produces

| Script | Output | How it works |
|---|---|---|
| `extract-schema.ts` | `db-schema.sql` | runs `initDatabase()` against a fresh SQLite file and dumps the result |
| `extract-contract.ts` | `rest-routes.json` | scans route modules and joins them with the mount table in `index.ts` |
| `extract-ws.ts` | `ws-protocol.json` | drives a live socket with every message type and records the actual reply |
| `extract-path-rules.ts` | `path-rules.json` | imports `path-security` and calls its pure functions |
| `extract-path-access.ts` | `path-access.json` | calls `checkPathsAccess` against a real workdir |
| `extract-confirmation.ts` | `confirmation.json` | exercises the session allowlist and the denial error |
| `extract-io-rules.ts` | `io-rules.json` | feeds real encoded bytes to `detectImageType` |
| `extract-encoding-rules.ts` | `encoding-rules.json` | calls `detectEncoding` on BOM and legacy samples |
| `extract-edit-rules.ts` | `edit-rules.json` | recovers `detectLineEnding`/`normalizeToLF` verbatim from the source and executes them |
| `extract-shell-rules.ts` | `shell-rules.json` | calls the shell tool's pure helpers |
| `extract-write-rules.ts` | `write-rules.json` | calls `encodeContent`, plus the tools' JSON Schemas |
| `extract-process-rules.ts` | `process-rules.json` | drives the background-process store through cap exhaustion and pagination |
| `verify-contract.ts` | `conformance-baseline.json` | boots the server in-process and probes every contract route |

## Verifying a contract change

After regenerating, the Rust tests replay every fixture:

```bash
cargo test
```

A fixture change that the port does not yet implement fails the tests rather
than passing silently. That is the whole point of generating the contract from
running code.
