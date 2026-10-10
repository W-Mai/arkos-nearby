#!/usr/bin/env python3
"""Build the ARM64 arcade load probe with existing pinned runtime libraries."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from build_core_inspect import compile_flags, linker, validate_libraries

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "native/core_probe"
LIBRARIES = ROOT / "build/native-libs"
ASSETS = ROOT / "build/assets"
NAME = "core-probe64"
REPORT = "core-probe-build.json"
TARGET = "aarch64-linux-gnu"
LOADER = "/lib/ld-linux-aarch64.so.1"


def source_sha256():
    value = hashlib.sha256()
    for path in sorted(SOURCE.iterdir()):
        if path.is_file():
            value.update(path.name.encode() + b"\0" + path.read_bytes())
    return value.hexdigest()


def artifacts(directory=ASSETS):
    report = json.loads((directory / REPORT).read_text())
    data = (directory / NAME).read_bytes()
    if (
        report.get("schema") != 1
        or report.get("source_sha256") != source_sha256()
        or report.get("mode") != "load-only-rollback"
        or report.get("target") != TARGET
        or report.get("loader") != LOADER
        or report.get("bytes") != len(data)
        or report.get("sha256") != hashlib.sha256(data).hexdigest()
        or data[:6] != b"\x7fELF\x02\x01"
        or int.from_bytes(data[18:20], "little") != 183
    ):
        raise ValueError("The arcade load probe does not match its source or build")
    return {
        NAME: data,
        REPORT: (directory / REPORT).read_bytes(),
        "licenses/core-probe-ABI-LICENSE.txt": (
            SOURCE / "ABI-LICENSE.txt"
        ).read_bytes(),
    }


def build(compiler, libraries, output, toolchain):
    pins = validate_libraries(libraries)
    flags = compile_flags(compiler, 64, TARGET)
    with tempfile.TemporaryDirectory(prefix="arkos-core-probe-") as temporary:
        directory = Path(temporary)
        obj = directory / "probe.o"
        binary = directory / NAME
        subprocess.run(
            [*flags, "-c", str(SOURCE / "probe.c"), "-o", str(obj)], check=True
        )
        subprocess.run(
            [
                str(linker(toolchain)),
                "-flavor",
                "gnu",
                "-m",
                "aarch64linux",
                "-e",
                "_start",
                "--dynamic-linker",
                LOADER,
                "--allow-shlib-undefined",
                "-z",
                "noexecstack",
                "-o",
                str(binary),
                str(libraries / "arm64-crt1.o"),
                str(libraries / "arm64-crti.o"),
                str(obj),
                str(libraries / "arm64-libdl.so.2"),
                str(libraries / "arm64-libc.so.6"),
                str(libraries / "arm64-libc_nonshared.a"),
                str(libraries / "arm64-crtn.o"),
            ],
            check=True,
        )
        data = binary.read_bytes()
    report = {
        "schema": 1,
        "source_sha256": source_sha256(),
        "mode": "load-only-rollback",
        "target": TARGET,
        "loader": LOADER,
        "toolchain": toolchain,
        "runtime_pins": {
            name: pin for name, pin in pins.items() if name.startswith("arm64-")
        },
        "bytes": len(data),
        "sha256": hashlib.sha256(data).hexdigest(),
    }
    output.mkdir(parents=True, exist_ok=True)
    (output / NAME).write_bytes(data)
    (output / NAME).chmod(0o755)
    (output / REPORT).write_text(json.dumps(report, indent=2) + "\n")
    artifacts(output)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("check", "build"))
    parser.add_argument("--libraries", type=Path, default=LIBRARIES)
    parser.add_argument("--output", type=Path, default=ASSETS)
    parser.add_argument("--toolchain", default="1.92")
    args = parser.parse_args()
    compiler = shutil.which(os.environ.get("CC", "clang"))
    if compiler is None:
        raise RuntimeError("An existing clang compiler is required")
    if args.action == "check":
        subprocess.run(
            [
                *compile_flags(compiler, 64, TARGET),
                "-fsyntax-only",
                str(SOURCE / "probe.c"),
            ],
            check=True,
        )
        if "retro_run" in (SOURCE / "probe.c").read_text():
            raise ValueError("The load probe must not resolve or advance frames")
        print("ARM64 load-only probe source checks passed")
    else:
        print(json.dumps(build(compiler, args.libraries, args.output, args.toolchain)))


if __name__ == "__main__":
    main()
