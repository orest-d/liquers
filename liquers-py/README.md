# liquers-py

Python bindings for Liquers, built with [PyO3](https://pyo3.rs) and [maturin](https://www.maturin.rs).

## Building

**Supported Python: CPython 3.8 to 3.12; PyPy 3.8 to 3.10.** The pinned PyO3 (0.21) supports
CPython up to 3.12 and PyPy up to 3.10, and `pyproject.toml` declares
`requires-python = ">=3.8,<3.13"` to match CPython. That field cannot state a narrower range per
implementation, so PyPy 3.11 and newer are inside it but fail the build, and the override below does
not help them: PyPy has no stable ABI, and PyO3 refuses the newer PyPy outright.

On a host whose `python3` is CPython 3.13 or newer, the build fails in PyO3's build script:

```text
error: the configured Python interpreter version (3.13) is newer than PyO3's maximum supported version (3.12)
```

To build anyway, against the stable ABI, set the override PyO3 names in that message:

```bash
PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 cargo check -p liquers-py --lib
```

The override builds, but Python 3.13 is not a supported target until PyO3 is upgraded
(`specs/issues/PY-PYO3-0-21-CANNOT-TARGET-PYTHON-3-13.md`).
