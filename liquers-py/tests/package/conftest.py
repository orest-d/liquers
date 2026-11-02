"""The installed-wheel tests (liquers-py-package). They run only where the release workflow's
`test` jobs set LIQUERS_PY_WHEEL_TEST=1, against a wheel installed into a fresh venv, never against
`maturin develop`. Run them with `--rootdir` and `--confcutdir` at this directory, so the
development suite's `liquers-py/tests/conftest.py` does not load."""

import os

import pytest


def pytest_collection_modifyitems(config, items):
    if os.environ.get("LIQUERS_PY_WHEEL_TEST") == "1":
        return
    skip = pytest.mark.skip(reason="installed-wheel tests: set LIQUERS_PY_WHEEL_TEST=1")
    for item in items:
        item.add_marker(skip)
