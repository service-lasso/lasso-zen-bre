# lasso-zen-bre

`lasso-zen-bre` is a portable, local-first HTTP decision service for Service
Lasso. It embeds [GoRules ZEN](https://github.com/gorules/zen) through the native
Rust `zen-engine` crate; a system Rust, Node.js or Python runtime is not needed
after installation.

The Service Lasso project owns this wrapper, its HTTP/lifecycle contract and its
release packaging. GoRules owns ZEN, JDM and its separate editor/BRMS products.

## Service contract

- Service ID: `zen-bre`
- Display name: `GoRules ZEN Business Rules Engine`
- Wrapper version: `0.1.0`
- Embedded engine: `zen-engine = 1.0.0-beta.11`
- Preferred local endpoint: `127.0.0.1:18089`
- Default state: disabled until a consuming application opts in

The release workflow publishes:

| Target | Release asset | Service Lasso selector |
| --- | --- | --- |
| Windows x64 MSVC | `lasso-zen-bre-1.0.0-beta.11-windows-x64.zip` | `win32` |
| Linux x64 GNU | `lasso-zen-bre-1.0.0-beta.11-linux-x64.tar.gz` | `linux` |
| Linux ARM64 GNU | `lasso-zen-bre-1.0.0-beta.11-linux-arm64.tar.gz` | supplemental |
| macOS x64 | `lasso-zen-bre-1.0.0-beta.11-macos-x64.tar.gz` | supplemental |
| macOS ARM64 | `lasso-zen-bre-1.0.0-beta.11-macos-arm64.tar.gz` | `darwin` |

Service Lasso currently selects artifacts by `process.platform`, not CPU
architecture. The manifest therefore selects Windows x64, Linux x64 and macOS
ARM64 as its primary install targets. Linux ARM64 and macOS x64 are still built,
tested and published for explicit acquisition until architecture-aware artifact
selection lands in core.

Each release also contains the exact pinned release `service.json`,
`SHA256SUMS.txt`, `SBOM.cdx.json` and GitHub build-provenance attestations.
Every platform archive includes `BUILD-IDENTITY.json`; native smoke tests also
assert that `/version` reports the same full commit SHA before publication.

## HTTP API

- `GET /health/live` reports process/event-loop liveness.
- `GET /health/ready` is healthy only when every discovered model parses and
  compiles.
- `GET /version` reports wrapper, engine and build identity.
- `GET /v1/decisions` returns decision IDs and validation status only.
- `POST /v1/decisions/{id}/evaluate` evaluates a JSON context.
- `POST /v1/decisions/reload` atomically rescans and compiles the workspace.

Example:

```bash
curl --fail http://127.0.0.1:18089/health/ready
curl --fail \
  -H 'content-type: application/json' \
  --data '{"customer":{"tier":"gold"}}' \
  http://127.0.0.1:18089/v1/decisions/pricing/evaluate
```

Evaluation responses contain `decisionId`, `result` and a correlation ID. Error
responses use a stable bounded envelope and do not return JDM source, host paths
or input/output values.

## Decision workspace

Put JDM JSON files under `decisions/`. A file such as
`decisions/commercial/pricing.json` is exposed as decision ID
`commercial/pricing`; nested Decision nodes continue to resolve the loader key
`commercial/pricing.json` within the same compiled registry.

Reload is transactional. A new registry is parsed and compiled away from live
traffic, then swapped in one operation. If any file is invalid, readiness stays
healthy on the last valid registry and the new registry is rejected with bounded
diagnostics. An invalid registry at process startup leaves liveness healthy and
readiness unhealthy.

The archive includes one non-production `decisions/example.json` model so a new
installation can be tested immediately. Releases never include operator models,
logs or generated state.

Service-owned paths are relocatable beneath `SERVICE_ROOT`:

- `decisions/` is the operator-managed read/write JDM workspace.
- `config/` is generated non-secret configuration.
- `logs/` is runtime logging output.
- `.state/registry.json` contains only generation, count, timestamp and engine
  version metadata.

Traversal, symbolic links, non-JSON files, invalid IDs and paths outside
`SERVICE_ROOT` are rejected.

## Security and limits

The default configuration:

- binds to loopback only;
- disables outbound ZEN HTTP adapters;
- retains ZEN's QuickJS execution boundary and sets its function timeout to the
  service evaluation timeout;
- disables evaluation traces;
- limits request bytes, JSON nesting, model bytes, total registry bytes, model
  count, evaluation depth and concurrent evaluations;
- logs correlation ID, decision ID, duration and outcome class only.

| Environment variable | Default |
| --- | ---: |
| `ZEN_BRE_MAX_BODY_BYTES` | `1048576` |
| `ZEN_BRE_MAX_MODEL_BYTES` | `4194304` |
| `ZEN_BRE_MAX_TOTAL_MODEL_BYTES` | `67108864` |
| `ZEN_BRE_MAX_MODELS` | `1000` |
| `ZEN_BRE_MAX_CONCURRENCY` | `32` |
| `ZEN_BRE_EVALUATION_TIMEOUT_MS` | `5000` |
| `ZEN_BRE_MAX_DEPTH` | `10` |
| `ZEN_BRE_MAX_JSON_DEPTH` | `64` |

Non-loopback binding requires the explicit `--allow-non-loopback` option. The
initial service does not provide authentication or TLS, so applications should
keep it local or place an authenticated local proxy in front of it.

## Local development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
bash ./scripts/package.sh
bash ./scripts/smoke.sh
```

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
.\scripts\package.ps1
.\scripts\smoke.ps1
```

Run from source:

```bash
cargo run -- \
  --service-root "$PWD/runtime" \
  --host 127.0.0.1 \
  --port 18089 \
  --decisions-dir "$PWD/runtime/decisions"
```

## Upgrades and rollback

`zen-engine` is locked at build time and never floats at runtime. Advancing a
beta engine version requires the full fixture suite, precision regression,
native package smoke tests, Service Lasso harness gates and release-asset
verification. A consuming application should pin the released `service.json`
and retain its previous archive/tag so Service Lasso rollback can restore the
last verified package.

The repository's development manifest tracks the latest release for maintainer
testing. The `service.json` attached to each release is rewritten and verified
as a self-contained manifest pinned to that exact release tag.
