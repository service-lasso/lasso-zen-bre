#!/usr/bin/env python3
import hashlib
import json
import pathlib
import struct
import sys
import tarfile
import zipfile


if len(sys.argv) != 4:
    raise SystemExit("usage: verify_release.py <assets-dir> <release-tag> <commit-sha>")

assets_dir = pathlib.Path(sys.argv[1])
release_tag = sys.argv[2]
commit_sha = sys.argv[3]
engine_version = "1.0.0-beta.11"
archives = {
    f"lasso-zen-bre-{engine_version}-windows-x64.zip": ("pe", "x64", "lasso-zen-bre.exe", "x86_64-pc-windows-msvc"),
    f"lasso-zen-bre-{engine_version}-linux-x64.tar.gz": ("elf", "x64", "lasso-zen-bre", "x86_64-unknown-linux-gnu"),
    f"lasso-zen-bre-{engine_version}-linux-arm64.tar.gz": ("elf", "arm64", "lasso-zen-bre", "aarch64-unknown-linux-gnu"),
    f"lasso-zen-bre-{engine_version}-macos-x64.tar.gz": ("macho", "x64", "lasso-zen-bre", "x86_64-apple-darwin"),
    f"lasso-zen-bre-{engine_version}-macos-arm64.tar.gz": ("macho", "arm64", "lasso-zen-bre", "aarch64-apple-darwin"),
}


def fail(message: str) -> None:
    raise SystemExit(message)


def normalize(name: str) -> str:
    return name.replace("\\", "/").removeprefix("./").rstrip("/")


def read_archive(path: pathlib.Path) -> tuple[set[str], dict[str, bytes]]:
    files: dict[str, bytes] = {}
    names: set[str] = set()
    if path.suffix == ".zip":
        with zipfile.ZipFile(path) as archive:
            for item in archive.infolist():
                name = normalize(item.filename)
                if not name:
                    continue
                names.add(name)
                if not item.is_dir():
                    files[name] = archive.read(item)
    else:
        with tarfile.open(path, "r:gz") as archive:
            for item in archive.getmembers():
                name = normalize(item.name)
                if not name:
                    continue
                names.add(name)
                if item.isfile():
                    extracted = archive.extractfile(item)
                    if extracted is None:
                        fail(f"could not read {name} from {path.name}")
                    files[name] = extracted.read()
    return names, files


def verify_binary(data: bytes, file_format: str, architecture: str, name: str) -> None:
    if engine_version.encode() not in data:
        fail(f"{name} does not embed the pinned engine version")
    if file_format == "pe":
        if data[:2] != b"MZ" or len(data) < 64:
            fail(f"{name} is not a PE executable")
        offset = struct.unpack_from("<I", data, 0x3C)[0]
        machine = struct.unpack_from("<H", data, offset + 4)[0]
        if machine != 0x8664 or architecture != "x64":
            fail(f"{name} has an unexpected PE architecture")
        return
    if file_format == "elf":
        if data[:4] != b"\x7fELF" or data[4] != 2:
            fail(f"{name} is not a 64-bit ELF executable")
        endian = "<" if data[5] == 1 else ">"
        machine = struct.unpack_from(f"{endian}H", data, 18)[0]
        expected = {"x64": 62, "arm64": 183}[architecture]
        if machine != expected:
            fail(f"{name} has an unexpected ELF architecture")
        return
    if file_format == "macho":
        if data[:4] == b"\xcf\xfa\xed\xfe":
            endian = "<"
        elif data[:4] == b"\xfe\xed\xfa\xcf":
            endian = ">"
        else:
            fail(f"{name} is not a 64-bit Mach-O executable")
        cpu_type = struct.unpack_from(f"{endian}I", data, 4)[0]
        expected = {"x64": 0x01000007, "arm64": 0x0100000C}[architecture]
        if cpu_type != expected:
            fail(f"{name} has an unexpected Mach-O architecture")
        return
    fail(f"unknown executable format for {name}")


required_archive_files = {
    "service.json",
    "LICENSE",
    "NOTICE",
    "THIRD_PARTY_LICENSES/zen-engine-MIT.txt",
    "README.md",
    "BUILD-IDENTITY.json",
    "decisions/example.json",
}
released_service_path = assets_dir / "service.json"
if not released_service_path.is_file():
    fail("missing released service.json")
released_service_bytes = released_service_path.read_bytes()
service = json.loads(released_service_bytes)
source = service.get("artifact", {}).get("source", {})
if source.get("tag") != release_tag or "channel" in source:
    fail("released service.json is not pinned to its release tag")
if service.get("updates", {}).get("mode") != "disabled":
    fail("released service.json must remain pinned")

for filename, (file_format, architecture, binary_name, target_triple) in archives.items():
    path = assets_dir / filename
    if not path.is_file():
        fail(f"missing release archive: {filename}")
    names, files = read_archive(path)
    required = required_archive_files | {binary_name}
    missing = required - names
    if missing:
        fail(f"{filename} is missing: {', '.join(sorted(missing))}")
    forbidden = [
        name
        for name in files
        if name.startswith("logs/")
        or name.startswith(".state/")
        or (name.startswith("decisions/") and name != "decisions/example.json")
    ]
    if forbidden:
        fail(f"{filename} contains runtime/operator data: {forbidden}")
    verify_binary(files[binary_name], file_format, architecture, filename)
    if files["service.json"] != released_service_bytes:
        fail(f"{filename} does not embed the exact released service.json")
    embedded = json.loads(files["service.json"])
    if embedded.get("version") != "0.1.0":
        fail(f"{filename} embeds the wrong wrapper version")
    if embedded.get("meta", {}).get("upstream", {}).get("version") != engine_version:
        fail(f"{filename} embeds the wrong engine manifest version")
    build = json.loads(files["BUILD-IDENTITY.json"])
    if build != {
        "service": "zen-bre",
        "wrapperVersion": "0.1.0",
        "engineVersion": engine_version,
        "buildIdentity": commit_sha,
        "targetTriple": target_triple,
    }:
        fail(f"{filename} contains invalid build identity metadata")

sbom = json.loads((assets_dir / "SBOM.cdx.json").read_text())
components = {(item.get("name"), item.get("version")) for item in sbom.get("components", [])}
if ("zen-engine", engine_version) not in components:
    fail("SBOM does not identify the pinned zen-engine")

checksums = {}
for line in (assets_dir / "SHA256SUMS.txt").read_text().splitlines():
    if not line.strip():
        continue
    digest, filename = line.split(maxsplit=1)
    checksums[pathlib.Path(filename.lstrip("*")).name] = digest.lower()
for filename in [*archives, "service.json", "SBOM.cdx.json"]:
    expected = checksums.get(filename)
    if expected is None:
        fail(f"SHA256SUMS.txt is missing {filename}")
    actual = hashlib.sha256((assets_dir / filename).read_bytes()).hexdigest()
    if actual != expected:
        fail(f"checksum mismatch for {filename}")

print("release asset verification passed")
