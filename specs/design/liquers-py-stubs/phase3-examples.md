# Phase 3: Examples & Use-cases — liquers-py-stubs

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A typed command in an editor | Decorators keep signatures; wrong calls are errors | AC-5, AC-6 |
| 2 | Example | A drifted declaration fails the loop | stubtest and the name comparison | AC-2, AC-3 |
| 3 | Test | `test_declaration_files_exist` | Files in the install | AC-1 |
| 4 | Test | `test_stubtest_passes` | Runtime matches stubs | AC-2 |
| 5 | Test | `test_native_names_are_declared`, `test_undeclared_names_the_class`, `test_stubtest_reports_drift[missing\|extra]` | Drift is caught | AC-3 |
| 6 | Test | `test_good_samples_pass[mypy\|pyright]` | Representative use | AC-4, AC-6, AC-7, AC-8 |
| 7 | Test | `test_bad_samples_fail_exactly[mypy\|pyright]` | Wrong use, exactly | AC-5, AC-6 |
| 8 | Test | `test_error_classes_are_declared` | Generated errors | AC-8 |

All tests are in `liquers-py/tests/test_STUBS_declarations.py`. Paths are built from
`Path(__file__).parent / "typing"`. Each checker gets its configuration explicitly: mypy
`--config-file`, pyright `-p`.

## Example 1: a typed command in an editor

`good/commands.py` asserts with `assert_type`, which both checkers understand. It does not
compare `reveal_type` text. The forms below were measured clean under mypy 2.4.1 and pyright
1.1.414 in the Phase 3 review, on a mock package with the same stubs.

```python
from typing import Any, assert_type
import liquers as lq

@lq.command
def repeat(text: str, count: int = 2) -> str: return text * count
@lq.first_command
def source(n: int) -> int: return n
env = lq.Environment()
@env.command
def bare(value: float) -> float: return value
@env.command(name="twice")
def twice(value: int) -> int: return value * 2
@env.first_command
async def fetch(url: str) -> bytes: return b""

assert_type(repeat("a", 3), str); assert_type(source(1), int)
assert_type(bare(1.0), float); assert_type(twice(2), int)

async def uses_fetch() -> None:                       # public: no reportUnusedFunction
    assert_type(await fetch("u"), bytes)              # still a coroutine function (AC-6)

q = lq.parse("a/b")
assert_type(q, lq.Query); assert_type(env.evaluate(q), Any)
assert_type(env.evaluate_state("a"), lq.State)
```

`bad/calls.py` starts with the same imports and definitions. After them, each line holds one
intended error and carries both ignores:

```python
lq.parse(42)            # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]
lq.Environment(pickles=True)   # type: ignore[call-arg]  # pyright: ignore[reportCallIssue]
repeat()                # type: ignore[call-arg]  # pyright: ignore[reportCallIssue]
repeat(1)               # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]
source("x")             # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]
bare("x")               # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]
twice("x")              # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]
async def f() -> None:
    await fetch(1)      # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]
    await env.evaluate_state("a")   # type: ignore[misc]  # pyright: ignore[reportGeneralTypeIssues]
```

A wrong argument fails for each of the five forms. `fetch(1)` is awaited inside `f`, so it is not
also an unused coroutine (`unused-coroutine`, `reportUnusedCoroutine`), which the `arg-type`
ignore would not cover. A stub that dropped the parameters (`Callable[...,
_R]`) would therefore leave those ignores unused, and both checkers flag an unused ignore
(`warn_unused_ignores`, `reportUnnecessaryTypeIgnoreComment`).

## Example 2: a drifted declaration fails the loop

`test_stubtest_reports_drift` copies the installed `liquers` tree, every `.py`, `.pyi` and
`py.typed` but not the extension, into a temporary directory and runs
stubtest with `MYPYPATH` pointing at the copy. It has two cases, and each asserts a non-zero exit
whose output names the drift:
- `[missing]` deletes `def evaluate_state`, and expects `liquers.Environment.evaluate_state is
  not present in stub`. The review measured this mechanism.
- `[extra]` adds `def no_such_method(self) -> None: ...` to `Environment`, and expects `is not
  present at runtime`.

`test_native_names_are_declared` reads the stub names with `ast` from `_native.pyi` and compares
them with the public names of `dir(liquers._native)`. It excludes exactly the module function
`live_record_handle_count`, which exists only in builds with `debug-handles`. The stubtest
allowlist covers `liquers.Environment.live_handle_count`, a `debug-handles` staticmethod, and the
module function. `test_undeclared_names_the_class` checks that the helper `undeclared(runtime,
declared)` names a synthetic extra class.

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| A leg other than CPython 3.14 with the GIL | Every STUBS test skips, naming the leg | each test's `skipif` |
| An allowlist entry no longer needed | stubtest exits 1, "unused allowlist entry" | `test_stubtest_passes` (no `--ignore-unused-allowlist`) |
| A checker checks no files (bad path or exclude) | Fails | `test_bad_samples_fail_exactly`; `test_good_samples_pass` asserts the checker's file count (mypy's "N source files", pyright's `filesAnalyzed`) is at least 7 |
| A new `ErrorType` variant in Rust | It is named | `test_error_classes_are_declared` |
| A `debug-handles` name | Excluded or allowlisted, as above | `test_native_names_are_declared`, `test_stubtest_passes` |

`test_bad_samples_fail_exactly` checks a bad sample in two runs. Without the second run, an
exit code of 0 would also hold if the checker had checked nothing:
1. It checks `bad/` as written, and expects exit 0.
2. It checks a copy with the ignore comments stripped, and expects exactly the marked line
   numbers to be reported, no more and no fewer.

## Test Plan

- `test_checkers_are_pinned`: the installed mypy and pyright are the pinned versions, so a
  changed result is never a silent checker upgrade.
- `test_package_sources_pass_mypy` (AC-4): `mypy --strict --config-file <typing>/mypy.ini` over
  the six pure-Python modules reports nothing.
- `test_declaration_files_exist` (AC-1): `importlib.resources.files("liquers")` holds
  `__init__.pyi`, `_native.pyi` and `py.typed`.
- `test_stubtest_passes` (AC-2): `python -m mypy.stubtest liquers --allowlist
  <typing>/stubtest-allowlist.txt` exits 0.
- `test_native_names_are_declared`, `test_undeclared_names_the_class` and
  `test_stubtest_reports_drift[missing|extra]` (AC-3).
- `test_good_samples_pass[mypy]` and `[pyright]` (AC-4, AC-6, AC-7, AC-8) check these samples:
  - `good/queries.py`;
  - `good/environments.py`, which includes `assert_type(env.liquers_version, str)`,
    `assert_type(lq.__version__, str)` and `lq.require("arrow")`;
  - `good/commands.py`;
  - `good/stores.py`, with a Python store and a recipe provider;
  - `good/errors.py`, with `except lq.KeyNotFoundError as e: assert_type(e.key, str | None)`;
  - `good/records.py`, covering constructors with `fields=`, columns, sources, `async for`, and
    `assert_type(view.to_arrow(), Any)`;
  - `good/async_api.py`, with `assert_type` on `await env.evaluate_async(q)`,
    `await asset.get_async()`, `await store.get_async(k)` and
    `await source.materialize_async(10)`.
- `test_bad_samples_fail_exactly[mypy]` and `[pyright]` (AC-5, AC-6): the two runs above.
- `test_error_classes_are_declared` (AC-8): the names of `liquers._ERROR_SUBCLASSES` and of
  each class's `__bases__` equal the stub's `ClassDef` names and bases.
- `liquers-py/tests/package/test_PACKAGE_wheel.py::test_wheel_contents` (AC-1 on the wheel,
  `STUBS07`) gains `py.typed`, `__init__.pyi` and `_native.pyi`; it runs in `liquers-py-package.yml`.
- Command to run: `bash scripts/test-liquers-py.sh -k STUBS`, on CPython 3.14.

## Learning Log

- `assert_type` is the portable assertion. `reveal_type` prints differ between the checkers:
  mypy omits default values.
- An `async def` result asserted directly is `CoroutineType` under pyright and an unused
  coroutine under mypy. Awaiting it inside an `async def` helper is clean under both.
- A bad sample has to be an error under both checkers, or the combined ignore is unused under one
  and fails. The sample lines above were chosen for that.
- stubtest discovers submodules on its own. Naming `liquers._native` as well fails with
  "Duplicate module".
- pyarrow ships no types, so a precise return type would only produce *Unknown* under pyright
  strict. `Any` is the honest declaration.
