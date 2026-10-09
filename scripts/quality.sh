#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
"${RUFF:-ruff}" check scripts tests
"${RUFF:-ruff}" format --check scripts tests
python3 -W error -m compileall -q scripts tests
python3 -m unittest discover -s tests -v
python3 scripts/build.py check
python3 scripts/build_core_inspect.py check
git diff --check
