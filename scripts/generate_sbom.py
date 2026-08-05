#!/usr/bin/env python3
import json
import pathlib
import subprocess
import sys
import uuid


if len(sys.argv) != 2:
    raise SystemExit("usage: generate_sbom.py <output>")

metadata = json.loads(
    subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1"],
        text=True,
    )
)
packages = sorted(metadata["packages"], key=lambda item: (item["name"], item["version"]))
components = []
for package in packages:
    component = {
        "type": "library",
        "bom-ref": package["id"],
        "name": package["name"],
        "version": package["version"],
        "purl": f"pkg:cargo/{package['name']}@{package['version']}",
    }
    if package.get("license"):
        component["licenses"] = [{"expression": package["license"]}]
    if package.get("source"):
        component["externalReferences"] = [
            {"type": "distribution", "url": package["source"]}
        ]
    components.append(component)

document = {
    "bomFormat": "CycloneDX",
    "specVersion": "1.6",
    "serialNumber": f"urn:uuid:{uuid.uuid4()}",
    "version": 1,
    "metadata": {
        "component": {
            "type": "application",
            "name": "lasso-zen-bre",
            "version": "0.1.0",
            "purl": "pkg:github/service-lasso/lasso-zen-bre@0.1.0",
        },
        "properties": [
            {"name": "service-lasso:embedded-engine", "value": "zen-engine@1.0.0-beta.11"},
            {"name": "service-lasso:arbitrary-precision", "value": "true"},
        ],
    },
    "components": components,
}
output = pathlib.Path(sys.argv[1])
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n")
