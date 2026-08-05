# Changelog

## 0.1.0

- embed `zen-engine` 1.0.0-beta.11 with arbitrary precision enabled
- add file-backed compiled decision registry and nested Decision-node loading
- add live, ready, version, inventory, evaluate and atomic reload APIs
- add bounded request, model, concurrency, timeout and depth controls
- add structured no-value logging and stable redacted error envelopes
- add canonical Service Lasso endpoint, workspace, health and artifact contract
- add Windows x64, Linux x64/ARM64 and macOS x64/ARM64 release packages
- add runtime-checked and archive-contained build identity metadata
- add checksums, CycloneDX SBOM, provenance and post-publication verification
- pin every archive's embedded `service.json` to its release tag and require it
  to match the attached manifest byte-for-byte
- remove unsupported descriptive custom-action modes so the released manifest
  validates against the current Service Lasso core contract
