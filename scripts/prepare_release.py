#!/usr/bin/env python3
import json
import pathlib
import sys


if len(sys.argv) != 4:
    raise SystemExit("usage: prepare_release.py <service.json> <tag> <output>")

source = pathlib.Path(sys.argv[1])
tag = sys.argv[2]
output = pathlib.Path(sys.argv[3])
document = json.loads(source.read_text())
artifact_source = document["artifact"]["source"]
artifact_source.pop("channel", None)
artifact_source["tag"] = tag
document["updates"] = {
    "enabled": False,
    "mode": "disabled",
    "track": "pinned",
}
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(json.dumps(document, indent=2) + "\n")
