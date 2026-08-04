#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DIST="$ROOT/dist"
OS_NAME=$(uname -s)
case "$OS_NAME" in
  Linux*) PLATFORM="linux" ;;
  Darwin*) PLATFORM="darwin" ;;
  *) echo "Unsupported OS for package.sh: $OS_NAME" >&2; exit 1 ;;
esac
STAGING="$DIST/lasso-zen-bre-$PLATFORM"
TAR_PATH="$DIST/lasso-zen-bre-1.0.0-beta.11-$PLATFORM.tar.gz"

cargo build --release --locked

mkdir -p "$DIST"
rm -rf "$STAGING"
mkdir -p "$STAGING/decisions" "$STAGING/config" "$STAGING/logs" "$STAGING/.state"

cp "$ROOT/target/release/lasso-zen-bre" "$STAGING/lasso-zen-bre"
cp "$ROOT/service.json" "$STAGING/service.json"
cp "$ROOT/LICENSE" "$STAGING/LICENSE"
cp "$ROOT/NOTICE" "$STAGING/NOTICE"
chmod +x "$STAGING/lasso-zen-bre"

rm -f "$TAR_PATH"
tar -czf "$TAR_PATH" -C "$STAGING" .
echo "Created $TAR_PATH"
