#!/usr/bin/env python3
"""Prepare checked assets and build the ARM64 native application offline."""

import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tarfile
import tomllib

import build_nearby_gui as gui
import build_paths

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "native/nearby/Cargo.toml"
ASSETS = ROOT / "build/assets"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def prepare(archive):
    expected = json.loads((ROOT / "release/assets.json").read_text())
    data = archive.read_bytes()
    if sha(data) != expected["sha256"]:
        raise ValueError("Build asset archive checksum differs")
    manifest = json.loads((ROOT / "release/manifest.json").read_text())
    names = {entry["name"]: entry for entry in manifest["files"]}
    with tarfile.open(fileobj=io.BytesIO(data)) as source:
        members = source.getmembers()
        if len(members) != len(names) or {m.name for m in members} != set(names):
            raise ValueError("Build asset archive member set differs")
        verified = {}
        for member in members:
            entry = names[member.name]
            if not member.isfile() or member.size != entry["size"]:
                raise ValueError("Build asset archive type or size differs")
            payload = source.extractfile(member).read()
            if sha(payload) != entry["sha256"]:
                raise ValueError("Build asset checksum differs: " + member.name)
            verified[member.name] = (payload, entry["mode"])
    for name, (payload, mode) in verified.items():
        target = ASSETS / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(payload)
        target.chmod(mode)
    print(json.dumps({"prepared_assets": len(verified)}))


def bundle():
    manifest = json.loads((ROOT / "release/manifest.json").read_text())
    manifest["version"] = tomllib.loads(MANIFEST.read_text())["package"]["version"]
    report = json.loads((ASSETS / "nearby-gui-build.json").read_text())
    if report["source_sha256"] != gui.source_sha256():
        raise ValueError("Renderer source differs; build the GUI before bundling")
    contents = bytearray()
    for entry in manifest["files"]:
        data = (ASSETS / entry["name"]).read_bytes()
        if entry["name"] == "nearby-gui-build.json":
            data = json.dumps(report).encode()
        if entry["name"] in {"arkos-nearby-gui", "nearby-gui-build.json"}:
            if entry["name"] == "arkos-nearby-gui" and sha(data) != report["sha256"]:
                raise ValueError("Renderer bytes differ from their build report")
            entry.update(sha256=sha(data), size=len(data))
        elif sha(data) != entry["sha256"] or len(data) != entry["size"]:
            raise ValueError("Pinned native asset differs: " + entry["name"])
        entry["offset"] = len(contents)
        contents.extend(data)
    header = json.dumps(manifest, separators=(",", ":")).encode()
    packed = gzip.compress(
        b"ARKNP001" + len(header).to_bytes(4, "little") + header + contents, mtime=0
    )
    output = build_paths.BUILD_ROOT / "release/bundle.gz"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes(packed)
    return output


def native(check):
    cargo, environment = gui.toolchain("1.92")
    environment.update(
        CARGO_TARGET_DIR=str(build_paths.target("release")),
        ARKOS_NEARBY_BUNDLE=str(bundle()),
    )
    common = [
        "--manifest-path",
        str(MANIFEST),
        "--locked",
        "--offline",
        "--features",
        "release-bundle",
    ]
    if check:
        gui.run(
            [str(cargo), "fmt", "--manifest-path", str(MANIFEST), "--check"],
            environment,
        )
        gui.run(
            [str(cargo), "clippy", *common, "--all-targets", "--", "-D", "warnings"],
            environment,
        )
        gui.run([str(cargo), "test", *common], environment)
        gui.run(
            [
                str(cargo),
                "clippy",
                *common,
                "--target",
                gui.TRIPLE,
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
            gui.linux_environment(cargo, environment),
        )
    else:
        gui.run(
            [str(cargo), "build", *common, "--target", gui.TRIPLE, "--release"],
            gui.linux_environment(cargo, environment),
        )
        binary = build_paths.target("release") / gui.TRIPLE / "release/arkos-nearby"
        output = ROOT / "dist"
        output.mkdir(exist_ok=True)
        data = binary.read_bytes()
        (output / "arkos-nearby").write_bytes(data)
        (output / "arkos-nearby").chmod(0o755)
        (output / "SHA256SUMS").write_text(sha(data) + "  arkos-nearby\n")
        print(
            json.dumps({"output": str(output), "sha256": sha(data), "bytes": len(data)})
        )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("prepare", "build", "check"))
    parser.add_argument("archive", type=Path, nargs="?")
    args = parser.parse_args()
    if args.action == "prepare":
        if args.archive is None:
            parser.error("prepare requires the downloaded build asset archive")
        prepare(args.archive)
        return
    subprocess.run(
        [
            "python3",
            str(ROOT / "scripts/build_nearby_gui.py"),
            "check" if args.action == "check" else "build",
        ],
        check=True,
    )
    native(args.action == "check")


if __name__ == "__main__":
    main()
