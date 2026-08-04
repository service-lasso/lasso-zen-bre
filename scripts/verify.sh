#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

CONTRACT="${1:-./verify/service-harness.json}"
OUTPUT_DIR="${2:-./output/verify}"

if [[ -n "${SERVICE_LASSO_HARNESS_BIN:-}" ]]; then
  HARNESS="$SERVICE_LASSO_HARNESS_BIN"
elif command -v service-lasso-harness >/dev/null 2>&1; then
  HARNESS="$(command -v service-lasso-harness)"
else
  echo "service-lasso-harness binary not found. Set SERVICE_LASSO_HARNESS_BIN or add it to PATH." >&2
  exit 1
fi

mkdir -p "$OUTPUT_DIR"
RESOLVED_CONTRACT="$ROOT/verify/service-harness.ci.json"
RUN_OUTPUT_DIR="$OUTPUT_DIR/harness-run"
OS_NAME=$(uname -s)
case "$OS_NAME" in
  Linux*) ARTIFACT_PATH="../dist/lasso-zen-bre-1.0.0-beta.11-linux.tar.gz" ;;
  Darwin*) ARTIFACT_PATH="../dist/lasso-zen-bre-1.0.0-beta.11-darwin.tar.gz" ;;
  *) echo "Unsupported OS for verify.sh: $OS_NAME" >&2; exit 1 ;;
esac

python3 - "$CONTRACT" "$RESOLVED_CONTRACT" "$ARTIFACT_PATH" <<'PY'
import json
import pathlib
import sys

source = pathlib.Path(sys.argv[1])
target = pathlib.Path(sys.argv[2])
artifact_path = sys.argv[3]
doc = json.loads(source.read_text())
doc['artifact']['path'] = artifact_path
target.write_text(json.dumps(doc, indent=2) + '\n')
PY

"$HARNESS" validate-contract --contract "$RESOLVED_CONTRACT"
"$HARNESS" run --contract "$RESOLVED_CONTRACT" --output-dir "$RUN_OUTPUT_DIR"
