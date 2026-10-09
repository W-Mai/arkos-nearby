"""Keep compiler caches outside the public source checkout."""

from pathlib import Path

BUILD_ROOT = Path("/tmp/arkos-rust-build/public")


def target(name):
    return BUILD_ROOT / name / "target"
