#!/usr/bin/env python3
"""Package a checked ARM64 executable for installation."""

import argparse
import hashlib
import json
from pathlib import Path
import tomllib

import release_archives

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
    manifest = json.loads((ROOT / "release/manifest.json").read_text())
    archive = release_archives.write(
        args.output,
        ROOT / "release",
        data,
        version=version,
        target="aarch64-unknown-linux-musl",
        profile=manifest["profile"],
        kernel_sha256=manifest["kernel_sha256"],
    )
    print(json.dumps(archive))


if __name__ == "__main__":
    main()
