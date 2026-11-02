"""Runs every Python example in ``specs/guides/PYTHON_GUIDE.md``.

The guide says each example runs as written; this keeps that true. Each block runs in its own
interpreter, as a reader would run it. A block that defines ``test_`` functions runs under pytest.
"""

import pathlib
import re
import subprocess
import sys

import pytest

GUIDE = pathlib.Path(__file__).resolve().parents[2] / "specs" / "guides" / "PYTHON_GUIDE.md"
BLOCKS = re.findall(r"```python\n(.*?)```", GUIDE.read_text(), re.S)


def test_the_guide_has_examples():
    assert len(BLOCKS) >= 10


@pytest.mark.parametrize("index", range(len(BLOCKS)))
def test_guide_example_runs(index, tmp_path):
    block = BLOCKS[index]
    for module in ("polars", "pyarrow"):
        if module in block:
            pytest.importorskip(module)
    script = tmp_path / f"test_example_{index}.py"
    script.write_text(block)
    if "def test_" in block:
        command = [sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider", str(script)]
    else:
        command = [sys.executable, str(script)]
    result = subprocess.run(command, capture_output=True, text=True, cwd=tmp_path, timeout=300)
    assert result.returncode == 0, f"example {index} failed:\n{block}\n{result.stdout}\n{result.stderr[-3000:]}"
