"""PACKAGE: `lq.require` and the extras table, in the development venv (liquers-py-package).

The bare-venv counterparts, against an installed wheel, are in `tests/package/`.
"""

import sys

import pytest

import liquers as lq
from liquers import _extras


def test_PACKAGE_require_missing_module(monkeypatch):
    monkeypatch.setattr(_extras, "_unavailable", lambda extra: None)
    monkeypatch.setitem(sys.modules, "PIL", None)
    with pytest.raises(ModuleNotFoundError) as raised:
        lq.require("images")
    assert str(raised.value) == (
        "No module named 'PIL'; install it with pip install 'liquers-py[images]'"
    )
    assert raised.value.name == "PIL"


@pytest.mark.parametrize(
    ("extra", "patch", "reason"),
    [
        ("arrow", "windows_arm64", "pyarrow has no wheel for Windows arm64"),
        ("pandas", "windows_arm64", "pyarrow has no wheel for Windows arm64"),
        ("polars", "free_threaded", "polars has no wheel for a free-threaded interpreter"),
    ],
)
def test_PACKAGE_require_not_available_here(monkeypatch, extra, patch, reason):
    monkeypatch.setitem(sys.modules, "polars" if extra == "polars" else "pyarrow", None)
    if patch == "windows_arm64":
        monkeypatch.setattr(_extras.sys, "platform", "win32")
        monkeypatch.setattr(_extras.platform, "machine", lambda: "ARM64")
    else:
        monkeypatch.setattr(
            _extras.sysconfig,
            "get_config_var",
            lambda name: 1 if name == "Py_GIL_DISABLED" else None,
        )
    with pytest.raises(ModuleNotFoundError) as raised:
        lq.require(extra)
    message = str(raised.value)
    assert message == f"liquers-py[{extra}] is not available on this platform: {reason}"
    assert "pip install" not in message


def test_PACKAGE_require_uses_an_importable_module_where_no_wheel_is(monkeypatch):
    # polars built from source on a free-threaded interpreter is used, not refused.
    monkeypatch.setattr(_extras, "_unavailable", lambda extra: "no wheel")
    monkeypatch.setattr(_extras.importlib, "import_module", lambda module: None)
    lq.require("polars")


def test_PACKAGE_require_all_skips_what_is_not_available(monkeypatch):
    imported = []

    def import_module(module):
        imported.append(module)
        if module == "pyarrow":
            raise ModuleNotFoundError(module)

    monkeypatch.setattr(
        _extras, "_unavailable", lambda extra: "x" if extra in ("arrow", "pandas") else None
    )
    monkeypatch.setattr(_extras.importlib, "import_module", import_module)
    lq.require("all")
    assert imported == ["polars", "PIL", "pyarrow", "pyarrow"]


def test_PACKAGE_require_all_reports_a_missing_available_module(monkeypatch):
    def import_module(module):
        if module == "PIL":
            raise ModuleNotFoundError(module)

    # Whatever this interpreter has installed, only PIL is missing.
    monkeypatch.setattr(_extras, "_unavailable", lambda extra: None)
    monkeypatch.setattr(_extras.importlib, "import_module", import_module)
    with pytest.raises(ModuleNotFoundError, match="liquers-py\\[images\\]"):
        lq.require("all")


def test_PACKAGE_require_unknown_extra():
    with pytest.raises(ValueError, match="unknown liquers-py extra 'nope'"):
        lq.require("nope")
