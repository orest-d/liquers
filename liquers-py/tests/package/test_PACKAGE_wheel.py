"""PACKAGE: the installed wheel (liquers-py-package Phase 3).

Run by the release and package workflows' `test` jobs in a fresh venv holding only the wheel
(`test_extras` makes its own venvs). See conftest.py for how they are selected.
"""

from __future__ import annotations

import importlib.metadata
import os
import re
import subprocess
import sys
import time
import venv
from pathlib import Path

import pytest

import liquers as lq
from liquers import _extras

HERE = Path(__file__).resolve().parent
README = HERE.parent.parent / "README.md"
DATA = HERE / "data"

SVG = (
    '<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4">'
    '<rect width="4" height="4" fill="red"/></svg>'
)


def _version(text: str):
    from packaging.version import Version

    return Version(text)


def _ready_type(env: lq.Environment, query: str) -> str:
    """The type identifier of a query's value, read from the asset's metadata, so the value is
    never converted to Python (which would import polars or PIL)."""
    asset = env.get_asset(query)
    deadline = time.monotonic() + 30
    while not asset.status.is_finished():
        if time.monotonic() > deadline:
            raise TimeoutError(f"{query} did not finish: {asset.status}")
        time.sleep(0.05)
    assert asset.status == "Ready", f"{query}: {asset.status} {asset.metadata}"
    return asset.metadata.type_identifier()


def _readme_block(name: str) -> str:
    text = README.read_text(encoding="utf-8")
    match = re.search(rf"<!-- {name} -->\s*```\w*\n(.*?)```\s*<!-- /{name} -->", text, re.S)
    assert match, f"no {name} block in {README}"
    return match.group(1)


def test_import_and_version():
    assert _version(lq.__version__) == _version(os.environ["RELEASE_VERSION"])


def test_quickstart_runs():
    result = subprocess.run(
        [sys.executable, "-c", _readme_block("quickstart")],
        capture_output=True, text=True, check=True, timeout=120,
    )
    assert result.stdout == _readme_block("quickstart-output")


def test_versions_agree():
    environment = lq.Environment()
    try:
        assert (
            _version(importlib.metadata.version("liquers-py"))
            == _version(lq.__version__)
            == _version(environment.liquers_version)
        )
    finally:
        environment.shutdown()


def test_documented_value_types():
    env = lq.Environment()
    try:
        @env.first_command
        def table() -> str:
            return "a,b\n1,2"

        @env.first_command
        def svg() -> str:
            return SVG

        @env.first_command
        def opaque() -> object:
            return object()

        @env.first_command
        def feather() -> bytes:
            return (DATA / "small.feather").read_bytes()

        @env.first_command
        def parquet() -> bytes:
            return (DATA / "small.parquet").read_bytes()

        assert _ready_type(env, "table/ns-pl/from_csv") == "polars.DataFrame"
        assert _ready_type(env, "svg/ns-img/svg_to_image-4-4") == "Image"
        assert _ready_type(env, "table/ns-rec/to_record-csv") == "RecordView"
        assert _ready_type(env, "opaque") == "py.Object"
        assert _ready_type(env, "feather/ns-rec/to_record-feather") == "RecordView"
        assert _ready_type(env, "parquet/ns-rec/to_record-parquet") == "RecordView"
        for name, namespace in [("from_csv", "pl"), ("svg_to_image", "img"),
                                ("to_record", "rec"), ("render", "template")]:
            assert env.command_metadata(name, namespace).name == name
    finally:
        env.shutdown()


def test_wheel_contents():
    files = {str(path).replace("\\", "/") for path in importlib.metadata.files("liquers-py") or []}
    # The notices, and the type declarations (liquers-py-stubs, STUBS07).
    for expected in (
        "liquers/THIRD-PARTY-NOTICES.html",
        "liquers/py.typed",
        "liquers/__init__.pyi",
        "liquers/_native.pyi",
    ):
        assert expected in files, sorted(files)
    assert any(name.endswith("/licenses/LICENSE") or name.endswith("/LICENSE") for name in files)
    metadata = importlib.metadata.metadata("liquers-py")
    assert metadata["Requires-Python"] == ">=3.10, <3.16" or metadata["Requires-Python"] == ">=3.10,<3.16"
    assert metadata["License-Expression"] == "AGPL-3.0-or-later"
    assert set(metadata.get_all("Provides-Extra") or []) == {*_extras.EXTRAS, "all"}


def test_require_names_a_missing_module():
    if importlib.util.find_spec("PIL") is not None:
        pytest.skip("Pillow is installed in this venv")
    with pytest.raises(ModuleNotFoundError) as raised:
        lq.require("images")
    assert str(raised.value) == (
        "No module named 'PIL'; install it with pip install 'liquers-py[images]'"
    )


def test_require_refuses_an_unknown_extra():
    with pytest.raises(ValueError):
        lq.require("nope")


def _wheel_file() -> Path:
    wheel = os.environ.get("LIQUERS_PY_WHEEL")
    if not wheel:
        pytest.skip("LIQUERS_PY_WHEEL (the wheel file) is not set")
    return Path(wheel)


@pytest.mark.parametrize("extra", [*_extras.EXTRAS, "all"])
def test_extras(tmp_path, extra):
    wheel = _wheel_file()
    target = tmp_path / "venv"
    venv.create(target, with_pip=True)
    python = target / ("Scripts/python.exe" if sys.platform == "win32" else "bin/python")
    reason = None if extra == "all" else _extras._unavailable(extra)
    subprocess.run(
        [str(python), "-m", "pip", "install", "--only-binary", ":all:", f"{wheel}[{extra}]"],
        check=True, timeout=900,
    )
    probe = subprocess.run(
        [str(python), "-c", f"import liquers as lq; lq.require({extra!r}); print('ok')"],
        capture_output=True, text=True, timeout=300,
    )
    if reason is None:
        assert probe.returncode == 0, probe.stderr
        assert probe.stdout.strip() == "ok"
    else:
        assert probe.returncode != 0
        assert "is not available on this platform" in probe.stderr
        assert "pip install" not in probe.stderr.splitlines()[-1]
