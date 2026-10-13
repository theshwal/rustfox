#!/usr/bin/env bash
#
# Regenerate every file in contract/ from a checkout of co-l/openfox.
#
# The contract is the specification for the Rust port, and it is produced by
# *running the original*, never by reading it. This script clones the original
# into a scratch directory, installs its dependencies, and runs each extractor
# against it.
#
# Nothing here starts a long-lived process: every extractor exits when it is
# done, so there is nothing to clean up if it fails part way.
#
# Usage: scripts/regenerate-contract.sh [outputDir]
#   outputDir defaults to ./contract

set -euo pipefail

OUT="${1:-contract}"
REF="${OPENFOX_REF:-https://github.com/co-l/openfox.git}"
BRANCH="${OPENFOX_BRANCH:-develop}"
SCRATCH="${OPENFOX_SCRATCH:-/tmp/openfox-contract-ref}"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPTS="$REPO_ROOT/scripts"

if [ ! -d "$SCRATCH/src/server" ]; then
  echo "==> cloning $REF ($BRANCH) into $SCRATCH"
  rm -rf "$SCRATCH"
  git clone --depth 1 --branch "$BRANCH" "$REF" "$SCRATCH"
fi

if [ ! -d "$SCRATCH/node_modules" ]; then
  echo "==> installing dependencies"
  (cd "$SCRATCH" && npm install --no-audit --no-fund)
fi

# Each extractor is copied next to the original's sources and run from there,
# because they import modules by relative path.
run_extractor() {
  local name="$1"
  shift
  echo "==> $name"
  cp "$SCRIPTS/$name.ts" "$SCRATCH/scripts/$name.ts"
  # Each extractor takes its own destination as argv[2]; make it absolute so it
  # still points here after cd'ing into the reference checkout.
  local dest="$1"
  case "$dest" in
    /*) ;;
    *) dest="$REPO_ROOT/$dest" ;;
  esac
  (cd "$SCRATCH" && npx tsx "scripts/$name.ts" "$dest")
}

run_extractor extract-schema          "$OUT/db-schema.sql"
# extract-contract takes the directory: it writes rest-routes.json into it.
run_extractor extract-contract        "$OUT"
run_extractor extract-ws              "$OUT/ws-protocol.json"
run_extractor extract-path-rules      "$OUT/path-rules.json"
run_extractor extract-path-access     "$OUT/path-access.json"
run_extractor extract-confirmation    "$OUT/confirmation.json"
run_extractor extract-io-rules        "$OUT/io-rules.json"
run_extractor extract-encoding-rules  "$OUT/encoding-rules.json"
run_extractor extract-edit-rules      "$OUT/edit-rules.json"
run_extractor extract-shell-rules     "$OUT/shell-rules.json"
run_extractor extract-write-rules     "$OUT/write-rules.json"
run_extractor extract-process-rules   "$OUT/process-rules.json"

echo
echo "==> contract regenerated in $OUT"
git -C "$REPO_ROOT" status --short "$OUT"
