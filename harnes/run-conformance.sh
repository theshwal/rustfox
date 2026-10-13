#!/bin/bash
# Run the P0 conformance harness against a freshly started reference server.
# Starting the server and probing it inside a single command avoids the race
# where the server dies between two separate tool invocations.
set -u
PORT=41555
BASE="http://127.0.0.1:$PORT"

cleanup() {
  [ -n "${SRV_PID:-}" ] && kill -TERM -- -"$SRV_PID" 2>/dev/null
  [ -n "${SRV_PID:-}" ] && kill -TERM "$SRV_PID" 2>/dev/null
  return 0
}
trap cleanup EXIT

cd /tmp/ofox-probe || exit 1
OPENFOX_MOCK_LLM=true OPENFOX_DB_PATH=:memory: OPENFOX_LOG_LEVEL=error \
OPENFOX_HOST=127.0.0.1 OPENFOX_PORT=$PORT \
  npx tsx src/cli/index.ts --no-browser > /tmp/ref-server.log 2>&1 &
SRV_PID=$!

for _ in $(seq 1 90); do
  if curl -sf -m 2 "$BASE/api/health" > /dev/null 2>&1; then
    break
  fi
  sleep 1
done

if ! curl -sf -m 2 "$BASE/api/health" > /dev/null 2>&1; then
  echo "reference server failed to start" >&2
  tail -30 /tmp/ref-server.log >&2
  exit 3
fi

echo "reference server up on $PORT" >&2
node "$1/harnes/conformance.mjs" "$BASE" "$1/contract/rest-routes.json"
RC=$?
if [ "$RC" -ne 0 ] && [ ! -s /tmp/conf.json ]; then
  echo "harness produced no output (rc=$RC)" >&2
  node --version >&2
  node -e "console.log('node exec ok')" >&2
fi
exit $RC
