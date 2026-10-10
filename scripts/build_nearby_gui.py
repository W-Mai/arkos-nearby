#!/usr/bin/env python3
"""Check, build, or preview the native handheld room interface."""

import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tomllib

import build_paths

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "gui/nearby/Cargo.toml"
TARGET = build_paths.target("nearby-gui")
SOURCES = build_paths.BUILD_ROOT / "nearby-gui/sources"
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


def patched_lock():
    source = MANIFEST.with_name("Cargo.lock").read_text()
    expected = tomllib.loads(source)
    version = locked_dependencies()["mirui"]
    package = next(item for item in expected["package"] if item["name"] == "mirui")
    package.pop("source")
    package.pop("checksum")
    blocks = re.split(r"(?m)(?=^\[\[package\]\]$)", source)
    for index, block in enumerate(blocks):
        packages = tomllib.loads(block).get("package", [])
        if packages and packages[0]["name"] == "mirui":
            if packages[0]["version"] != version:
                raise ValueError(
                    "The staged mirui version differs from its registry lock"
                )
            blocks[index] = re.sub(r"(?m)^(?:source|checksum) = .*\n", "", block)
    result = "".join(blocks)
    if tomllib.loads(result) != expected:
        raise ValueError("The staged lock changed dependencies beyond the mirui source")
    return result.encode()


def patch_record():
    version = locked_dependencies()["mirui"]
    package = next(
        item
        for item in tomllib.loads(MANIFEST.with_name("Cargo.lock").read_text())[
            "package"
        ]
        if item["name"] == "mirui"
    )
    patch = MANIFEST.parent / "patches" / f"mirui-{version}.patch"
    return patch, {
        "mirui_crate_sha256": package["checksum"],
        "mirui_patch_sha256": hashlib.sha256(patch.read_bytes()).hexdigest(),
        "effective_cargo_lock_sha256": hashlib.sha256(patched_lock()).hexdigest(),
    }


def extract_crate(data, destination, version):
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        for member in archive.getmembers():
            path = Path(member.name)
            if (
                path.is_absolute()
                or ".." in path.parts
                or not path.parts
                or path.parts[0] != "mirui-" + version
                or not (member.isfile() or member.isdir())
            ):
                raise ValueError("The cached mirui archive has an unexpected member")
            if member.isdir():
                continue
            if len(path.parts) < 2:
                raise ValueError(
                    "The cached mirui archive has no package-relative path"
                )
            output = destination.joinpath(*path.parts[1:])
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_bytes(archive.extractfile(member).read())


def tree_sha256(directory):
    digest = hashlib.sha256()
    for path in sorted(directory.rglob("*")):
        if path.is_symlink():
            raise ValueError("Staged GUI sources cannot contain symbolic links")
        if path.is_file() and path.name != "stage.json":
            digest.update(path.relative_to(directory).as_posix().encode() + b"\0")
            digest.update(path.read_bytes())
    return digest.hexdigest()


def stage_sources(environment):
    patch, record = patch_record()
    inputs = {
        "schema": 1,
        "source_sha256": source_sha256(),
        "cargo_lock_sha256": hashlib.sha256(
            MANIFEST.with_name("Cargo.lock").read_bytes()
        ).hexdigest(),
        **record,
    }
    key = hashlib.sha256(json.dumps(inputs, sort_keys=True).encode()).hexdigest()
    stage = SOURCES / key
    stamp = stage / "stage.json"
    if stamp.is_file():
        saved = json.loads(stamp.read_text())
        if saved["inputs"] == inputs and saved["tree_sha256"] == tree_sha256(stage):
            return stage / "gui/Cargo.toml", inputs
    if stage.exists():
        shutil.rmtree(stage)
    version = locked_dependencies()["mirui"]
    cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    archive = None
    for path in sorted(cargo_home.glob(f"registry/cache/*/mirui-{version}.crate")):
        data = path.read_bytes()
        if hashlib.sha256(data).hexdigest() == record["mirui_crate_sha256"]:
            archive = data
            break
    if archive is None:
        raise ValueError(
            "The checked mirui crate is missing from the existing Cargo cache"
        )
    crate = stage / "mirui"
    extract_crate(archive, crate, version)
    changes = run(
        ["git", "apply", "--numstat", str(patch)], environment, capture=True
    ).stdout
    paths = [Path(line.split("\t", 2)[2]) for line in changes.splitlines()]
    if not paths or any(
        path.is_absolute()
        or ".." in path.parts
        or path.parts[0] not in {"src", "tests"}
        or path.suffix != ".rs"
        for path in paths
    ):
        raise ValueError("The mirui patch may change only Rust source and test files")
    run(["git", "apply", "--check", str(patch)], environment, cwd=crate)
    run(["git", "apply", str(patch)], environment, cwd=crate)
    package = stage / "gui"
    for path in source_paths():
        output = package / path.relative_to(MANIFEST.parent)
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(path.read_bytes())
    (package / "Cargo.toml").write_text(
        MANIFEST.read_text() + '\n[patch.crates-io]\nmirui = { path = "../mirui" }\n'
    )
    (package / "Cargo.lock").write_bytes(patched_lock())
    if source_sha256() != inputs["source_sha256"]:
        raise ValueError(
            "GUI sources changed while preparing the build; retry the command"
        )
    stamp.write_text(json.dumps({"inputs": inputs, "tree_sha256": tree_sha256(stage)}))
    return package / "Cargo.toml", inputs


def artifacts(assets):
    report = json.loads((assets / "nearby-gui-build.json").read_text())
    data = (assets / "arkos-nearby-gui").read_bytes()
    if (
        report["target"] != TRIPLE
        or any(
            report.get(name) != version
            for name, version in locked_dependencies().items()
        )
        or any(report.get(name) != value for name, value in patch_record()[1].items())
        or report["source_sha256"] != source_sha256()
        or report["cargo_lock_sha256"]
        != hashlib.sha256(MANIFEST.with_name("Cargo.lock").read_bytes()).hexdigest()
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
    paths.extend(sorted((package / "patches").glob("*")))
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
    manifest, patch = stage_sources(environment)
    common = ["--manifest-path", str(manifest), "--locked", "--offline"]
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
        if source_sha256() != patch["source_sha256"]:
            raise ValueError(
                "GUI sources changed during compilation; rebuild before packaging"
            )
        ASSETS.mkdir(parents=True, exist_ok=True)
        destination = ASSETS / "arkos-nearby-gui"
        destination.write_bytes(data)
        destination.chmod(0o755)
        report = {
            "schema": 1,
            "target": TRIPLE,
            "toolchain": args.toolchain,
            **locked_dependencies(),
            **patch,
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
