# liquers-py

Python bindings for Liquers, built with [PyO3](https://pyo3.rs) and [maturin](https://www.maturin.rs).

How to use it is [`specs/guides/PYTHON_GUIDE.md`](https://github.com/orest-d/liquers/blob/main/specs/guides/PYTHON_GUIDE.md):
configuring an environment, registering commands, running queries, values and errors. How it is
built on the Rust crates is
[`specs/reference/PYTHON_INTEGRATION.md`](https://github.com/orest-d/liquers/blob/main/specs/reference/PYTHON_INTEGRATION.md).

Licensed under the GNU Affero General Public License, version 3 or (at your option) any later
version (`AGPL-3.0-or-later`), like every Liquers crate. The wheel also carries
`THIRD-PARTY-NOTICES.html`, the licenses of the Rust crates compiled into it.

## Installing

```bash
pip install liquers-py                 # the package, imported as `liquers`
pip install 'liquers-py[all]'          # with every optional Python package
```

**Wheels** are published for CPython 3.14 and later, on Linux x86_64 and aarch64 (manylinux_2_28),
macOS arm64, and Windows x86_64 and arm64: one `cp314-abi3` wheel for every GIL-enabled CPython
from 3.14, and `cp314t` / `cp315t` wheels for the free-threaded builds. Everything else installs
from the source distribution, which needs a Rust toolchain (1.95 or newer) and a C compiler: CPython
3.10-3.13, PyPy 3.11+, macOS x86_64 and musllinux. `pip install --only-binary :all: liquers-py`
refuses that fallback.

**Extras** install the Python packages an optional feature needs. `lq.require(extra)` imports them
or says what is missing:

| Extra | Installs | Needed for |
|---|---|---|
| `polars` | polars | DataFrames as `polars.DataFrame` |
| `images` | pillow | images as PIL images |
| `arrow` | pyarrow | `RecordView.to_arrow()` / `from_arrow()` |
| `pandas` | pyarrow, pandas | `RecordView.to_pandas()` / `from_pandas()` |
| `all` | all of the above | |

pyarrow has no wheel for Windows arm64, so `arrow` and `pandas` are left out there, and polars has
none for a free-threaded interpreter; `lq.require` reports either as not available on that
platform.

**Quick start.**

<!-- quickstart -->
```python
import liquers as lq

@lq.first_command
def hello() -> str:
    return "world"

@lq.command
def greet(text: str, greeting: str = "Hello") -> str:
    return f"{greeting}, {text}!"

print(lq.evaluate("hello/greet-Hi"))
```
<!-- /quickstart -->

prints

<!-- quickstart-output -->
```text
Hi, world!
```
<!-- /quickstart-output -->

`lq.__version__` is the Liquers version (PEP 440 form), the same as `Environment().liquers_version`.

## Using it

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
The asset cache is not, and the replaced environment is kept until `shutdown()`
(`LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION`), so register commands up front where
you can.

**Waiting, interrupting and shutting down.** `evaluate` and `AssetRef.get` block with the GIL
released. Ctrl-C raises `KeyboardInterrupt` in a waiting call, and a wait still outstanding when
`shutdown()` is called raises `lq.CancelledError`; leaving a wait either way does not cancel the
asset (use `AssetRef.cancel()`). `shutdown()` is idempotent, and every later use of the
environment, registering commands included, raises `lq.GeneralError`.

## Layout

This is a mixed Rust/Python project. The Rust crate (`src/`) builds the native extension
`liquers._native`; the Python package around it is `python/liquers/`, whose `__init__.py`
re-exports every class and function of the extension by name. A class registered in
`src/lib.rs` is not reachable as `liquers.<Name>` until it is added to `__init__.py` as well, and
it must be declared in `__init__.pyi` (and re-exported from `_native.pyi`): the type declarations
are written by hand, and `tests/test_STUBS_declarations.py` fails on a name, method or parameter
that the runtime has and the declarations do not, or the other way round. `py.typed` marks the
package as typed.

## Supported Python

**CPython 3.10 to 3.15; PyPy 3.11 or newer.** PyO3 0.30 also builds for CPython 3.9, but 3.9
reached its end of life in October 2025, so the floor is 3.10 (decided 2026-10-31).
`pyproject.toml` declares `requires-python = ">=3.10,<3.16"` to match CPython. That field cannot
state a narrower range per implementation, so PyPy 3.10 is inside it but fails the build.

No environment override is needed for any supported version, including the cloud dev
environment's CPython 3.13.

**Free-threaded CPython (3.14t)** is supported: the extension declares that it does not need the
GIL, so importing `liquers` leaves it off, and the whole suite passes on 3.14t as well as on 3.13
(`specs/design/liquers-py-async/`). `PYTHON=$(uv python find 3.14t) bash scripts/test-liquers-py.sh`
runs it. CI runs the suite on CPython 3.10, 3.14 and 3.14t (`.github/workflows/liquers-py-tests.yml`);
3.14t runs without polars, which has no free-threaded wheel, so its polars tests skip. PyPy is not
tested in CI.

## Building and testing

```bash
bash scripts/test-liquers-py.sh
```

run from the repository root, does the whole loop: it creates (or reuses) a virtual environment at
`target/liquers-py-venv-<interpreter>` (one per interpreter, so `PYTHON=python3.13 bash
scripts/test-liquers-py.sh` really tests 3.13), installs `maturin`, `pytest`, `polars` and `pillow`
into it, runs `maturin develop --features debug-handles` to build and install the `liquers` package,
runs `pytest liquers-py/tests`, and then runs the crate's Rust
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
(`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §3). The one exception is `test_GUIDE_examples.py`,
which runs every example in `specs/guides/PYTHON_GUIDE.md`.

## Conformance

Against the Python baseline profile of `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`; states as
defined in `specs/reference/CONFORMANCE_TERMS.md`. The design is `specs/design/liquers-py/`. Each
test is named for its ID, in `tests/test_<FEATURE>_*.py`.

| Feature | Selected level | Status | Limitations | Test evidence |
|---|---|---|---|---|
| `OBJECT` | Essential | COMPLETE | — | `test_OBJECT01`–`09` |
| `ERROR` | Essential | COMPLETE | A Python exception's class and message ride in `message`, its stack in `traceback` (`LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`) | `test_ERROR01`–`06`, `test_ERROR_wrapper_deserialization_raises_serialization_error`, `test_ERROR_stored_error_keeps_its_type`, `test_ERROR_wrapper_constructors_raise_typed_errors`, `test_ERROR_typed_wrapper_errors_are_still_exceptions` |
| `RUNTIME` | Essential | COMPLETE | `RUNTIME02` is `NA` until a Pyodide target exists | `test_RUNTIME01`, `03`–`06` |
| `VALUE` | Essential | COMPLETE | `VALUE12` is `NA` until a `Value` wrapper with operators exists | `test_VALUE01`–`13` |
| `ENVIRON` | Essential | COMPLETE | `config["recipes"]` lists the chain's providers, built-in and Python (`RECIPE`) | `test_ENVIRON01`–`08`, `test_ENVIRON_recipes_*` |
| `EVAL` | Essential | COMPLETE | `EVAL05`'s payload half is `NA`: the environment's payload type is `()`, so only the context half is tested | `test_EVAL01`–`06` |
| `COMMAND` | Essential | COMPLETE | No `realm=` on the decorators | `test_COMMAND01`–`14` |
| `ASYNCQ` | Optional | COMPLETE | `ASYNCQ07` is `NA`: Python has an async model. Cancelling an awaitable detaches by policy, so `ASYNCQ04` cancels with `cancel_async()`; `ASYNCQ05` shows detach by completion; `ASYNCQ08` raises on a liquers loop and warns elsewhere. The design is `specs/design/liquers-py-async/` | `test_ASYNCQ01`–`08`, `tests/test_ASYNC_loops.py` |
| `ASYNCCMD` | Optional | COMPLETE | All commands share one job queue (`CORE-ONE-JOB-QUEUE-FOR-ALL-COMMANDS`); `async_loops` sets the number of event loops | `test_ASYNCCMD01`–`06` |
| `STORE` | Optional | COMPLETE | Directions 1-3. `STORE18` is `NA`: Python cannot define `store_config` handlers yet (`LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE`); a Python mount runs the Rust pipeline instead. `STORE12`'s override half is `NA`: a Python type cannot take a built-in name. `STORE06` on the OpenDAL `fs` type is an allowed failure (`STORE-OPENDAL-FS-CONCURRENT-WRITES-TO-ONE-KEY-FAIL`). The design is `specs/design/liquers-py-store/` | `test_STORE01`–`17` (`tests/test_STORE_conformance.py`, `test_STORE_configured.py`), `tests/test_STORE_python_protocol.py`, `tests/test_STORE_registration.py` |
| `RECIPE` | Optional | COMPLETE | A provider is registered as a factory (`register_recipe_provider_factory`) and placed by a `recipes:` entry; one type with different arguments makes several members. `env.recipes.recipe(key)` for a key no member has raises `GeneralError`, not `KeyNotFoundError` (`RECIPE-MISSING-RECIPE-IS-A-GENERAL-ERROR`). Python providers are invisible to `liquers-validate` (`VALIDATE-CANNOT-SEE-NON-STANDARD-RECIPE-PROVIDERS`). The design is `specs/design/liquers-py-recipes/` | `test_RECIPE01`–`07` (`tests/test_RECIPE_python_providers.py`), `tests/test_RECIPE_registration.py` |
| `RECORDS` | Optional | COMPLETE | `RECORDS09` is `NA`: Python has an async model (`async for`). `RECORDS11` is `NA`: Python does not implement the view or source traits (`liquers-py-records` Decision 4). Arrow, pandas and polars exits copy; the column buffers are lent, read-only. A command returning raw CSV bytes cannot be a manifest chunk (`BYTES-VALUE-HAS-NO-NAMED-DATA-FORMAT`). The design is `specs/design/liquers-py-records/` | `test_RECORDS01`–`10` (`tests/test_RECORDS_views.py`, `_columns.py`, `_arrow.py`, `_sources.py`), `tests/test_RECORDS_build.py` |
| `STUBS` | Optional | COMPLETE | Values are `Any` (a query result, `State.value`, a command's inputs), and so are pyarrow, pandas and polars objects, which ship no type information. The checks run on CPython 3.14 with the GIL only, with mypy and pyright pinned. The design is `specs/design/liquers-py-stubs/` | `tests/test_STUBS_declarations.py` (stubtest, name and error-class comparisons, typed samples under `tests/typing/` with mypy `--strict` and pyright `strict`); `test_wheel_contents` |
| `DOCS` | Optional | PARTIAL | `DOCS04`: every public name has a docstring and a signature, but there is no generated API reference yet (`PY-API-REFERENCE-NOT-GENERATED`) | `DOCS03` is `test_GUIDE_examples.py`; the docstring half of `DOCS04` is `test_DOCS04_*` (`tests/test_DOCS_docstrings.py`); `DOCS01`, `02`, `05`–`07` reviewed |

`ASYNCQ`, `ASYNCCMD`, `STORE`, `RECIPE`, `RECORDS`, `STUBS` and `DOCS` are outside the baseline profile. Every Essential feature is `COMPLETE` and every required check passes; each `NA` item carries
its reversing condition. The Python baseline profile is therefore `CONFORMANT`.
