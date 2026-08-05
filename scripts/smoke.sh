#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

HOST_TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
TARGET_TRIPLE="${TARGET_TRIPLE:-$HOST_TRIPLE}"
case "$TARGET_TRIPLE" in
  x86_64-unknown-linux-gnu) ASSET_PLATFORM_VALUE="linux-x64" ;;
  aarch64-unknown-linux-gnu) ASSET_PLATFORM_VALUE="linux-arm64" ;;
  x86_64-apple-darwin) ASSET_PLATFORM_VALUE="macos-x64" ;;
  aarch64-apple-darwin) ASSET_PLATFORM_VALUE="macos-arm64" ;;
  *) echo "Unsupported smoke target: $TARGET_TRIPLE" >&2; exit 1 ;;
esac
ASSET_PLATFORM_VALUE="${ASSET_PLATFORM:-$ASSET_PLATFORM_VALUE}"
ARCHIVE="$ROOT/dist/lasso-zen-bre-1.0.0-beta.11-$ASSET_PLATFORM_VALUE.tar.gz"
TEMP_DIR="$(mktemp -d)"
PROCESS_ID=""

cleanup() {
  if [[ -n "$PROCESS_ID" ]] && kill -0 "$PROCESS_ID" 2>/dev/null; then
    kill -TERM "$PROCESS_ID" 2>/dev/null || true
    wait "$PROCESS_ID" 2>/dev/null || true
  fi
  rm -rf "$TEMP_DIR"
}
trap cleanup EXIT

tar -xzf "$ARCHIVE" -C "$TEMP_DIR"
PORT="$(python3 - <<'PY'
import socket
with socket.socket() as sock:
    sock.bind(('127.0.0.1', 0))
    print(sock.getsockname()[1])
PY
)"

"$TEMP_DIR/lasso-zen-bre" \
  --service-root "$TEMP_DIR" \
  --host 127.0.0.1 \
  --port "$PORT" \
  --decisions-dir "$TEMP_DIR/decisions" \
  >"$TEMP_DIR/stdout.log" 2>"$TEMP_DIR/stderr.log" &
PROCESS_ID=$!

for _ in $(seq 1 80); do
  if curl --silent --fail "http://127.0.0.1:$PORT/health/ready" >"$TEMP_DIR/ready.json"; then
    break
  fi
  sleep 0.25
done

curl --silent --fail "http://127.0.0.1:$PORT/health/ready" >"$TEMP_DIR/ready.json"
curl --silent --fail \
  -H 'content-type: application/json' \
  --data '{}' \
  "http://127.0.0.1:$PORT/v1/decisions/example/evaluate" >"$TEMP_DIR/evaluation.json"

python3 - "$TEMP_DIR/ready.json" "$TEMP_DIR/evaluation.json" <<'PY'
import json
import pathlib
import sys

ready = json.loads(pathlib.Path(sys.argv[1]).read_text())
evaluation = json.loads(pathlib.Path(sys.argv[2]).read_text())
if ready.get('status') != 'ready' or ready.get('engineVersion') != '1.0.0-beta.11':
    raise SystemExit('packaged readiness response is invalid')
if evaluation.get('result', {}).get('message') != 'Hello from Service Lasso ZEN BRE':
    raise SystemExit('packaged decision evaluation is invalid')
PY

kill -TERM "$PROCESS_ID"
wait "$PROCESS_ID"
PROCESS_ID=""

if grep -R -E 'DO_NOT_LEAK|BEGIN PRIVATE KEY|raw_secret[=:]' "$TEMP_DIR"/stdout.log "$TEMP_DIR"/stderr.log; then
  echo "Packaged service logs contain a forbidden sentinel" >&2
  exit 1
fi

echo "Packaged $ASSET_PLATFORM_VALUE smoke test passed"
