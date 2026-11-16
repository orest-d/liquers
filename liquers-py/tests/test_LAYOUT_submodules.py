"""The package layout: public submodules over the private native package `_liquers`.

Design `specs/design/py-package-submodules/` (Phase 3 test plan). `LAYOUT01`-`LAYOUT08` are local to
that design, not conformance IDs of `LANGUAGE-INTEGRATION_GUIDE.md`; each test names the acceptance
scenarios (`AC-n`) of the design's Phase 1 that it proves.
"""

import ast
import importlib
import importlib.util
import inspect
import pathlib
import pickle
import subprocess
import sys
import textwrap

import pytest

import _liquers
import liquers as lq

PACKAGE = pathlib.Path(lq.__file__).parent
REPO = pathlib.Path(__file__).resolve().parents[2]
SUBMODULES = (
    "query", "commands", "metadata", "dependencies", "plan", "recipes", "store", "environment",
    "assets", "error", "data_nodes", "pipeline", "template", "records", "server", "lui",
)

# `liquers.__all__` before the submodules existed (89 names), without the generated error classes.
# A design that adds a public name updates this list: whichever of py-package-submodules, PR #273
# (`configure`) and PR #265 (`import_module`) lands later.
TOP_LEVEL_NAMES = {
    "Position", "ActionParameter", "ResourceName", "ActionRequest", "SegmentHeader",
    "TransformQuerySegment", "Key", "ResourceQuerySegment", "Query", "QuerySource", "parse",
    "parse_key", "EnumArgumentType", "EnumArgument", "ArgumentType", "ArgumentInfo",
    "CommandDefinition", "CommandPreset", "CommandMetadata", "CommandKey",
    "CommandMetadataRegistry", "Metadata", "Version", "DependencyKey", "DependencyRecord",
    "MetadataRecord", "AssetInfo", "Status", "LogEntry", "LogEntryKind", "ErrorType", "Error",
    "Expires", "ExpirationTime", "DependencyRelation", "PlanDependency", "Plan", "Step",
    "ParameterValue", "ResolvedParameterValues", "build_plan", "Recipe", "RecipeList",
    "RecipeProviders", "Environment", "Issue", "Server", "UI", "UIElement", "UIElementInfo",
    "UIStatusEvent", "UISubscription", "Context", "State", "AssetRef", "NodeKind", "DataNode",
    "data_node", "RenderResult", "Diagnostic", "Link", "link", "Pipeline", "PipelineEditor",
    "QueryEditor", "ActionEditor", "StoreArgumentInfo", "Store", "FieldType", "KeyRole",
    "FieldSchema", "RecordSchema", "ChunkOrigin", "RecordView", "RecordColumn", "RecordBuffer",
    "RecordSource", "RecordStream", "RecordChunk", "default_environment", "evaluate",
    "evaluate_async", "serve", "serve_async", "ui", "command", "first_command", "require",
    "LiquersError",
}


def submodule(name):
    return importlib.import_module(f"liquers.{name}")


def run_python(code):
    result = subprocess.run(
        [sys.executable, "-c", textwrap.dedent(code)], capture_output=True, text=True, timeout=120
    )
    assert result.returncode == 0, result.stdout + result.stderr


@pytest.mark.parametrize("name", SUBMODULES)
def test_LAYOUT01_submodule_imports_in_a_fresh_interpreter(name):
    """AC-1: a name imports from its submodule first, and is the top-level object."""
    first = "KeyNotFoundError" if name == "error" else submodule(name).__all__[0]
    run_python(
        f"""
        from liquers.{name} import {first}
        import liquers
        assert liquers.{name}.{first} is liquers.{first} is {first}
        """
    )


def test_LAYOUT02_top_level_names_are_unchanged():
    """AC-2: every name of `liquers.__all__` is still there, and nothing else was added."""
    assert set(lq.__all__) == TOP_LEVEL_NAMES | set(lq.error._ERROR_SUBCLASSES)
    assert isinstance(lq.__version__, str)


def test_LAYOUT03_each_name_has_one_home():
    """AC-2, AC-5: each public name is exported by exactly one submodule, as the same object."""
    homes = {}
    for name in SUBMODULES:
        module = submodule(name)
        for exported in module.__all__:
            assert exported not in homes, f"{exported} is in liquers.{homes.get(exported)} and liquers.{name}"
            homes[exported] = name
            assert getattr(lq, exported) is getattr(module, exported), exported
    assert set(homes) == set(lq.__all__)
    assert not set(SUBMODULES) & set(lq.__all__)


@pytest.mark.parametrize("order", ["submodules first", "package first"])
def test_LAYOUT04_functions_are_not_shadowed(order):
    """AC-3: importing `liquers.lui` and `liquers.data_nodes` keeps `lq.ui` and `lq.data_node`."""
    imports = (
        "import liquers.lui, liquers.data_nodes\nimport liquers as lq"
        if order == "submodules first"
        else "import liquers as lq\nimport liquers.lui, liquers.data_nodes"
    )
    run_python(
        imports
        + """
import inspect
assert callable(lq.ui) and not inspect.ismodule(lq.ui)
assert callable(lq.data_node) and not inspect.ismodule(lq.data_node)
assert liquers.lui.ui is lq.ui and liquers.data_nodes.data_node is lq.data_node
"""
    )


def test_LAYOUT05_classes_report_their_submodule():
    """AC-4: every public class reports the submodule that exports it."""
    for name in SUBMODULES:
        module = submodule(name)
        for exported in module.__all__:
            value = getattr(module, exported)
            if inspect.isclass(value):
                assert value.__module__ == module.__name__, f"{exported} reports {value.__module__}"
    assert type(lq.parse("a/b").segments[0]).__module__ == "liquers.query"
    assert lq.KeyNotFoundError.__module__ == "liquers.error"
    assert pickle.loads(pickle.dumps(lq.KeyNotFoundError)) is lq.KeyNotFoundError


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


def test_LAYOUT07_submodules_have_module_docs():
    """AC-8: each public submodule and the native package have a full module doc."""
    script = REPO / "scripts" / "check_api_docs.py"
    if not script.is_file():
        pytest.skip("needs the repository checkout (scripts/check_api_docs.py)")
    sources = REPO / "liquers-py" / "python"
    files = [sources / "liquers" / f"{name}.py" for name in SUBMODULES]
    files.append(sources / "_liquers" / "__init__.py")
    result = subprocess.run(
        [sys.executable, str(script), "--strict", *map(str, files)],
        capture_output=True, text=True, timeout=120, cwd=REPO,
    )
    assert result.returncode == 0 and result.stdout.strip() == "", result.stdout + result.stderr
    # Every file was checked, not skipped: the summary counts them.
    assert f"check_api_docs: {len(files)} modules," in result.stderr, result.stderr


def test_LAYOUT08_private_helpers_import():
    """AC-9: the private modules the Rust code imports by name are still there."""
    assert callable(importlib.import_module("liquers._ui")._run_as_command)
    assert callable(importlib.import_module("liquers._env")._awaiting)
    for name in ("_commands", "_errors", "_records", "_factories"):
        importlib.import_module(f"liquers.{name}")
