#!/usr/bin/env python3
"""Build ARM core metadata readers using existing pinned runtime libraries."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "native/core_inspect"
LIBRARIES = ROOT / "build/native-libs"
ASSETS = ROOT / "build/assets"
PROFILES = (
    (32, "armv7-linux-gnueabihf", "armelf_linux_eabi", "/lib/ld-linux-armhf.so.3"),
    (64, "aarch64-linux-gnu", "aarch64linux", "/lib/ld-linux-aarch64.so.1"),
)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_sha256():
    value = hashlib.sha256()
    for path in sorted(SOURCE.iterdir()):
        value.update(path.name.encode() + b"\0")
        value.update(path.read_bytes())
    return value.hexdigest()


def command(arguments, capture=False):
    return subprocess.run(arguments, check=True, capture_output=capture, text=True)


def linker(toolchain):
    result = command(["rustup", "which", "--toolchain", toolchain, "rustc"], True)
    compiler = Path(result.stdout.strip())
    version = command([str(compiler), "-vV"], True).stdout
    host = next(
        line.split(": ", 1)[1]
        for line in version.splitlines()
        if line.startswith("host:")
    )
    binary = compiler.parent.parent / "lib/rustlib" / host / "bin/rust-lld"
    if not binary.is_file():
        raise RuntimeError("The existing Rust toolchain has no rust-lld")
    return binary


def validate_libraries(directory):
    pins = json.loads((SOURCE / "runtime-pins.json").read_text())
    for name, pin in pins.items():
        path = directory / name
        if (
            not path.is_file()
            or path.stat().st_size != pin["bytes"]
            or digest(path) != pin["sha256"]
        ):
            raise ValueError("The inspected runtime library changed: " + name)
    return pins


def compile_flags(compiler, bits, target):
    flags = [
        compiler,
        "--target=" + target,
        "-nostdinc",
        "-ffreestanding",
        "-fno-stack-protector",
        "-O2",
        "-Wall",
        "-Wextra",
        "-Werror",
    ]
    if bits == 32:
        flags.extend(["-mfloat-abi=hard", "-mfpu=vfpv3-d16"])
    return flags


def artifacts(directory):
    report = json.loads((directory / "core-inspect-build.json").read_text())
    if (
        report["source_sha256"] != source_sha256()
        or report["load_mode"] != "RTLD_LAZY|RTLD_LOCAL"
    ):
        raise ValueError("The built core readers do not match the source")
    result = {"core-inspect-build.json": json.dumps(report).encode()}
    for bits, machine in ((32, 40), (64, 183)):
        name = "core-inspect" + str(bits)
        data = (directory / name).read_bytes()
        artifact = report["binaries"][name]
        if (
            artifact["bits"] != bits
            or artifact["sha256"] != hashlib.sha256(data).hexdigest()
            or artifact["bytes"] != len(data)
            or data[:4] != b"\x7fELF"
            or data[4] != (1 if bits == 32 else 2)
            or int.from_bytes(data[18:20], "little") != machine
        ):
            raise ValueError("The native core reader changed: " + name)
        result[name] = data
    return result


def build(compiler, native_linker, libraries, destination, toolchain):
    pins = validate_libraries(libraries)
    destination.mkdir(parents=True, exist_ok=True)
    report = {
        "schema": 1,
        "toolchain": toolchain,
        "source_sha256": source_sha256(),
        "runtime_pins": pins,
        "load_mode": "RTLD_LAZY|RTLD_LOCAL",
        "binaries": {},
    }
    with tempfile.TemporaryDirectory(prefix="arkos-core-build-") as temporary:
        scratch = Path(temporary)
        for bits, target, emulation, loader in PROFILES:
            flags = compile_flags(compiler, bits, target)
            obj = scratch / ("core" + str(bits) + ".o")
            command([*flags, "-c", str(SOURCE / "core_inspect.c"), "-o", str(obj)])
            prefix = "arm" + str(bits) + "-"
            if bits == 32:
                startup = scratch / "start32.o"
                command([*flags, "-c", str(SOURCE / "start32.S"), "-o", str(startup)])
                objects = [startup, obj]
                trailing = []
            else:
                objects = [
                    libraries / (prefix + "crt1.o"),
                    libraries / (prefix + "crti.o"),
                    obj,
                ]
                trailing = [
                    libraries / (prefix + "libc_nonshared.a"),
                    libraries / (prefix + "crtn.o"),
                ]
            output = scratch / ("core-inspect" + str(bits))
            command(
                [
                    str(native_linker),
                    "-flavor",
                    "gnu",
                    "-m",
                    emulation,
                    "-e",
                    "_start",
                    "--dynamic-linker",
                    loader,
                    "--allow-shlib-undefined",
                    "-z",
                    "noexecstack",
                    "-o",
                    str(output),
                    *map(str, objects),
                    str(libraries / (prefix + "libdl.so.2")),
                    str(libraries / (prefix + "libc.so.6")),
                    *map(str, trailing),
                ]
            )
            data = output.read_bytes()
            machine = 40 if bits == 32 else 183
            if (
                data[:4] != b"\x7fELF"
                or data[4] != (1 if bits == 32 else 2)
                or int.from_bytes(data[18:20], "little") != machine
            ):
                raise ValueError("The core reader has an unexpected ELF target")
            path = destination / output.name
            path.write_bytes(data)
            path.chmod(0o755)
            report["binaries"][output.name] = {
                "bits": bits,
                "target": target,
                "loader": loader,
                "bytes": len(data),
                "sha256": hashlib.sha256(data).hexdigest(),
            }
    (destination / "core-inspect-build.json").write_text(
        json.dumps(report, indent=2) + "\n"
    )
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
        with tempfile.TemporaryDirectory(prefix="arkos-core-check-") as temporary:
            for bits, target, _, _ in PROFILES:
                flags = compile_flags(compiler, bits, target)
                command([*flags, "-fsyntax-only", str(SOURCE / "core_inspect.c")])
                if bits == 32:
                    command(
                        [
                            *flags,
                            "-c",
                            str(SOURCE / "start32.S"),
                            "-o",
                            str(Path(temporary) / "start32.o"),
                        ]
                    )
        print("ARM32/ARM64 core reader source checks passed")
    else:
        report = build(
            compiler,
            linker(args.toolchain),
            args.libraries,
            args.output,
            args.toolchain,
        )
        print(
            json.dumps(
                {key: report[key] for key in ("source_sha256", "load_mode", "binaries")}
            )
        )


if __name__ == "__main__":
    main()
