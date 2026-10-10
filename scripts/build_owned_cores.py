#!/usr/bin/env python3
"""Validate or explicitly rebuild the pinned Nestopia core from local inputs."""

import argparse
import hashlib
import io
import json
from pathlib import Path
import shutil
import subprocess
import tarfile

import build_core_inspect
import build_paths

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "native/owned_cores/nestopia"
CACHE = ROOT / "build/assets/cores"
BUILD = build_paths.BUILD_ROOT / "owned-cores/nestopia"
FILENAME = "nestopia_nearby_libretro.so"
PIN = "2c276c3338e7df8105213f7c2008a8e2a0cd307a89f40f19fbde05fc2c84f430"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def checked(path, expected):
    data = path.read_bytes()
    if sha(data) != expected:
        raise ValueError("Owned core input checksum differs: " + str(path))
    return data


def source_record():
    record = json.loads((SOURCE / "source.json").read_text())
    if record["artifact_sha256"] != PIN or record["id"] != "nestopia_nearby":
        raise ValueError("Owned core source identity differs")
    checked(SOURCE / "state-footer.patch", record["patch_sha256"])
    checked(SOURCE / "COPYING", record["license_sha256"])
    return record


def artifacts(cache=CACHE):
    record = source_record()
    data = checked(cache / FILENAME, PIN)
    if (
        len(data) != record["artifact_bytes"]
        or data[:6] != b"\x7fELF\x02\x01"
        or int.from_bytes(data[18:20], "little") != 183
    ):
        raise ValueError("Owned core ELF identity differs")
    return {
        "cores/" + FILENAME: data,
        "cores/owned-core-build.json": json.dumps(record, indent=2).encode(),
        "licenses/nestopia-COPYING.txt": (SOURCE / "COPYING").read_bytes(),
    }


def extract_archive(data, destination, selected=None):
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:*") as archive:
        for member in archive.getmembers():
            name = Path(member.name)
            if member.isdir():
                continue
            if selected and (not member.isfile() or not selected(name)):
                continue
            target = destination / name
            if (
                not member.isfile()
                or not target.resolve().is_relative_to(destination.resolve())
                or member.size > 16 * 1024 * 1024
            ):
                raise ValueError("Invalid owned core source archive member")
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(archive.extractfile(member).read())


def wrapper(path, arguments):
    path.write_text(
        "#!/usr/bin/env python3\nimport os, sys\ncommand = "
        + repr(arguments)
        + "\nos.execv(command[0], command + sys.argv[1:])\n"
    )
    path.chmod(0o755)


def build(args):
    for name in (
        "inputs",
        "sysroot",
        "runtime",
        "libc",
        "libm",
        "builtins",
        "strip",
        "clang",
        "output",
    ):
        setattr(args, name, getattr(args, name).resolve())
    if args.linker:
        args.linker = args.linker.resolve()
    record = source_record()
    archive = checked(
        args.inputs / "nestopia-7dfdc25.tar.gz", record["source_archive_sha256"]
    )
    source = BUILD / ("nestopia-" + record["revision"])
    if source.exists():
        raise ValueError("Use an empty owned-core build directory: " + str(BUILD))
    BUILD.mkdir(parents=True, exist_ok=True)
    extract_archive(archive, BUILD)
    glue = source / "libretro/libretro.cpp"
    checked(glue, record["source_file_sha256"])
    subprocess.run(
        [
            "patch",
            "--batch",
            "--fuzz=0",
            "-p1",
            "-i",
            str(SOURCE / "state-footer.patch"),
        ],
        cwd=source,
        check=True,
    )
    checked(glue, record["patched_file_sha256"])
    for name, item in record["build_inputs"].items():
        package = args.inputs / name
        checked(package, item["sha256"])
        data = subprocess.check_output(["ar", "p", str(package), "data.tar.xz"])
        extract_archive(
            data,
            BUILD / "packages",
            lambda path: (
                str(path).startswith("usr/include/")
                or path.name in {"crtbeginS.o", "crtendS.o", "copyright"}
            ),
        )
    libraries = {
        "libstdc++.so.6": args.runtime / "libstdc++.so.6",
        "libgcc_s.so.1": args.runtime / "libgcc_s.so.1",
        "libc.so.6": args.libc,
        "libm.so.6": args.libm,
        "builtins.a": args.builtins,
    }
    for name, path in libraries.items():
        checked(path, record["link_inputs"][name])
    headers = BUILD / "packages/usr/include"
    common = [
        "--target=aarch64-linux-gnu",
        "--sysroot=" + str(args.sysroot),
        "-isystem",
        str(args.sysroot / "usr/include/aarch64-linux-gnu"),
        "-fno-stack-protector",
        "-mno-outline-atomics",
        "-ffile-prefix-map=" + str(source) + "=.",
    ]
    cpp = [
        str(args.clang),
        *common,
        "-nostdinc++",
        "-isystem",
        str(headers / "c++/9"),
        "-isystem",
        str(headers / "aarch64-linux-gnu/c++/9"),
        "-isystem",
        str(headers / "c++/9/backward"),
    ]
    wrapper(BUILD / "cxx.py", cpp)
    wrapper(BUILD / "cc.py", [str(args.clang), *common])
    crt = BUILD / "packages/usr/lib/gcc/aarch64-linux-gnu/9"
    runtime = [
        libraries[n]
        for n in (
            "libstdc++.so.6",
            "libgcc_s.so.1",
            "libm.so.6",
            "libc.so.6",
            "builtins.a",
        )
    ]
    before = [args.sysroot / "usr/lib/aarch64-linux-gnu/crti.o", crt / "crtbeginS.o"]
    after = [crt / "crtendS.o", args.sysroot / "usr/lib/aarch64-linux-gnu/crtn.o"]
    linker = args.linker or build_core_inspect.linker("1.92")
    link = BUILD / "link.py"
    link.write_text(
        "#!/usr/bin/env python3\nimport os, sys\ncommand = "
        + repr(
            [str(linker), "-flavor", "gnu", "-m", "aarch64linux", "-z", "noexecstack"]
            + list(map(str, before))
        )
        + "\nfor arg in sys.argv[1:]:\n"
        + "    command.extend(arg[4:].split(',')) if arg.startswith('-Wl,') else command.append(arg)\n"
        + "command += "
        + repr(list(map(str, runtime + after)))
        + "\nos.execv(command[0], command)\n"
    )
    link.chmod(0o755)
    command = [
        "make",
        "platform=unix",
        "CC=" + str(BUILD / "cc.py"),
        "CXX=" + str(BUILD / "cxx.py"),
        "LD=" + str(link),
        "GIT_VERSION=" + record["build"]["git_version"],
        "-j4",
    ]
    with (BUILD / "build.log").open("wb") as log:
        subprocess.run(
            command,
            cwd=source / "libretro",
            stdout=log,
            stderr=subprocess.STDOUT,
            timeout=240,
            check=True,
        )
    output = BUILD / FILENAME
    shutil.copy2(source / "libretro/nestopia_libretro.so", output)
    subprocess.run([str(args.strip), "--strip-unneeded", str(output)], check=True)
    checked(output, PIN)
    args.output.mkdir(parents=True, exist_ok=True)
    shutil.copy2(output, args.output / FILENAME)
    artifacts(args.output)
    print(
        json.dumps(
            {"path": str(args.output / FILENAME), "sha256": PIN, "reproduced": True}
        )
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="action", required=True)
    commands.add_parser("check")
    compile_parser = commands.add_parser("build")
    for name in ("inputs", "sysroot", "runtime", "libc", "libm", "builtins", "strip"):
        compile_parser.add_argument("--" + name, type=Path, required=True)
    compile_parser.add_argument("--clang", type=Path, default=Path("/usr/bin/clang++"))
    compile_parser.add_argument("--linker", type=Path)
    compile_parser.add_argument("--output", type=Path, default=CACHE)
    args = parser.parse_args()
    if args.action == "check":
        print(json.dumps({"assets": len(artifacts()), "offline": True}))
    else:
        build(args)


if __name__ == "__main__":
    main()
