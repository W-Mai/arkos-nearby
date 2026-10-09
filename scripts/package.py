#!/usr/bin/env python3
"""Package a checked ARM64 executable with installation docs and licenses."""

import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--expected-sha256", required=True)
    parser.add_argument("--output", type=Path, default=ROOT / "dist")
    args = parser.parse_args()
    data = args.binary.read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    if digest != args.expected_sha256:
        raise ValueError("The executable differs from its verified checksum")
    if data[:6] != b"\x7fELF\x02\x01" or int.from_bytes(data[18:20], "little") != 183:
        raise ValueError("The package requires a little-endian ARM64 ELF executable")
    version = tomllib.loads((ROOT / "native/nearby/Cargo.toml").read_text())["package"][
        "version"
    ]
    entries = {
        "arkos-nearby": (data, 0o755),
        "Install Nearby Multiplayer.sh": (
            (ROOT / "release/Install Nearby Multiplayer.sh").read_bytes(),
            0o755,
        ),
        "README.md": ((ROOT / "README.md").read_bytes(), 0o644),
        "LICENSE": ((ROOT / "LICENSE").read_bytes(), 0o644),
        "SHA256SUMS": ((digest + "  arkos-nearby\n").encode(), 0o644),
    }
    for path in (ROOT / "docs").rglob("*"):
        if path.is_file():
            entries[path.relative_to(ROOT).as_posix()] = (path.read_bytes(), 0o644)
    for name in ("OFL.txt", "font-source.json"):
        relative = "gui/nearby/assets/" + name
        entries[relative] = ((ROOT / relative).read_bytes(), 0o644)
    for name in (
        "provenance.json",
        "manifest.json",
        "rust-dependencies.json",
        "third-party-sources.json",
        "release-sources.json",
        "source.json",
        "assets.json",
    ):
        entries["release/" + name] = ((ROOT / "release" / name).read_bytes(), 0o644)
    args.output.mkdir(parents=True, exist_ok=True)
    path = args.output / f"arkos-nearby-{version}-arm64.tar.gz"
    with (
        path.open("wb") as output,
        gzip.GzipFile(fileobj=output, mode="wb", mtime=0, filename="") as compressed,
        tarfile.open(fileobj=compressed, mode="w") as package,
    ):
        for name, (payload, mode) in sorted(entries.items()):
            member = tarfile.TarInfo("arkos-nearby/" + name)
            member.size = len(payload)
            member.mode = mode
            member.mtime = 0
            package.addfile(member, io.BytesIO(payload))
    print(
        json.dumps(
            {
                "archive": str(path),
                "executable_sha256": digest,
                "archive_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                "files": len(entries),
            }
        )
    )


if __name__ == "__main__":
    main()
