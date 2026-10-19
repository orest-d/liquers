#!/usr/bin/env bash
# Build liquers-py into a virtual environment and run its tests: the Python suite under pytest,
# then the crate's Rust unit tests.
#
#   1. Creates (or reuses) a venv at target/liquers-py-venv-<implementation><version> (for example
#      `-cpython3.12`), from `python3` on PATH, so each interpreter gets its own venv.
#   2. Installs maturin and pytest into it with pip -- nothing else.
#   3. `maturin develop` builds the extension and installs the `liquers` package into the venv.
#   4. `pytest liquers-py/tests`.
#   5. `cargo test -p liquers-py --no-default-features --lib` (links libpython; see Cargo.toml).
#
# The venvs live under target/, so `cargo clean` removes them and the next run recreates them.
# Set PYTHON to build against an interpreter other than `python3`; it gets a venv of its own.
#
# Usage: bash scripts/test-liquers-py.sh [extra pytest arguments]
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python="${PYTHON:-python3}"
# One venv per interpreter: reusing a venv made by another interpreter would silently ignore PYTHON.
python_tag="$("$python" -c 'import sys; print(sys.implementation.name + "%d.%d" % sys.version_info[:2])')"
venv="$repo_root/target/liquers-py-venv-$python_tag"

if [ ! -x "$venv/bin/python" ]; then
  echo "==> creating virtual environment at $venv"
  "$python" -m venv "$venv"
else
  echo "==> reusing virtual environment at $venv"
fi

# Activating sets VIRTUAL_ENV, which is how `maturin develop` finds the environment to install into.
# shellcheck disable=SC1091
source "$venv/bin/activate"

echo "==> installing maturin and pytest"
python3 -m pip install --quiet --disable-pip-version-check maturin pytest

echo "==> maturin develop"
maturin develop --manifest-path liquers-py/Cargo.toml

echo "==> pytest liquers-py/tests"
python3 -m pytest liquers-py/tests "$@"

echo "==> cargo test -p liquers-py --no-default-features --lib"
# PyO3 picks the interpreter to link against from PYO3_PYTHON, else `python3` on PATH -- which is
# the venv's here, so both halves of this script use the same interpreter.
cargo test -p liquers-py --no-default-features --lib

echo
echo "liquers-py: all tests passed."
