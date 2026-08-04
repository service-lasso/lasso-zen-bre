# lasso-zen-bre

_Status: initial Service Lasso wrapper bootstrap_

`lasso-zen-bre` packages GoRules ZEN as a portable, local-first Service Lasso
managed decision service. The repository was created from
`service-lasso/service-template` and keeps `develop` as the developer baseline.

ZEN is embedded through the Rust `zen-engine` crate. The service owns the HTTP
host, packaging, Service Lasso manifest, health contract, workspace layout,
logging defaults, and release artifacts.

## Service Identity

- Service ID: `zen-bre`
- Display name: `GoRules ZEN Business Rules Engine`
- Short name: `ZEN BRE`
- Repository: `service-lasso/lasso-zen-bre`
- Upstream engine: `zen-engine = 1.0.0-beta.11`
- Upstream licence: MIT

## Initial HTTP Contract

- `GET /health/live`
- `GET /health/ready`
- `GET /version`
- `GET /v1/decisions`
- `POST /v1/decisions/{id}/evaluate`
- `POST /v1/decisions/reload`

The initial bootstrap exposes health, version, and safe decision inventory
metadata. Evaluation is intentionally bounded until the first engine-backed
registry implementation lands.

## Workspace

Service-owned paths are relocatable under `SERVICE_ROOT`:

- `decisions/` for operator-managed JDM JSON files
- `config/` for generated non-secret configuration
- `logs/` for structured logs
- `.state/` for registry/reload metadata only

Decision IDs are derived from filenames inside `decisions/`; IDs are not
interpreted as arbitrary filesystem paths.

## Local Development

```powershell
cargo test --locked
pwsh -NoLogo -NoProfile -File .\scripts\test.ps1
pwsh -NoLogo -NoProfile -File .\scripts\package.ps1
```

```bash
cargo test --locked
bash ./scripts/test.sh
bash ./scripts/package.sh
```

Run locally:

```powershell
cargo run -- --host 127.0.0.1 --port 18089 --decisions-dir .\decisions
```

The service must not log decision input, output, JDM source, generated config
contents, or secret-like values by default.
