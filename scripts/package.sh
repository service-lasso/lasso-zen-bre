#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

ENGINE_VERSION="1.0.0-beta.11"
HOST_TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
TARGET_TRIPLE="${TARGET_TRIPLE:-$HOST_TRIPLE}"

if [[ -n "${ASSET_PLATFORM:-}" ]]; then
  ASSET_PLATFORM_VALUE="$ASSET_PLATFORM"
else
  case "$TARGET_TRIPLE" in
    x86_64-unknown-linux-gnu) ASSET_PLATFORM_VALUE="linux-x64" ;;
    aarch64-unknown-linux-gnu) ASSET_PLATFORM_VALUE="linux-arm64" ;;
    x86_64-apple-darwin) ASSET_PLATFORM_VALUE="macos-x64" ;;
    aarch64-apple-darwin) ASSET_PLATFORM_VALUE="macos-arm64" ;;
    *) echo "Unsupported package target: $TARGET_TRIPLE" >&2; exit 1 ;;
  esac
fi

DIST="$ROOT/dist"
STAGING="$DIST/staging-$ASSET_PLATFORM_VALUE"
ARCHIVE="$DIST/lasso-zen-bre-$ENGINE_VERSION-$ASSET_PLATFORM_VALUE.tar.gz"
BINARY="$ROOT/target/$TARGET_TRIPLE/release/lasso-zen-bre"

cargo build --release --locked --target "$TARGET_TRIPLE"

mkdir -p "$DIST"
rm -rf "$STAGING"
mkdir -p "$STAGING/decisions" "$STAGING/config" "$STAGING/logs" "$STAGING/.state" "$STAGING/THIRD_PARTY_LICENSES"

cp "$BINARY" "$STAGING/lasso-zen-bre"
cp "$ROOT/service.json" "$STAGING/service.json"
cp "$ROOT/LICENSE" "$STAGING/LICENSE"
cp "$ROOT/NOTICE" "$STAGING/NOTICE"
cp "$ROOT/README.md" "$STAGING/README.md"
cp "$ROOT/THIRD_PARTY_LICENSES/zen-engine-MIT.txt" "$STAGING/THIRD_PARTY_LICENSES/zen-engine-MIT.txt"
cp "$ROOT/examples/decisions/example.json" "$STAGING/decisions/example.json"
python3 "$ROOT/scripts/write_build_metadata.py" "$STAGING/BUILD-IDENTITY.json" "$TARGET_TRIPLE"
chmod +x "$STAGING/lasso-zen-bre"

rm -f "$ARCHIVE"
tar -czf "$ARCHIVE" -C "$STAGING" \
  lasso-zen-bre \
  service.json \
  LICENSE \
  NOTICE \
  README.md \
  BUILD-IDENTITY.json \
  THIRD_PARTY_LICENSES \
  decisions \
  config \
  logs \
  .state

ARCHIVE_CONTENTS="$(tar -tzf "$ARCHIVE")"
grep -Eq '^(\./)?lasso-zen-bre$' <<<"$ARCHIVE_CONTENTS"
grep -Eq '^(\./)?service.json$' <<<"$ARCHIVE_CONTENTS"
grep -Eq '^(\./)?BUILD-IDENTITY.json$' <<<"$ARCHIVE_CONTENTS"
grep -Eq '^(\./)?decisions/example.json$' <<<"$ARCHIVE_CONTENTS"

echo "Created $ARCHIVE"
