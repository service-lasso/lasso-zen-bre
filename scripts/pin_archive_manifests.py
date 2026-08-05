#!/usr/bin/env python3
import copy
import io
import pathlib
import sys
import tarfile
import tempfile
import zipfile


if len(sys.argv) != 3:
    raise SystemExit(
        "usage: pin_archive_manifests.py <assets-dir> <pinned-service.json>"
    )

assets_dir = pathlib.Path(sys.argv[1])
manifest_path = pathlib.Path(sys.argv[2])
manifest = manifest_path.read_bytes()
engine_version = "1.0.0-beta.11"
archives = [
    f"lasso-zen-bre-{engine_version}-windows-x64.zip",
    f"lasso-zen-bre-{engine_version}-linux-x64.tar.gz",
    f"lasso-zen-bre-{engine_version}-linux-arm64.tar.gz",
    f"lasso-zen-bre-{engine_version}-macos-x64.tar.gz",
    f"lasso-zen-bre-{engine_version}-macos-arm64.tar.gz",
]


def pin_zip(path: pathlib.Path) -> None:
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as temporary:
        temporary_path = pathlib.Path(temporary.name)
    found = False
    try:
        with zipfile.ZipFile(path, "r") as source, zipfile.ZipFile(
            temporary_path, "w"
        ) as target:
            for item in source.infolist():
                data = source.read(item) if not item.is_dir() else b""
                normalized = item.filename.replace("\\", "/").removeprefix("./")
                if normalized == "service.json":
                    data = manifest
                    found = True
                target.writestr(item, data)
        if not found:
            raise SystemExit(f"{path.name} does not contain root service.json")
        temporary_path.replace(path)
    finally:
        temporary_path.unlink(missing_ok=True)


def pin_tar(path: pathlib.Path) -> None:
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as temporary:
        temporary_path = pathlib.Path(temporary.name)
    found = False
    try:
        with tarfile.open(path, "r:gz") as source, tarfile.open(
            temporary_path, "w:gz"
        ) as target:
            for source_item in source.getmembers():
                item = copy.copy(source_item)
                normalized = item.name.replace("\\", "/").removeprefix("./")
                if normalized == "service.json":
                    item.size = len(manifest)
                    target.addfile(item, io.BytesIO(manifest))
                    found = True
                elif item.isfile():
                    extracted = source.extractfile(source_item)
                    if extracted is None:
                        raise SystemExit(f"could not read {item.name} from {path.name}")
                    target.addfile(item, extracted)
                else:
                    target.addfile(item)
        if not found:
            raise SystemExit(f"{path.name} does not contain root service.json")
        temporary_path.replace(path)
    finally:
        temporary_path.unlink(missing_ok=True)


for archive_name in archives:
    archive_path = assets_dir / archive_name
    if not archive_path.is_file():
        raise SystemExit(f"missing release archive: {archive_name}")
    if archive_path.suffix == ".zip":
        pin_zip(archive_path)
    else:
        pin_tar(archive_path)

print("embedded release manifests pinned")
