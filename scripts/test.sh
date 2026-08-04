#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

for path in \
  "$ROOT/Cargo.toml" \
  "$ROOT/service.json" \
  "$ROOT/verify/service-harness.json" \
  "$ROOT/src/main.rs" \
  "$ROOT/NOTICE"; do
  if [[ ! -f "$path" ]]; then
    echo "Missing required file: $path" >&2
    exit 1
  fi
done

python3 - <<'PY'
import json
import pathlib
import re

service = json.loads(pathlib.Path('service.json').read_text())
if service.get('id') != 'zen-bre':
    raise SystemExit('service.json id mismatch')
if service.get('enabled') is not False:
    raise SystemExit('zen-bre must stay disabled by default')
upstream = service.get('meta', {}).get('upstream', {})
if upstream.get('crate') != 'zen-engine' or upstream.get('version') != '1.0.0-beta.11':
    raise SystemExit('upstream zen-engine pin mismatch')
globalenv = service.get('execconfig', {}).get('globalenv', {})
if 'ZEN_BRE_URL' not in globalenv:
    raise SystemExit('ZEN_BRE_URL global export missing')
if 'healthcheck' in service:
    raise SystemExit('Singular healthcheck is not allowed; use healthchecks[].')
if 'healthcheck' in service.get('execconfig', {}):
    raise SystemExit('execconfig.healthcheck is not allowed; use top-level healthchecks[].')
checks = service.get('healthchecks')
if not isinstance(checks, list):
    raise SystemExit('healthchecks must be an array.')

contract = json.loads(pathlib.Path('verify/service-harness.json').read_text())
if contract.get('serviceId') != 'zen-bre':
    raise SystemExit('service-harness.json serviceId mismatch')

cargo = pathlib.Path('Cargo.toml').read_text()
expected = r'zen-engine = \{ version = "=1\.0\.0-beta\.11", features = \["arbitrary_precision"\] \}'
if not re.search(expected, cargo):
    raise SystemExit('Cargo.toml must pin zen-engine 1.0.0-beta.11 with arbitrary_precision enabled')
PY

cargo test --locked

echo "lasso-zen-bre tests passed ($(uname -s))"
