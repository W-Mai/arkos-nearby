#!/usr/bin/env python3
"""Build and validate the scoped ARM64 Netplay compression helper offline."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import struct
import subprocess

from build_core_inspect import linker, validate_libraries
from build_paths import BUILD_ROOT

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "native/netplay_compression"
CACHE = BUILD_ROOT / "netplay-compression"
ASSETS = ROOT / "build/assets/assets"
FRONTEND_SHA = "81f3cf1b8850f9457730346d3f973dfb03d5316d62c4bb92b1654372bbc86e77"
BINARY_SHA = "381d05d1ff68ff5f4eca503965f35a6fe99922e280f4e52620e68b21dca0d633"
NAME = "netplay-compression.so"
REPORT = "netplay-compression-build.json"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def source_sha256(source=SOURCE):
    digest = hashlib.sha256()
    for name in (
        "compression.c",
        "util.h",
        "LICENSE",
        "ABI-COPYING.txt",
        "abi-source.json",
    ):
        digest.update(name.encode() + b"\0" + (source / name).read_bytes())
    return digest.hexdigest()


def artifacts(assets=ASSETS, source=SOURCE):
    data = (assets / NAME).read_bytes()
    report = json.loads((assets / REPORT).read_text())
    if (
        data[:6] != b"\x7fELF\x02\x01"
        or struct.unpack_from("<H", data, 18)[0] != 183
        or report["sha256"] != sha(data)
        or sha(data) != BINARY_SHA
        or report["bytes"] != len(data)
        or report["source_sha256"] != source_sha256(source)
        or report["frontend_sha256"] != FRONTEND_SHA
        or report["needed"] != ["libc.so.6"]
        or report["maximum_glibc"] != "2.17"
    ):
        raise ValueError("Netplay compression asset differs")
    return {
        "assets/" + NAME: data,
        "assets/" + REPORT: (assets / REPORT).read_bytes(),
        "licenses/netplay-compression-COPYING.txt": (source / "LICENSE").read_bytes()
        + b"\n"
        + (source / "ABI-COPYING.txt").read_bytes(),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("build", "check"))
    parser.add_argument("--source", type=Path, default=SOURCE)
    parser.add_argument("--assets", type=Path, default=ASSETS)
    parser.add_argument("--sysroot", type=Path, default=CACHE / "sysroot")
    parser.add_argument("--libraries", type=Path, default=ROOT / "build/native-libs")
    parser.add_argument("--compiler", default="clang")
    parser.add_argument(
        "--readelf", default=shutil.which("llvm-readelf") or shutil.which("readelf")
    )
    args = parser.parse_args()
    if args.action == "check":
        print(
            json.dumps(
                {"assets": len(artifacts(args.assets, args.source)), "offline": True}
            )
        )
        return
    if (
        not args.readelf
        or not (
            args.sysroot / "usr/include/aarch64-linux-gnu/bits/libc-header-start.h"
        ).is_file()
    ):
        parser.error("Provide an existing ARM64 glibc sysroot and readelf executable")
    validate_libraries(args.libraries)
    source_record = json.loads((args.source / "abi-source.json").read_text())
    if source_record["frontend_sha256"] != FRONTEND_SHA:
        raise ValueError("Frontend ABI source differs")
    CACHE.mkdir(parents=True, exist_ok=True)
    args.assets.mkdir(parents=True, exist_ok=True)
    obj, output = CACHE / "compression.o", CACHE / NAME
    flags = [
        "--target=aarch64-linux-gnu",
        "--sysroot=" + str(args.sysroot),
        "-isystem",
        str(args.sysroot / "usr/include/aarch64-linux-gnu"),
        "-std=gnu11",
        "-fPIC",
        "-fvisibility=hidden",
        "-fno-stack-protector",
        "-mno-outline-atomics",
        "-fno-optimize-sibling-calls",
        "-O2",
        "-Wall",
        "-Wextra",
        "-Werror",
    ]
    subprocess.run(
        [
            args.compiler,
            *flags,
            "-c",
            str(args.source / "compression.c"),
            "-o",
            str(obj),
        ],
        check=True,
    )
    ld = str(linker("1.92"))
    subprocess.run(
        [
            ld,
            "-flavor",
            "gnu",
            "-m",
            "aarch64linux",
            "-shared",
            "--no-undefined",
            "-z",
            "noexecstack",
            "-o",
            str(output),
            str(obj),
            str(args.libraries / "arm64-libc.so.6"),
        ],
        check=True,
    )
    dynamic = subprocess.check_output(
        [args.readelf, "-d", "--version-info", str(output)], text=True
    )
    needed = re.findall(r"Shared library: \[([^\]]+)\]", dynamic)
    versions = sorted(
        set(re.findall(r"GLIBC_([0-9.]+)", dynamic)),
        key=lambda x: tuple(map(int, x.split("."))),
    )
    if needed != ["libc.so.6"] or versions != ["2.17"]:
        raise ValueError("Unexpected compression helper dependencies")
    data = output.read_bytes()
    report = {
        "schema": 1,
        "sha256": sha(data),
        "bytes": len(data),
        "source_sha256": source_sha256(args.source),
        "frontend_sha256": FRONTEND_SHA,
        "abi": source_record,
        "target": "aarch64-linux-gnu",
        "needed": needed,
        "maximum_glibc": versions[-1],
        "compiler": subprocess.check_output(
            [args.compiler, "--version"], text=True
        ).splitlines()[0],
        "linker": subprocess.check_output(
            [ld, "-flavor", "gnu", "--version"], text=True
        ).strip(),
        "flags": [flag.replace(str(args.sysroot), "<sysroot>") for flag in flags],
    }
    (args.assets / NAME).write_bytes(data)
    (args.assets / REPORT).write_text(json.dumps(report, indent=2) + "\n")
    artifacts(args.assets, args.source)
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
