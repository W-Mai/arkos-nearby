#!/usr/bin/env python3
"""Build the SDL2/GLES interface against an existing ARM64 GNU sysroot."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil

import build_nearby_gui as gui
import build_paths

ROOT = Path(__file__).resolve().parents[1]
TRIPLE = "aarch64-unknown-linux-gnu"
LICENSES = ROOT / "docs/licenses"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("build", "check"))
    parser.add_argument("--sysroot", type=Path, required=True)
    parser.add_argument("--sdl-library", type=Path, required=True)
    parser.add_argument("--sdl-linkage", choices=("shared", "static"), default="shared")
    parser.add_argument("--linker", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=ROOT / "dist/gpu")
    args = parser.parse_args()
    sysroot = args.sysroot.resolve()
    library = args.sdl_library.resolve()
    data = library.read_bytes()
    if args.sdl_linkage == "static":
        if data[:8] != b"!<arch>\n" or library.name != "libSDL2.a":
            raise ValueError("Static SDL2 must be a libSDL2.a archive")
    elif data[:6] != b"\x7fELF\x02\x01" or int.from_bytes(data[18:20], "little") != 183:
        raise ValueError("SDL2 must be a little-endian ARM64 ELF library")
    if args.sdl_linkage == "shared" and not (library.parent / "libSDL2.so").is_file():
        raise ValueError("The SDL2 library directory needs its libSDL2.so linker name")
    if not (sysroot / "usr/lib/aarch64-linux-gnu/Scrt1.o").is_file():
        raise ValueError("The GNU sysroot needs ARM64 startup objects")
    if not args.linker.is_file():
        raise ValueError("Pass the existing ARM64 GNU C linker")
    inputs = gui.build_inputs()
    cargo, environment = gui.toolchain("1.92")
    target = build_paths.target("gpu-interface")
    environment.update(
        CARGO_TARGET_DIR=str(target),
        CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=str(args.linker.resolve()),
        RUSTFLAGS="-L native=" + str(library.parent),
    )
    common = [
        "--manifest-path",
        str(gui.MANIFEST),
        "--offline",
        "--locked",
        "--target",
        TRIPLE,
        "--features",
        "gpu",
    ]
    if args.action == "check":
        gui.run(
            [str(cargo), "clippy", *common, "--all-targets", "--", "-D", "warnings"],
            environment,
        )
        return
    gui.run([str(cargo), "build", *common, "--release"], environment)
    if inputs != gui.build_inputs():
        raise ValueError("GPU sources changed during compilation")
    binary = target / TRIPLE / "release/arkos-nearby-gui"
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    shutil.copy2(binary, output / "arkos-nearby-gui")
    if args.sdl_linkage == "shared":
        (output / "lib").mkdir(exist_ok=True)
        shutil.copy2(library, output / "lib/libSDL2-2.0.so.0")
    shutil.copytree(LICENSES, output / "licenses", dirs_exist_ok=True)
    font_licenses = output / "licenses/font"
    font_licenses.mkdir(exist_ok=True)
    for name in ("OFL.txt", "font-source.json"):
        shutil.copy2(ROOT / "gui/nearby/assets" / name, font_licenses / name)
    report = {
        "schema": 2,
        "target": TRIPLE,
        "backend": "sdl-gpu",
        "sdl_linkage": args.sdl_linkage,
        **gui.locked_dependencies(),
        **inputs,
        "sha256": sha(binary),
        "bytes": binary.stat().st_size,
        **(
            {"sdl_static_sha256": sha(library)}
            if args.sdl_linkage == "static"
            else {"libraries": {"libSDL2-2.0.so.0": sha(library)}}
        ),
    }
    (output / "nearby-gui-build.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
