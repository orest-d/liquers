"""STUBS: the type declarations match the runtime and type representative use (AC-1 to AC-8).

Design: specs/design/liquers-py-stubs/phase3-examples.md. The checks run on the typing leg only,
CPython 3.14 with the GIL, where `scripts/test-liquers-py.sh` installs the pinned checkers.
"""

import ast
import importlib.metadata
import importlib.resources
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

import liquers
import liquers._native

TYPING_LEG = (
    sys.implementation.name == "cpython"
    and sys.version_info[:2] == (3, 14)
    and "t" not in sys.abiflags
)
pytestmark = pytest.mark.skipif(
    not TYPING_LEG,
    reason=f"the typing checks run on CPython 3.14 with the GIL, not {sys.implementation.name} "
    f"{sys.version_info[0]}.{sys.version_info[1]}{sys.abiflags}",
)

TYPING = Path(__file__).parent / "typing"
MYPY_INI = TYPING / "mypy.ini"
PYRIGHT_CONFIG = TYPING / "pyrightconfig.json"
ALLOWLIST = TYPING / "stubtest-allowlist.txt"
PACKAGE = Path(liquers.__file__).parent
PINS = {"mypy": "2.4.1", "pyright": "1.1.414"}
# Names `_native` has only in a `debug-handles` build (the test build).
DEBUG_ONLY = {"live_record_handle_count"}


def run(args, cwd, env=None):
    """Runs a checker outside the source tree, so no repository configuration is picked up."""
    return subprocess.run(args, cwd=cwd, env=env, capture_output=True, text=True, timeout=600)


def stubtest(cwd, env=None):
    return run(
        [sys.executable, "-m", "mypy.stubtest", "liquers", "--allowlist", str(ALLOWLIST),
         "--mypy-config-file", str(MYPY_INI)],
        cwd,
        env,
    )


def remaining(stubtest_output):
    """The class or function each stubtest error names (`liquers.X.y is ...` gives `X`)."""
    names = set()
    for match in re.finditer(r"^error: liquers\.(?:_native\.)?(\w+)", stubtest_output, re.M):
        names.add(match.group(1))
    return names


def mypy(paths, cwd):
    return run([sys.executable, "-m", "mypy", "--config-file", str(MYPY_INI), *paths], cwd)


def pyright(paths, cwd):
    return run(
        [sys.executable, "-m", "pyright", "-p", str(PYRIGHT_CONFIG), "--pythonpath", sys.executable,
         "--outputjson", *paths],
        cwd,
    )


def declared_names(stub):
    """The names a stub file binds at module level: classes, functions, assignments, imports."""
    names = set()
    for node in ast.parse(stub.read_text()).body:
        if isinstance(node, (ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
            names.add(node.name)
        elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
            names.add(node.target.id)
        elif isinstance(node, ast.Assign):
            names.update(t.id for t in node.targets if isinstance(t, ast.Name))
        elif isinstance(node, ast.ImportFrom):
            names.update(a.asname or a.name for a in node.names)
    return names


def undeclared(runtime, declared):
    """Public runtime names with no declaration, sorted, so a failure names each one."""
    return sorted(n for n in runtime if not n.startswith("_") and n not in declared)


def test_checkers_are_pinned():
    for package, version in PINS.items():
        assert importlib.metadata.version(package) == version, (
            f"{package} {importlib.metadata.version(package)} is installed, the pin is {version}; "
            "raise the pin in scripts/test-liquers-py.sh and here together"
        )


def test_declaration_files_exist():
    files = importlib.resources.files("liquers")
    for name in ("__init__.pyi", "_native.pyi", "py.typed"):
        assert files.joinpath(name).is_file(), name


def test_stubtest_passes(tmp_path):
    result = stubtest(tmp_path)
    assert result.returncode == 0, result.stdout + result.stderr
    assert remaining(result.stdout) == set()


def test_package_sources_pass_mypy(tmp_path):
    # `-p liquers` checks the stubs and the six pure-Python modules; `__init__.py` is read
    # through `__init__.pyi`.
    result = mypy(["-p", "liquers"], tmp_path)
    assert result.returncode == 0, result.stdout + result.stderr


def test_native_names_are_declared():
    runtime = set(dir(liquers._native)) - DEBUG_ONLY
    assert undeclared(runtime, declared_names(PACKAGE / "_native.pyi")) == []
    assert undeclared(set(liquers.__all__), declared_names(PACKAGE / "__init__.pyi")) == []


def test_undeclared_names_the_class():
    assert undeclared({"Query", "NewClass", "_private"}, {"Query"}) == ["NewClass"]


@pytest.mark.parametrize("case", ["missing", "extra"])
def test_stubtest_reports_drift(tmp_path, case):
    copy = tmp_path / "stubs" / "liquers"
    copy.mkdir(parents=True)
    for path in PACKAGE.iterdir():
        if path.suffix in (".py", ".pyi") or path.name == "py.typed":
            shutil.copy(path, copy / path.name)
    stub = copy / "__init__.pyi"
    text = stub.read_text()
    if case == "missing":
        changed = re.sub(r"\n    def evaluate_state\(self[^\n]*", "", text, count=1)
        expected = "liquers.Environment.evaluate_state is not present in stub"
    else:
        changed = text.replace(
            "class Environment:\n", "class Environment:\n    def no_such_method(self) -> None: ...\n", 1
        )
        expected = "is not present at runtime"
    assert changed != text
    stub.write_text(changed)
    env = dict(os.environ, MYPYPATH=str(copy.parent))
    result = stubtest(tmp_path, env)
    assert result.returncode != 0
    assert expected in result.stdout, result.stdout


def stub_classes():
    """`{name: [base names]}` of the classes in `__init__.pyi`."""
    classes = {}
    for node in ast.parse((PACKAGE / "__init__.pyi").read_text()).body:
        if isinstance(node, ast.ClassDef):
            classes[node.name] = [b.id for b in node.bases if isinstance(b, ast.Name)]
    return classes


def test_error_classes_are_declared():
    classes = stub_classes()
    assert classes["LiquersError"] == ["Exception"]
    for name, cls in liquers._ERROR_SUBCLASSES.items():
        assert name in classes, f"{name} is not declared in __init__.pyi"
        assert classes[name] == [b.__name__ for b in cls.__bases__], name
    declared_errors = {
        n for n, bases in classes.items()
        if any(b in liquers._ERROR_SUBCLASSES or b == "LiquersError" for b in bases)
    }
    assert declared_errors == set(liquers._ERROR_SUBCLASSES)


@pytest.mark.parametrize("checker", ["mypy", "pyright"])
def test_good_samples_pass(tmp_path, checker):
    good = TYPING / "good"
    if checker == "mypy":
        result = mypy([str(good)], tmp_path)
        assert result.returncode == 0, result.stdout + result.stderr
        checked = int(re.search(r"in (\d+) source files?", result.stdout).group(1))
    else:
        result = pyright([str(good)], tmp_path)
        report = json.loads(result.stdout)
        assert result.returncode == 0, json.dumps(report["generalDiagnostics"], indent=1)
        checked = report["summary"]["filesAnalyzed"]
    assert checked >= 7


IGNORE = re.compile(r"  # type: ignore\[[\w-]+\]  # pyright: ignore\[\w+\]$")


def reported_lines(checker, path, cwd):
    if checker == "mypy":
        result = mypy([str(path)], cwd)
        # mypy prints the path relative to its working directory when it can.
        pattern = rf"^(?:.*/)?{re.escape(path.name)}:(\d+): error"
        lines = {int(m.group(1)) for m in re.finditer(pattern, result.stdout, re.M)}
    else:
        result = pyright([str(path)], cwd)
        report = json.loads(result.stdout)
        lines = {d["range"]["start"]["line"] + 1 for d in report["generalDiagnostics"] if d["severity"] == "error"}
    return result, lines


@pytest.mark.parametrize("checker", ["mypy", "pyright"])
def test_bad_samples_fail_exactly(tmp_path, checker):
    bad = TYPING / "bad" / "calls.py"
    # 1. As written: every intended error is ignored, and every ignore is used.
    result, lines = reported_lines(checker, bad, tmp_path)
    assert result.returncode == 0 and lines == set(), result.stdout
    # 2. Without the ignores: errors on exactly the marked lines.
    source = bad.read_text().splitlines()
    marked = {i + 1 for i, line in enumerate(source) if IGNORE.search(line)}
    assert len(marked) >= 10
    stripped = tmp_path / "calls.py"
    stripped.write_text("\n".join(IGNORE.sub("", line) for line in source) + "\n")
    result, lines = reported_lines(checker, stripped, tmp_path)
    assert lines == marked, result.stdout
