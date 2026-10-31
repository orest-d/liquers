#!/usr/bin/env bash
# Build liquers-py into a virtual environment and run its tests: the Python suite under pytest,
# then the crate's Rust unit tests.
#
#   1. Creates (or reuses) a venv at target/liquers-py-venv-<implementation><version><abiflags>
#      (for example `-cpython3.12`, or `-cpython3.14t` for a free-threaded build), from `python3` on
#      PATH, so each interpreter gets its own venv.
#   2. Installs maturin and pytest into it with pip, plus polars and pillow from wheels only: the
#      VALUE suite crosses a DataFrame and an image, and skips those tests (rather than failing) when
#      a package has no wheel for the interpreter (polars on free-threaded 3.14t).
#   3. `maturin develop --features debug-handles` builds the extension and installs the `liquers`
#      package into the venv. `debug-handles` is test-only: it lets RUNTIME05 count the Python
#      callables the registrations hold.
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
python_tag="$("$python" -c 'import sys; print(sys.implementation.name + "%d.%d" % sys.version_info[:2] + getattr(sys, "abiflags", ""))')"
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

echo "==> installing maturin, pytest, polars and pillow"
python3 -m pip install --quiet --disable-pip-version-check maturin pytest
# Wheels only: polars ships abi3 wheels, which a free-threaded interpreter cannot load, and building
# it from its sdist is a long Rust build. Its tests skip without it.
for pkg in polars pillow; do
  python3 -m pip install --quiet --disable-pip-version-check --only-binary :all: "$pkg" \
    || echo "==> no $pkg wheel for this interpreter; its tests will skip"
done

echo "==> maturin develop --features debug-handles"
maturin develop --manifest-path liquers-py/Cargo.toml --features debug-handles

echo "==> pytest liquers-py/tests"
python3 -m pytest liquers-py/tests "$@"

echo "==> cargo test -p liquers-py --no-default-features --lib"
# PyO3 picks the interpreter to link against from PYO3_PYTHON, else `python3` on PATH -- which is
# the venv's here, so both halves of this script use the same interpreter. The test binary links
# libpython dynamically; an interpreter installed outside the system paths (uv, pyenv) keeps it in
# its own LIBDIR, which the loader must be told about.
libdir="$(python3 -c 'import sysconfig; print(sysconfig.get_config_var("LIBDIR") or "")')"
if [ -n "$libdir" ]; then
  export LD_LIBRARY_PATH="$libdir${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
fi
cargo test -p liquers-py --no-default-features --lib

echo
echo "liquers-py: all tests passed."
