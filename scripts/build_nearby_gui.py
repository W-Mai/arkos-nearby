#!/usr/bin/env python3
"""Check, build, or preview the native handheld room interface."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tomllib

import build_paths

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "gui/nearby/Cargo.toml"
TARGET = build_paths.target("nearby-gui")
ASSETS = ROOT / "build/assets"
TRIPLE = "aarch64-unknown-linux-musl"


def locked_dependencies():
    manifest = tomllib.loads(MANIFEST.read_text())
    packages = tomllib.loads(MANIFEST.with_name("Cargo.lock").read_text())["package"]
    versions = {}
    for name in ("mirui", "mirx"):
        matches = [package for package in packages if package["name"] == name]
        if len(matches) != 1:
            raise ValueError("The GUI needs one locked package for " + name)
        package = matches[0]
        requirement = manifest["dependencies"][name]
        if isinstance(requirement, dict):
            requirement = requirement["version"]
        if (
            requirement != "=" + package["version"]
            or package.get("source")
            != "registry+https://github.com/rust-lang/crates.io-index"
            or not package.get("checksum")
        ):
            raise ValueError("The GUI dependency must match its registry lock: " + name)
        versions[name] = package["version"]
    return versions


def dependency_record():
    locked_dependencies()
    packages = tomllib.loads(MANIFEST.with_name("Cargo.lock").read_text())["package"]
    record = {}
    for name in ("mirui", "mirui-macros", "mirx"):
        matches = [package for package in packages if package["name"] == name]
        if len(matches) != 1:
            raise ValueError("The GUI needs one locked package for " + name)
        package = matches[0]
        if package.get(
            "source"
        ) != "registry+https://github.com/rust-lang/crates.io-index" or not package.get(
            "checksum"
        ):
            raise ValueError("The GUI needs a checked registry package for " + name)
        record[name.replace("-", "_") + "_crate_sha256"] = package["checksum"]
    return record


def build_inputs():
    return {
        "source_sha256": source_sha256(),
        "cargo_lock_sha256": hashlib.sha256(
            MANIFEST.with_name("Cargo.lock").read_bytes()
        ).hexdigest(),
        **dependency_record(),
    }


def artifacts(assets):
    report = json.loads((assets / "nearby-gui-build.json").read_text())
    data = (assets / "arkos-nearby-gui").read_bytes()
    backend = report.get("backend", "framebuffer")
    supported = (report["target"], backend) == (TRIPLE, "framebuffer") or (
        (report["target"], backend) == ("aarch64-unknown-linux-gnu", "sdl-gpu")
        and report.get("sdl_linkage") == "static"
    )
    if (
        not supported
        or any(
            report.get(name) != version
            for name, version in locked_dependencies().items()
        )
        or any(report.get(name) != value for name, value in build_inputs().items())
        or report["sha256"] != hashlib.sha256(data).hexdigest()
        or len(data) < 64
        or data[:6] != b"\x7fELF\x02\x01"
        or int.from_bytes(data[18:20], "little") != 183
    ):
        raise ValueError("The built GUI does not match the locked ARM64 build")
    return {
        "arkos-nearby-gui": data,
        "nearby-gui-build.json": json.dumps(report).encode(),
    }


def source_paths():
    package = MANIFEST.parent
    paths = [MANIFEST, MANIFEST.with_name("Cargo.lock")]
    paths.extend(sorted((package / "src").rglob("*.rs")))
    paths.extend(sorted((package / "assets").iterdir()))
    return paths


def source_sha256():
    digest = hashlib.sha256()
    for path in source_paths():
        digest.update(path.relative_to(MANIFEST.parent).as_posix().encode() + b"\0")
        digest.update(path.read_bytes())
    return digest.hexdigest()


def run(arguments, environment, capture=False, *, cwd=None):
    return subprocess.run(
        arguments,
        env=environment,
        check=True,
        text=True,
        capture_output=capture,
        cwd=cwd,
    )


def toolchain(name):
    result = subprocess.run(
        ["rustup", "which", "--toolchain", name, "cargo"],
        check=True,
        text=True,
        capture_output=True,
    )
    binary = Path(result.stdout.strip())
    environment = dict(os.environ)
    environment.update(
        RUSTC=str(binary.parent / "rustc"),
        RUSTDOC=str(binary.parent / "rustdoc"),
        CARGO_TARGET_DIR=str(TARGET),
        RUSTUP_TOOLCHAIN=name,
        PATH=str(binary.parent) + os.pathsep + environment.get("PATH", ""),
    )
    environment.pop("RUSTFLAGS", None)
    return binary, environment


def linux_environment(cargo, environment):
    metadata = run([environment["RUSTC"], "-vV"], environment, capture=True).stdout
    host = next(
        line.split(": ", 1)[1]
        for line in metadata.splitlines()
        if line.startswith("host:")
    )
    linker = cargo.parent.parent / "lib/rustlib" / host / "bin/rust-lld"
    standard = cargo.parent.parent / "lib/rustlib" / TRIPLE / "lib"
    if not linker.is_file() or not standard.is_dir():
        raise RuntimeError("The existing toolchain needs ARM64 musl std and rust-lld")
    configured = dict(environment)
    configured["RUSTFLAGS"] = (
        "-C linker="
        + str(linker)
        + " -C linker-flavor=ld.lld -C link-self-contained=yes"
    )
    compiler = os.environ.get("CC_aarch64_unknown_linux_musl")
    archiver = os.environ.get("AR_aarch64_unknown_linux_musl")
    if not compiler or not archiver:
        raise RuntimeError(
            "Set CC_aarch64_unknown_linux_musl and "
            "AR_aarch64_unknown_linux_musl to existing clang/llvm-ar"
        )
    configured["CC_aarch64_unknown_linux_musl"] = compiler
    configured["AR_aarch64_unknown_linux_musl"] = archiver
    configured["CFLAGS_aarch64_unknown_linux_musl"] = (
        "-ffreestanding -DRING_CORE_NOSTDLIBINC"
    )
    return configured


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("check", "build", "snapshot"))
    parser.add_argument("--toolchain", default="1.92")
    parser.add_argument("--fetch", action="store_true")
    parser.add_argument("--view", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    cargo, environment = toolchain(args.toolchain)
    if args.fetch:
        run(
            [
                str(cargo),
                "fetch",
                "--manifest-path",
                str(MANIFEST),
                "--locked",
                "--target",
                TRIPLE,
            ],
            environment,
        )
    if args.action == "check":
        run(
            [str(cargo), "fmt", "--manifest-path", str(MANIFEST), "--check"],
            environment,
        )
        formatter = shutil.which("xrune-fmt", path=environment["PATH"])
        if formatter:
            for path in sorted((MANIFEST.parent / "src").rglob("*.rs")):
                if "ui!" in path.read_text():
                    run([formatter, str(path), "--check"], environment)
    inputs = build_inputs()
    common = ["--manifest-path", str(MANIFEST), "--locked", "--offline"]
    if args.action == "check":
        run(
            [str(cargo), "clippy", *common, "--all-targets", "--", "-D", "warnings"],
            environment,
        )
        run([str(cargo), "test", *common], environment)
        run(
            [
                str(cargo),
                "clippy",
                *common,
                "--target",
                TRIPLE,
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
            linux_environment(cargo, environment),
        )
    elif args.action == "build":
        run(
            [str(cargo), "build", *common, "--target", TRIPLE, "--release"],
            linux_environment(cargo, environment),
        )
        binary = TARGET / TRIPLE / "release/arkos-nearby-gui"
        data = binary.read_bytes()
        if source_sha256() != inputs["source_sha256"]:
            raise ValueError(
                "GUI sources changed during compilation; rebuild before packaging"
            )
        ASSETS.mkdir(parents=True, exist_ok=True)
        destination = ASSETS / "arkos-nearby-gui"
        destination.write_bytes(data)
        destination.chmod(0o755)
        report = {
            "schema": 2,
            "target": TRIPLE,
            "backend": "framebuffer",
            "toolchain": args.toolchain,
            **locked_dependencies(),
            **inputs,
            "sha256": hashlib.sha256(data).hexdigest(),
            "bytes": len(data),
        }
        (ASSETS / "nearby-gui-build.json").write_text(
            json.dumps(report, indent=2) + "\n"
        )
        print(json.dumps(report))
    else:
        if args.view is None or args.output is None:
            parser.error("snapshot requires --view and --output")
        run([str(cargo), "build", *common], environment)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        run(
            [
                str(TARGET / "debug/arkos-nearby-gui"),
                "--snapshot",
                str(args.view),
                str(args.output),
            ],
            environment,
        )


if __name__ == "__main__":
    main()
