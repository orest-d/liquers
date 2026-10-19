# liquers-py

Python bindings for Liquers, built with [PyO3](https://pyo3.rs) and [maturin](https://www.maturin.rs).

The distribution is named `liquers-py`; the package it installs is imported as `liquers`:

```python
import liquers

q = liquers.parse("a/b/-/x")
```

## Layout

This is a mixed Rust/Python project. The Rust crate (`src/`) builds the native extension
`liquers._native`; the Python package around it is `python/liquers/`, whose `__init__.py`
re-exports every class and function of the extension by name. A class registered in
`src/lib.rs` is not reachable as `liquers.<Name>` until it is added to `__init__.py` as well.

## Supported Python

**CPython 3.9 to 3.15; PyPy 3.11 or newer.** These are the versions PyO3 0.30 supports, and
`pyproject.toml` declares `requires-python = ">=3.9,<3.16"` to match CPython. That field cannot
state a narrower range per implementation, so PyPy 3.9 and 3.10 are inside it but fail the build.

No environment override is needed for any supported version, including the cloud dev
environment's CPython 3.13.

## Building and testing

```bash
bash scripts/test-liquers-py.sh
```

run from the repository root, does the whole loop: it creates (or reuses) a virtual environment at
`target/liquers-py-venv-<interpreter>` (one per interpreter, so `PYTHON=python3.13 bash
scripts/test-liquers-py.sh` really tests 3.13), installs `maturin` and `pytest` into it, runs `maturin develop` to build
and install the `liquers` package, runs `pytest liquers-py/tests`, and then runs the crate's Rust
unit tests. Extra arguments are passed to pytest (`bash scripts/test-liquers-py.sh -k parse`).
`cargo clean` removes the environment along with the rest of `target/`; the next run recreates it.

The pieces on their own:

```bash
cargo check -p liquers-py --lib --tests              # compile only
cargo test -p liquers-py --no-default-features --lib # Rust unit tests
```

`--no-default-features` matters for the second: the default `extension-module` feature leaves the
Python symbols for the loading interpreter to supply, so a test binary built with it fails to link
(`Cargo.toml` explains).

Python test modules are named for the feature they test, `test_<FEATURE>_<topic>.py`
(`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §3).
