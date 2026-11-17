#!/usr/bin/env python3
"""Render the mirui dual-handheld storyboard and encode WebP or MP4."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / "scripts"))
import build_howff_gui as gui  # noqa: E402


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--frames", type=Path, default=Path("/tmp/arkos-nearby-demo-frames")
    )
    parser.add_argument("--toolchain", default="1.97.0")
    parser.add_argument("--quality", type=int, default=75)
    parser.add_argument("--width", type=int)
    parser.add_argument("--crf", type=int, default=22)
    parser.add_argument("--lossless", action="store_true")
    parser.add_argument("--reuse-frames", action="store_true")
    args = parser.parse_args()
    video = args.output.suffix.lower() == ".mp4"
    if args.output.suffix.lower() not in (".webp", ".mp4"):
        parser.error("--output must use the .webp or .mp4 extension")
    if not 0 <= args.crf <= 51:
        parser.error("--crf must be between 0 and 51")
    args.width = args.width or (1120 if video else 840)
    if args.width <= 0 or (video and args.width % 2):
        parser.error("--width must be positive and even for MP4")
    if not args.reuse_frames:
        cargo, env = gui.toolchain(args.toolchain)
        env["CARGO_INCREMENTAL"] = "0"
        subprocess.run(
            [
                str(cargo),
                "build",
                "--manifest-path",
                str(gui.MANIFEST),
                "--release",
                "--example",
                "dual_demo",
                "--locked",
                "--offline",
            ],
            env=env,
            check=True,
            cwd=ROOT,
        )
        binary = Path(env["CARGO_TARGET_DIR"]) / "release/examples/dual_demo"
        subprocess.run([str(binary), str(args.frames.resolve())], check=True)
    animation = json.loads((args.frames / "animation.json").read_text())
    if video:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        encoded = args.output.with_suffix(".part.mp4").resolve()
        subprocess.run(
            [
                "ffmpeg",
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-framerate",
                str(animation["fps"]),
                "-i",
                str(args.frames / "%04d.png"),
                "-vf",
                f"scale={args.width}:-2:flags=lanczos",
                "-frames:v",
                str(animation["frames"]),
                "-c:v",
                "libx264",
                "-preset",
                "slow",
                "-crf",
                str(args.crf),
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
                "-an",
                str(encoded),
            ],
            check=True,
        )
        encoded.replace(args.output.resolve())
        print(args.output.resolve())
        return
    scaled = args.frames / "scaled"
    scaled.mkdir(exist_ok=True)
    height = round(animation["height"] * args.width / animation["width"])
    subprocess.run(
        [
            "ffmpeg",
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-framerate",
            str(animation["fps"]),
            "-i",
            str(args.frames / "%04d.png"),
            "-vf",
            f"scale={args.width}:{height}:flags=lanczos",
            "-frames:v",
            str(animation["frames"]),
            "-start_number",
            "0",
            str(scaled / "%04d.png"),
        ],
        check=True,
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    encoded = args.output.with_suffix(".part.webp").resolve()
    subprocess.run(
        [
            "img2webp",
            "-min_size",
            "-sharp_yuv",
            "-loop",
            "0",
            "-d",
            str(round(1000 / animation["fps"])),
            "-lossless" if args.lossless else "-lossy",
            "-q",
            str(args.quality),
            "-m",
            "4",
            *(
                str(scaled.resolve() / f"{index:04}.png")
                for index in range(animation["frames"])
            ),
            "-o",
            str(encoded),
        ],
        check=True,
    )
    encoded.replace(args.output.resolve())
    print(args.output.resolve())


if __name__ == "__main__":
    main()
