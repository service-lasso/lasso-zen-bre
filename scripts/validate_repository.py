#!/usr/bin/env python3
import json
import pathlib
import re


ROOT = pathlib.Path(__file__).resolve().parent.parent
ENGINE_VERSION = "1.0.0-beta.11"
PRIMARY_ASSETS = {
    "win32": f"lasso-zen-bre-{ENGINE_VERSION}-windows-x64.zip",
    "linux": f"lasso-zen-bre-{ENGINE_VERSION}-linux-x64.tar.gz",
    "darwin": f"lasso-zen-bre-{ENGINE_VERSION}-macos-arm64.tar.gz",
}
ALL_ASSETS = {
    *PRIMARY_ASSETS.values(),
    f"lasso-zen-bre-{ENGINE_VERSION}-linux-arm64.tar.gz",
    f"lasso-zen-bre-{ENGINE_VERSION}-macos-x64.tar.gz",
}


def fail(message: str) -> None:
    raise SystemExit(message)


required = [
    "Cargo.toml",
    "Cargo.lock",
    "service.json",
    "src/lib.rs",
    "src/main.rs",
    "verify/service-harness.json",
    "examples/decisions/example.json",
    "LICENSE",
    "NOTICE",
    "THIRD_PARTY_LICENSES/zen-engine-MIT.txt",
    "scripts/write_build_metadata.py",
    "README.md",
]
for relative in required:
    if not (ROOT / relative).is_file():
        fail(f"missing required file: {relative}")

service = json.loads((ROOT / "service.json").read_text())
if service.get("id") != "zen-bre" or service.get("enabled") is not False:
    fail("service identity or disabled-by-default policy is invalid")
if service.get("version") != "0.1.0":
    fail("wrapper version is not 0.1.0")
upstream = service.get("meta", {}).get("upstream", {})
if upstream.get("crate") != "zen-engine" or upstream.get("version") != ENGINE_VERSION:
    fail("upstream zen-engine pin is invalid")

endpoints = service.get("endpoints")
if not isinstance(endpoints, list) or not endpoints:
    fail("canonical endpoints[] is required")
endpoint_ids = {item.get("id") for item in endpoints}
if endpoint_ids != {"http", "base_url", "live", "ready"}:
    fail("endpoint contract is incomplete")
network = next(item for item in endpoints if item.get("id") == "http")
if network.get("kind") != "network" or network.get("port", {}).get("strategy") != "preferred":
    fail("HTTP endpoint must use canonical preferred allocation")
if network.get("bind") != "127.0.0.1" or network.get("exposure") != "local":
    fail("HTTP endpoint must remain loopback-only by default")

env = service.get("env", {})
globalenv = service.get("globalenv", {})
for key in ("ZEN_BRE_URL", "ZEN_BRE_DECISIONS_DIR", "ZEN_BRE_VERSION"):
    if key not in globalenv:
        fail(f"missing global export: {key}")
if env.get("ZEN_BRE_PORT") != "${endpoint.http.port}":
    fail("runtime port must come from the resolved endpoint")
if "env" in service.get("execconfig", {}) or "globalenv" in service.get("execconfig", {}):
    fail("env/globalenv must use the canonical top-level contract")

if "healthcheck" in service or "healthcheck" in service.get("execconfig", {}):
    fail("singular healthcheck is not allowed")
healthchecks = service.get("healthchecks")
if not isinstance(healthchecks, list) or healthchecks[0].get("url") != "${endpoint.ready.url}":
    fail("readiness healthcheck is invalid")

files = service.get("files", {})
if files.get("enabled") is not True:
    fail("files workspace must be enabled")
for root in files.get("roots", []):
    path = pathlib.PurePosixPath(root.get("path", ""))
    if path.is_absolute() or ".." in path.parts:
        fail("files roots must remain relative and traversal-free")

artifact = service.get("artifact", {})
if artifact.get("kind") != "archive":
    fail("artifact kind must be archive")
source = artifact.get("source", {})
if source.get("repo") != "service-lasso/lasso-zen-bre" or source.get("channel") != "latest":
    fail("repository manifest must track the latest verified release")
platforms = artifact.get("platforms", {})
for platform, asset in PRIMARY_ASSETS.items():
    definition = platforms.get(platform, {})
    if definition.get("assetName") != asset:
        fail(f"artifact name mismatch for {platform}")
    checksum = definition.get("checksum", {})
    if checksum != {"algorithm": "sha256", "assetName": "SHA256SUMS.txt"}:
        fail(f"checksum contract mismatch for {platform}")

cargo = (ROOT / "Cargo.toml").read_text()
expected = r'zen-engine = \{ version = "=1\.0\.0-beta\.11", features = \["arbitrary_precision"\] \}'
if not re.search(expected, cargo):
    fail("Cargo.toml must pin zen-engine with arbitrary_precision")
lock = (ROOT / "Cargo.lock").read_text()
if 'name = "zen-engine"' not in lock or f'version = "{ENGINE_VERSION}"' not in lock:
    fail("Cargo.lock does not contain the pinned engine")

contract = json.loads((ROOT / "verify/service-harness.json").read_text())
if contract.get("serviceId") != "zen-bre":
    fail("harness service identity mismatch")

release = (ROOT / ".github/workflows/release.yml").read_text()
for asset in ALL_ASSETS:
    if asset not in release:
        fail(f"release workflow does not publish {asset}")
for required_release_output in ("SHA256SUMS.txt", "SBOM.cdx.json", "attest-build-provenance"):
    if required_release_output not in release:
        fail(f"release workflow is missing {required_release_output}")

for package_script in ("scripts/package.sh", "scripts/package.ps1"):
    if "BUILD-IDENTITY.json" not in (ROOT / package_script).read_text():
        fail(f"{package_script} does not package build identity metadata")

print("repository contract validation passed")
