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
  Linux*)
    case "$(uname -m)" in
      x86_64) ASSET_PLATFORM_VALUE="linux-x64" ;;
      aarch64|arm64) ASSET_PLATFORM_VALUE="linux-arm64" ;;
      *) echo "Unsupported Linux architecture: $(uname -m)" >&2; exit 1 ;;
    esac
    ;;
  Darwin*)
    case "$(uname -m)" in
      x86_64) ASSET_PLATFORM_VALUE="macos-x64" ;;
      arm64) ASSET_PLATFORM_VALUE="macos-arm64" ;;
      *) echo "Unsupported macOS architecture: $(uname -m)" >&2; exit 1 ;;
    esac
    ;;
  *) echo "Unsupported OS for verify.sh: $OS_NAME" >&2; exit 1 ;;
esac
ASSET_PLATFORM_VALUE="${ASSET_PLATFORM:-$ASSET_PLATFORM_VALUE}"
ARTIFACT_PATH="../dist/lasso-zen-bre-1.0.0-beta.11-$ASSET_PLATFORM_VALUE.tar.gz"

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
