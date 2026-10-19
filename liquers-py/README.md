# liquers-py

Python bindings for Liquers, built with [PyO3](https://pyo3.rs) and [maturin](https://www.maturin.rs).

How to use it is [`specs/guides/PYTHON_GUIDE.md`](../specs/guides/PYTHON_GUIDE.md): configuring an
environment, registering commands, running queries, values and errors. How it is built on the Rust
crates is [`specs/reference/PYTHON_INTEGRATION.md`](../specs/reference/PYTHON_INTEGRATION.md).

The distribution is named `liquers-py`; the package it installs is imported as `liquers`:

```python
import liquers as lq

@lq.first_command
def hello() -> str:
    return "hello"

@lq.command
def repeat(text: str, count: int = 2) -> str:
    """Repeat the input text."""
    return text * count

assert lq.evaluate("hello/repeat-3") == "hellohellohello"
```

`lq.command` registers a function whose first parameter is the state. The parameter's name chooses
what it receives: `text` the state's text, `state` an `lq.State` (value and metadata), anything else
the value. `lq.first_command` registers a source command, which takes no state. A parameter named
`context` receives an `lq.Context` (`evaluate`, `info`, `warning`, `error`, `is_cancelled`), and
`*args` takes every remaining query parameter. Both decorators accept `name=`, `namespace=`,
`label=`, `doc=`, `filename=`, `volatile=` and `cached=`, plus `env=` to register in an environment
other than the default.

The bare decorators and `lq.evaluate` use one shared `lq.default_environment()`. For isolation
(tests, several configurations), make your own and use its bound decorators:

```python
import polars as pl

env = lq.Environment()                      # or Environment(config=…, conversion="strict", pickle=True)

@env.first_command
def make_frame() -> pl.DataFrame:
    return pl.DataFrame({"a": [1, 2, 3]})

df = env.evaluate("make_frame/ns-pl/head-2")  # a Python command feeding a Rust one
env.shutdown()
```

Every environment has `liquers-lib`'s built-in commands (`to_text`, `commands_doc`, `ns-pl`, images,
records, templates). Values cross structurally: `None`, `bool`, `int` (64-bit), `float`, `str`,
`bytes`, `list` and `dict` arrive as themselves, a `polars.DataFrame` and a PIL image cross as
Liquers values, and anything else is kept as an opaque `py.Object` (refused instead with
`conversion="strict"`). An opaque object is stored only with `pickle=True`, and a pickle-off
environment never unpickles stored bytes. Errors are raised as `lq.LiquersError`, through a
subclass per `ErrorType` (`lq.KeyNotFoundError`, `lq.ExecutionError`, …); a command may raise one
and its type is kept.

**Late registration.** A command registered after the environment has been used takes effect on the
next evaluation: the environment is rebuilt from everything declared on it, and the store is kept.
The asset cache is not, and the replaced environment is never freed
(`ENVIRONMENT-MANAGER-REFERENCE-CYCLE`), so register commands up front where you can.

**Waiting, interrupting and shutting down.** `evaluate` and `AssetRef.get` block with the GIL
released. Ctrl-C raises `KeyboardInterrupt` in a waiting call, and a wait still outstanding when
`shutdown()` is called raises `lq.CancelledError`; leaving a wait either way does not cancel the
asset (use `AssetRef.cancel()`). `shutdown()` is idempotent, and every later use of the
environment, registering commands included, raises `lq.GeneralError`.

## Layout

This is a mixed Rust/Python project. The Rust crate (`src/`) builds the native extension
`liquers._native`; the Python package around it is `python/liquers/`, whose `__init__.py`
re-exports every class and function of the extension by name. A class registered in
`src/lib.rs` is not reachable as `liquers.<Name>` until it is added to `__init__.py` as well.

## Supported Python

**CPython 3.9 to 3.15; PyPy 3.11 or newer.** These are the versions PyO3 0.30 supports, and
`pyproject.toml` declares `requires-python = ">=3.9,<3.16"` to match CPython. That field cannot
state a narrower range per implementation, so PyPy 3.9 and 3.10 are inside it but fail the build.

No environment override is needed for any supported version, including the cloud dev
environment's CPython 3.13.

## Building and testing

```bash
bash scripts/test-liquers-py.sh
```

run from the repository root, does the whole loop: it creates (or reuses) a virtual environment at
`target/liquers-py-venv`, installs `maturin`, `pytest`, `polars` and `pillow` into it, runs
`maturin develop --features debug-handles` to build and install the `liquers` package, runs `pytest liquers-py/tests`, and then runs the crate's Rust
unit tests. Extra arguments are passed to pytest (`bash scripts/test-liquers-py.sh -k parse`).
`cargo clean` removes the environment along with the rest of `target/`; the next run recreates it.

The pieces on their own:

```bash
cargo check -p liquers-py --lib --tests              # compile only
cargo test -p liquers-py --no-default-features --lib # Rust unit tests
```

`--no-default-features` matters for the second: the default `extension-module` feature leaves the
Python symbols for the loading interpreter to supply, so a test binary built with it fails to link
(`Cargo.toml` explains).

Python test modules are named for the feature they test, `test_<FEATURE>_<topic>.py`
(`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §3).

## Conformance

Against the Python baseline profile of `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`; states as
defined in `specs/reference/CONFORMANCE_TERMS.md`. The design is `specs/design/liquers-py/`. Each
test is named for its ID, in `tests/test_<FEATURE>_*.py`.

| Feature | Selected level | Status | Limitations | Test evidence |
|---|---|---|---|---|
| `OBJECT` | Essential | COMPLETE | — | `test_OBJECT01`–`09` |
| `ERROR` | Essential | COMPLETE | A Python exception's class and message ride in `message`, its stack in `traceback` (`LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`) | `test_ERROR01`–`06` |
| `RUNTIME` | Essential | BLOCKED | `RUNTIME05` is blocked for a built environment by `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` (a strict `xfail`): a built environment's callables are never released. `RUNTIME02` is `NA` until a Pyodide target exists | `test_RUNTIME01`, `03`–`06` |
| `VALUE` | Essential | BLOCKED | A pickle-off read of a pickled `py.Object` refuses to unpickle, but reports "No recipe found" instead of naming the type (`DEGRADED-STORED-VALUE-WITHOUT-RECIPE-REPORTS-NO-RECIPE`, a strict `xfail`). `VALUE12` is `NA` until a `Value` wrapper with operators exists | `test_VALUE01`–`13` |
| `ENVIRON` | Essential | PARTIAL | Every `ENVIRON` test passes; held at `PARTIAL` because its hard dependency `RUNTIME` is `BLOCKED`. Only the default recipe provider can be configured | `test_ENVIRON01`–`08` |
| `EVAL` | Essential | PARTIAL | Every `EVAL` test passes; held at `PARTIAL` because its hard dependencies `VALUE` (`BLOCKED`) and `ENVIRON` (`PARTIAL`) are not `COMPLETE`. `EVAL05`'s payload half is `NA`: the environment's payload type is `()`, so only the context half is tested. Evaluation is synchronous; async is `specs/design/liquers-py-async/` | `test_EVAL01`–`06` |
| `COMMAND` | Essential | PARTIAL | Every `COMMAND` test passes; held at `PARTIAL` because its hard dependencies `RUNTIME` and `VALUE` are `BLOCKED`. No `realm=` on the decorators; `async def` commands are the async design's | `test_COMMAND01`–`14` |

A feature is never claimed `COMPLETE` over a hard dependency that is not (guide §1, "Dependencies
constrain these states"), which is why three features whose own tests all pass read `PARTIAL`.
Both blockers are defects in `liquers-core`, filed under `specs/issues/`; fixing either moves the
features that depend on it.
