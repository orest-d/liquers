# Phase 2: Solution & Architecture — liquers-py-stubs

## Overview

`liquers/__init__.pyi` is the one place where the package's surface is declared. It holds every
native class under its runtime home `liquers`, the module functions, the typed decorators and the
generated error classes. `liquers/_native.pyi` re-exports those names, so a checker that follows
`liquers._native` sees the same declarations. The pure-Python modules carry inline annotations,
and `py.typed` marks the package.

`test_STUBS_declarations.py` runs on the CPython 3.14 (non-free-threaded) leg only. It runs, in
order:
- `stubtest liquers`, which also checks `_native` and the private `_*.py` modules, against a
  justified allowlist;
- a comparison of the names in `dir(liquers._native)` with the declared names;
- a comparison of the error classes;
- mypy `--strict` and pyright `strict` on samples that must pass and samples that must fail.

Rejected:
- Classes declared in `_native.pyi` and re-exported by `__init__`: checkers would print
  `liquers._native.X` where the runtime and the guide say `liquers.X`.
- Two full copies of the declarations: they drift.
- stubtest alone: it cannot see a decorated command's signature (AC-6).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `PY-COMMAND-DECORATOR-CANNOT-SET-REALM`, `PY-COMMAND-DECORATOR-CANNOT-DECLARE-ARGUMENT-METADATA` (design `py-command-decorator-declaration-keywords`, in review) | draft | P3 | It adds `arguments=` and `realm=` to the decorators | No | No | The overloads follow the signature at HEAD when this lands; whichever lands second updates them (AC-3 fails otherwise) |
| `PY-METADATA-FIELDS-ARE-METHODS` | draft | P3 | `Metadata`'s fields are declared as methods, as they are at run time | No | No | Its design updates the stubs |
| `PY-QUERY-SEGMENT-CLASS-NOT-REGISTERED` | draft | P3 | `Query.segments` returns a class that `_native` does not export | No | No | Declared as a stub-only private `_QuerySegment`. stubtest does not report a stub-only private name, so it needs no allowlist entry |
| `PY-ADD-PYTHON-COMMAND-STILL-DECLARES-PYCALL-ALIASES` | draft | P3 | A public method that should not be | No | No | Declared as it is |

## Interfaces

**Files** (new, under `liquers-py/python/liquers/`):
- `__init__.pyi`: the surface, in the order of `__init__.py`'s `__all__`.
- `_native.pyi`: `from liquers import X as X` for every name `_native` registers.
- `py.typed`: empty. maturin ships everything under `python-source`.

**Typed forms** (excerpt):

```python
_P = ParamSpec("_P"); _R = TypeVar("_R")
class _Decorator(Protocol):                       # stub-only; measured: stubtest accepts it
    def __call__(self, fn: Callable[_P, _R], /) -> Callable[_P, _R]: ...
@overload
def command(fn: Callable[_P, _R]) -> Callable[_P, _R]: ...
@overload
def command(fn: None = None, *, name: str | None = None, namespace: str = "", label: str | None = None,
            doc: str | None = None, filename: str | None = None, volatile: bool | None = None,
            cached: bool | None = None, env: Environment | None = None) -> _Decorator: ...
# first_command likewise; Environment.command / first_command the same without `env`
# (measured: accepted against the runtime's (fn=None, **options))
__version__: str
def require(extra: str) -> None: ...              # liquers-py-package
class Environment:
    def __init__(self, *, config: dict[str, Any] | str | None = None,
                 conversion: Literal["permissive", "strict"] = "permissive", pickle: bool = False,
                 async_loops: int = 1, job_capacity: int | None = None) -> None: ...
    def evaluate(self, query: Query | str) -> Any: ...
    def evaluate_async(self, query: Query | str) -> Awaitable[Any]: ...
    @property
    def liquers_version(self) -> str: ...         # liquers-py-package
class RecordColumn:                               # fixed-width columns only lend a buffer
    def __buffer__(self, flags: int, /) -> memoryview: ...
class KeyNotFoundError(StoreError, NotFoundError): ...   # bases in ErrorType::categories order
```

**Rules the stubs follow.**
- Every value is `Any`: results, `State.value`, and the inputs of commands.
- Every awaitable is `Awaitable[T]`: the `_async` methods, `RecordStream.__anext__` and
  `RecordSource.__aiter__` (which returns a `RecordStream`).
- A `Query | str` parameter accepts both forms.
- The PyO3 enums are classes whose members are class attributes of their own type
  (`ErrorType.KeyNotFound: ErrorType`, `KeyRole.Data: KeyRole`).
- Comparison and hash dunders are declared, not allowlisted (`__eq__`, `__lt__`, `__hash__:
  ClassVar[None]`).
- The record classes are declared with the members in `liquers-py-records` Phase 2's table, typed
  here: `RecordView`, `RecordColumn`, `RecordBuffer`, `RecordSource`, `RecordStream`,
  `RecordChunk`, `RecordSchema`, `FieldSchema`, `FieldType`, `KeyRole`, `ChunkOrigin` and
  `State.record_view()`. `fields=` is `Mapping[str, FieldType | str | FieldSchema] | RecordSchema
  | None`.

**Third-party types are `Any`.** pyarrow ships no type information (no `py.typed`, no `.pyi`;
measured on pyarrow 26.0), so `-> pyarrow.Table` would be `Any` under mypy and *Unknown* under
pyright strict, which reports `reportUnknown*` even with pyarrow installed. The stubs therefore
declare the exits (`to_arrow`, `to_pandas`, `to_polars`) and the dataframe parameters of the
constructors as `Any`, with a comment naming the runtime type, and import no third-party
package. This is Phase 1 question 8's "sees `Any`, not an error", with no guard needed (a `.pyi`
is never executed).

**The checks** (`liquers-py/tests/typing/`):
- `good/` holds samples that must pass.
- `bad/` holds samples whose intended errors end in `# type: ignore[<code>]  # pyright:
  ignore[<rule>]`.
- `mypy.ini` sets `strict`, `warn_unused_ignores` and `python_version = 3.14`.
- `pyrightconfig.json` sets `strict`, `"enableTypeIgnoreComments": false` and
  `"reportUnnecessaryTypeIgnoreComment": "error"`.
- `stubtest-allowlist.txt` gives each entry a `#` reason.

The AC-8 test compares `liquers._ERROR_SUBCLASSES`, and each class's `__bases__` by name, with
the classes in `__init__.pyi`, which it reads with `ast`. It therefore checks what `_install`
built, `LookupError` included.

**The typing leg.** It runs where `sys.implementation.name == "cpython"`, the version is 3.14, and
`abiflags` has no `t`. There `scripts/test-liquers-py.sh` installs exact pins of current releases:
`mypy==2.4.1`, `pyright[nodejs]==1.1.414` (Node comes as a wheel, so nothing is downloaded), and
nothing else: the stubs import no third-party package. A comment in the script says the pins are raised deliberately. Each other
leg skips with a reason.

**The allowlist, from measurement.** STUBTEST-RAW-OUTPUT

**Annotations.** `_commands.py`, `_env.py`, `_errors.py` and `_factories.py` are annotated by
this design. `_records.py` and `_extras.py` are annotated by the designs that create them
(records and package), and stubtest checks them all.

**Landing order.** M5 is implemented in one branch: `liquers-py-package`, then
`liquers-py-records`, then this design, last. The stubs are written once every name they declare
exists, so stubtest never sees a declaration ahead of its runtime. Later changes follow Phase 1
question 7's rule: the change that adds a name declares it.

## Integration Points

- `liquers-py/python/liquers/`: two `.pyi` files, `py.typed`, and annotations.
- `liquers-py/tests/typing/`, and `liquers-py/tests/test_STUBS_declarations.py`.
- `scripts/test-liquers-py.sh`: the typing leg's installs.
- `liquers-py-package` AC-6: the wheel holds these files.

## Error Handling

Nothing changes at run time. Every failure is a test failure that names its cause: stubtest's
report, a missing or extra name, a checker diagnostic with file and line, or a differing base
class.

## Relevant Commands

None.

## Documentation Architecture

Each changed reference or guide adds a History row and bumps `reviewed:`. `affects_docs` is
unchanged.

| Path | Change |
|---|---|
| `specs/reference/PYTHON_INTEGRATION.md` | New §Type declarations: layout, what is `Any` and why, the decorator typing, third-party types and pyright strict, the checks and the allowlist rule, and that a surface change updates the stubs in the same change. Open problems: hand-written upkeep, values as `Any`, generated stubs revisited later. §1 Layout gains rows for `__init__.pyi`, `_native.pyi` and `py.typed`. §8 drops only "no type stubs" |
| `specs/guides/PYTHON_GUIDE.md` | New "Type checking" section; §10 drops the limitation |
| `liquers-py/README.md` | A `STUBS` row; §Layout: adding a class means declaring it |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | §6 Python: what cost time (PyO3's `module=`, how stubtest finds submodules) |
| `specs/README.md` | The design's row |

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-py/python/liquers/*`, `liquers-py/tests/typing/*`, `test_STUBS_declarations.py`, `scripts/test-liquers-py.sh` |
| Crates and workflows affected | liquers-py; the 3.14 leg runs about a minute longer |
| Existing tests likely to change | None |
| New validation | `test_STUBS_declarations.py` (STUBS01-06, error classes) |
| Compatibility / data / concurrency / performance / security | A new checker release can change results, so the versions are pinned exactly |
| Recovery | Removing the stubs restores today's untyped state. Nothing at run time depends on them |
| Certainty and open questions | High. Measured by the Phase 2 review: the stub-only protocol, the overloads against `**options`, ParamSpec with `async def`, and the combined ignore comments. No decision needs Orest |
