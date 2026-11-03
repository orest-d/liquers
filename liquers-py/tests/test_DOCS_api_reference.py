"""The generated API reference (LANGUAGE-INTEGRATION_GUIDE.md Appendix A §DOCS, DOCS04; design
py-api-reference).

``scripts/build-liquers-py-docs.sh`` renders the reference with pdoc; these tests check that pdoc
lists every public name, and that the script fails when the module cannot be documented.
"""

import os
import pathlib
import subprocess
import sys

import pytest

import liquers as lq

REPO = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts" / "build-liquers-py-docs.sh"


def test_DOCS04_api_reference_lists_every_public_name(tmp_path):
    pytest.importorskip("pdoc")
    subprocess.run(
        [sys.executable, "-m", "pdoc", "liquers", "-o", str(tmp_path), "--docformat", "markdown"],
        check=True,
    )
    page = (tmp_path / "liquers.html").read_text(encoding="utf-8")
    missing = [name for name in lq.__all__ if f'id="{name}"' not in page]
    assert missing == []


def test_DOCS04_docs_script_fails_on_generator_error(tmp_path):
    """A module pdoc cannot import fails the script (AC-5)."""
    pytest.importorskip("pdoc")
    env = dict(os.environ)
    # With VIRTUAL_ENV set the script uses this interpreter's environment instead of building one.
    env["VIRTUAL_ENV"] = sys.prefix
    env["PATH"] = os.path.dirname(sys.executable) + os.pathsep + env.get("PATH", "")
    result = subprocess.run(
        ["bash", str(SCRIPT), "--out", str(tmp_path / "out"), "liquers_missing"],
        env=env,
        capture_output=True,
        text=True,
    )
    assert result.returncode != 0
