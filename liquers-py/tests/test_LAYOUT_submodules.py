"""The package layout: public submodules over the private native package `_liquers`.

Design `specs/design/py-package-submodules/` (Phase 3 test plan). `LAYOUT01`-`LAYOUT08` are local to
that design, not conformance IDs of `LANGUAGE-INTEGRATION_GUIDE.md`; each test names the acceptance
scenarios (`AC-n`) of the design's Phase 1 that it proves.
"""

import ast
import importlib.util
import pathlib

import _liquers
import liquers as lq

PACKAGE = pathlib.Path(lq.__file__).parent


def test_LAYOUT06_native_package_is_private():
    """AC-6: the native code is the private package `_liquers`, reached only through `_backend`."""
    assert importlib.util.find_spec("liquers._native") is None
    assert pathlib.Path(_liquers.__file__).parent.name == "_liquers"
    assert _liquers.Key is lq.Key
    importers = []
    for path in sorted(PACKAGE.glob("*.py")):
        tree = ast.parse(path.read_text(encoding="utf-8"))
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                modules = [alias.name for alias in node.names]
            elif isinstance(node, ast.ImportFrom):
                modules = [node.module or ""]
            else:
                continue
            if any(m == "_liquers" or m.startswith("_liquers.") for m in modules):
                importers.append(path.name)
    assert sorted(set(importers)) == ["_backend.py"]
