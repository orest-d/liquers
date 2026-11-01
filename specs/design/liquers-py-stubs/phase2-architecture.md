# Phase 2: Solution & Architecture — liquers-py-stubs

## Overview

`liquers/__init__.pyi` is the one place where the package's surface is declared. It holds:
- every native class, under its runtime home `liquers`;
- the module functions;
- the typed decorators;
- the generated error classes.

`liquers/_native.pyi` re-exports those names, so a checker that follows `liquers._native` reads
the same declarations rather than a second copy. The pure-Python modules gain inline
annotations, and `py.typed` marks the package.

A pytest module, `test_STUBS_declarations.py`, runs on the CPython 3.14 leg only. It runs:
- `stubtest liquers liquers._native` with a justified allowlist;
- a name comparison between `dir(liquers._native)` and the declarations;
- mypy `--strict` and pyright `strict` over the `good/` and `bad/` samples.

Rejected:
- **Declaring the classes in `_native.pyi` and re-exporting them from `__init__.pyi`.** Checkers
  would then name every class `liquers._native.X`, while the runtime and the guide say
  `liquers.X` (Phase 1 question 2).
- **Two full copies of the declarations.** They would drift.
- **Checking only with stubtest.** It does not see whether a decorated command keeps its
  signature, which is AC-6.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `PY-METADATA-FIELDS-ARE-METHODS` | draft | P3 | The stubs declare `Metadata`'s fields as methods, as the runtime has them | No | No | Its design updates the stubs (AC-3 enforces it) |
| `PY-QUERY-SEGMENT-CLASS-NOT-REGISTERED` | draft | P3 | `Query.segments` returns a class `_native` does not export | No | No | Declared as a private `_QuerySegment` in the stub, documented, and allowlisted with that reason |
| `PY-ADD-PYTHON-COMMAND-STILL-DECLARES-PYCALL-ALIASES` | draft | P3 | A public method that should not be | No | No | Declared as it is, with a docstring note pointing to the issue |
| `PY-REPRS-SHOW-RUST-DEBUG-FORMAT` | draft | P3 | None: a repr is not typed | No | No | None |
| `PY-PYPY-SUPPORT-UNVERIFIED` | draft | P3 | The checks skip on PyPy | No | No | None |

## Interfaces

**Files** (all new, under `liquers-py/python/liquers/`):
- `__init__.pyi` holds the surface, ordered as `__init__.py`'s `__all__`.
- `_native.pyi` holds `from liquers import X as X` for every name `_native` registers. It also
  declares `live_handle_count` / `live_record_handle_count` in no form at all, because they are
  test-only.
- `py.typed` is empty. maturin copies everything under `python-source`, so no `include` is needed.

The typed forms:

```python
# __init__.pyi (excerpt)
_P = ParamSpec("_P"); _R = TypeVar("_R")
class _Decorator(Protocol):                       # stub-only: the runtime returns a closure
    def __call__(self, fn: Callable[_P, _R], /) -> Callable[_P, _R]: ...

@overload
def command(fn: Callable[_P, _R]) -> Callable[_P, _R]: ...
@overload
def command(fn: None = None, *, name: str | None = None, namespace: str = "", label: str | None = None,
            doc: str | None = None, filename: str | None = None, volatile: bool | None = None,
            cached: bool | None = None, env: Environment | None = None) -> _Decorator: ...
# first_command: the same overloads. Environment.command / first_command: the same without `env`.

class Environment:
    def __init__(self, *, config: dict[str, Any] | str | None = None, conversion: Literal["permissive", "strict"] = "permissive",
                 pickle: bool = False, async_loops: int = 1, job_capacity: int | None = None) -> None: ...
    def evaluate(self, query: Query | str) -> Any: ...
    def evaluate_async(self, query: Query | str) -> Awaitable[Any]: ...
    def evaluate_state(self, query: Query | str) -> State: ...
    @property
    def liquers_version(self) -> str: ...        # liquers-py-package
    # … every #[pymethods] item, with its pyo3 signature's defaults

class RecordView:                                 # liquers-py-records Phase 2, verbatim
    @staticmethod
    def from_records(rows: Iterable[Mapping[str, Any]], *, fields: _Fields = None) -> RecordView: ...
    def to_arrow(self) -> pyarrow.Table: ...      # `import pyarrow` at the top of the stub
class RecordColumn:
    def __buffer__(self, flags: int, /) -> memoryview: ...   # also RecordBuffer (PEP 688)
_Fields: TypeAlias = Mapping[str, FieldType | str | FieldSchema] | RecordSchema | None

class LiquersError(Exception):
    type: ErrorType; message: str; query: str | None; key: str | None; position: Position | None; traceback: str | None
class KeyNotFoundError(NotFoundError, StoreError): ...    # one class per ErrorType, bases as _install builds them
```

- **Rules the stubs follow.**
  - Every value is `Any`: results, `State.value`, and command inputs.
  - Every `_async` method returns `Awaitable[T]`.
  - A `Query | str` parameter accepts both forms.
  - The PyO3 enums are declared as classes with class attributes of their own type, such as
    `ErrorType.KeyNotFound: ErrorType`. They are not `enum.Enum`, which they are not at run time.
- **Third-party types.** `__init__.pyi` imports `pyarrow`, `pandas` and `polars` plainly
  (`import pyarrow`), and annotates `to_arrow() -> pyarrow.Table`, `from_pandas(df:
  pandas.DataFrame)` and so on. Checkers do not report errors inside an installed library's
  stubs. When a package is absent, its names resolve to `Any` for the user, and no error is
  shown, as Phase 1 question 8 requires. The bad samples include one call that is wrong only
  when pyarrow is installed, so the typing leg (which installs it) proves the precise type is
  used.
- **Generated error classes.** These are the classes `_errors._install` creates.
  `test_STUBS_declarations.py` builds the expected list from `lq.ErrorType`'s variants and their
  categories, and compares it with the stub's classes and base classes (AC-8). A variant added in
  Rust therefore fails the loop until it is declared.

**Inline annotations.** `_commands.py`, `_env.py`, `_errors.py`, `_factories.py`, `_records.py`
and `_extras.py` are annotated for the stubtest comparison. Their public entry points come through
`__init__.pyi`, so the annotations are for maintainers, and for stubtest's view of the runtime.

**The checks** (`liquers-py/tests/typing/`):
- `good/*.py` are samples that must pass.
- `bad/*.py` are samples where each intended error line ends in `# type: ignore[<code>]  #
  pyright: ignore[<rule>]`.
- `mypy.ini` sets `strict = True`, `warn_unused_ignores = True` and `python_version = 3.14`.
- `pyrightconfig.json` sets `"typeCheckingMode": "strict"`, `"enableTypeIgnoreComments": false`
  and `"reportUnnecessaryTypeIgnoreComment": "error"`.
- `stubtest-allowlist.txt` holds one line per entry, each preceded by a `#` comment giving the
  reason.

`scripts/test-liquers-py.sh` installs the pinned checkers (`mypy==1.18.*`, `pyright[nodejs]==1.1.4*`,
pinned exactly in the script) on CPython 3.14 only. On every other leg the checks skip with a
reason.

**The allowlist is expected to hold:**
- PyO3's generated `__new__`/`__init__` differences (stubtest sees `__new__` only);
- `__richcmp__`-derived comparison dunders;
- `__hash__ = None` on unhashable classes;
- the `debug-handles` methods;
- `_QuerySegment`.

Implementation Step 2 records the measured list. Any other entry needs a reason that a reviewer
accepts.

## Integration Points

- `liquers-py/python/liquers/`: the two `.pyi` files and `py.typed`, plus annotations in the `_*.py`
  modules.
- `liquers-py/tests/typing/`: the samples and the checker configurations.
  `liquers-py/tests/test_STUBS_declarations.py`.
- `scripts/test-liquers-py.sh`: installs the checkers on the 3.14 leg.
  `.github/workflows/liquers-py-tests.yml`: unchanged (the script decides).
- `liquers-py-package`'s AC-6 checks that the wheel holds the files.

## Error Handling

No runtime errors are added. Every failure is a test failure, and each one names its cause:
stubtest's own report, a missing or extra name, a checker's diagnostic with the sample's file and
line, or an error class whose bases differ.

## Relevant Commands

None.

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `specs/reference/PYTHON_INTEGRATION.md` | reference, py | New §Type declarations: layout, what is `Any` and why, decorator typing, the checks, the allowlist rule, and "a surface change updates the stubs in the same change". §8 drops "no type stubs" |
| `specs/guides/PYTHON_GUIDE.md` | guide, py | New "Type checking" section; §10 drops the limitation |
| `liquers-py/README.md` | readme | A `STUBS` row; §Layout says that adding a class means declaring it |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | guide | §6 Python: what cost time (stubtest and PyO3's `module=`) |
| `specs/README.md` | map | The design's row |

`affects_docs` is unchanged.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-py/python/liquers/*`, `liquers-py/tests/typing/*`, `liquers-py/tests/test_STUBS_declarations.py`, `scripts/test-liquers-py.sh` |
| Crates and workflows affected | liquers-py only; the 3.14 CI leg runs longer (about a minute for the checkers) |
| Existing tests likely to change | None |
| New validation | `test_STUBS_declarations.py` (STUBS01-06, the error classes) |
| Compatibility / data / concurrency / performance / security | Checker upgrades can change results, so the versions are pinned and raised deliberately. `pyright[nodejs]` downloads Node on first use; CI caches it under the venv |
| Recovery | Removing the stubs restores today's untyped state; nothing at run time depends on them |
| Certainty and open questions | Medium-high. Measured at implementation: how stubtest treats the stub-only `_Decorator` protocol, and the size of the allowlist. If stubtest refuses a stub-only private name, the protocol moves to `_commands.py` as a real `typing.Protocol`, which changes nothing for users. No user decision is open |
