#!/usr/bin/env python3
import json
import os
import pathlib
import re
import sys


if len(sys.argv) != 3:
    raise SystemExit("usage: write_build_metadata.py <output> <target-triple>")

build_identity = os.environ.get("LASSO_ZEN_BRE_BUILD_SHA", "development")
if build_identity != "development" and not re.fullmatch(r"[0-9a-f]{40}", build_identity):
    raise SystemExit("LASSO_ZEN_BRE_BUILD_SHA must be a full lowercase commit SHA")

document = {
    "service": "zen-bre",
    "wrapperVersion": "0.1.0",
    "engineVersion": "1.0.0-beta.11",
    "buildIdentity": build_identity,
    "targetTriple": sys.argv[2],
}
output = pathlib.Path(sys.argv[1])
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(json.dumps(document, indent=2) + "\n")
