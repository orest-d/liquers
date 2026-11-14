"""Runs every Python example in ``specs/guides/PYTHON_GUIDE.md`` and in the user guide.

The guide says each example runs as written; this keeps that true. Each block runs in its own
interpreter, as a reader would run it. A block that defines ``test_`` functions runs under pytest.

The chapters of the user guide *Liquers for Python* (``docs/guides/python/``) are covered too. A
chapter may hold a fragment that is not a program (a call to a server that is not running, say);
its complete examples are the blocks that import ``liquers``, and only those run. A block whose
first line is ``# needs a display`` opens a window that waits for its reader, so it is not run.
"""

import pathlib
import re
import subprocess
import sys

import pytest

ROOT = pathlib.Path(__file__).resolve().parents[2]
GUIDE = ROOT / "specs" / "guides" / "PYTHON_GUIDE.md"
BLOCKS = re.findall(r"```python\n(.*?)```", GUIDE.read_text(), re.S)

USER_GUIDE = ROOT / "docs" / "guides" / "python"
USER_BLOCKS = [
    pytest.param(block, id=f"{chapter.stem}-{index}")
    for chapter in sorted(USER_GUIDE.glob("*.md"))
    for index, block in enumerate(re.findall(r"```python\n(.*?)```", chapter.read_text(), re.S))
    if "import liquers" in block
]


def run_block(block, tmp_path, name):
    if block.startswith("# needs a display"):
        pytest.skip("opens a window that waits for its reader")
    for module in ("polars", "pyarrow", "pandas", "PIL"):
        if module in block:
            pytest.importorskip(module)
    script = tmp_path / f"test_example_{name}.py"
    script.write_text(block)
    if "def test_" in block:
        command = [sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider", str(script)]
    else:
        command = [sys.executable, str(script)]
    result = subprocess.run(command, capture_output=True, text=True, cwd=tmp_path, timeout=300)
    assert result.returncode == 0, f"example {name} failed:\n{block}\n{result.stdout}\n{result.stderr[-3000:]}"


def test_the_guide_has_examples():
    assert len(BLOCKS) >= 10


@pytest.mark.parametrize("index", range(len(BLOCKS)))
def test_guide_example_runs(index, tmp_path):
    run_block(BLOCKS[index], tmp_path, index)


def test_the_user_guide_has_examples():
    assert len(USER_BLOCKS) >= 40


@pytest.mark.parametrize("block", USER_BLOCKS)
def test_user_guide_example_runs(block, tmp_path):
    run_block(block, tmp_path, "user")
