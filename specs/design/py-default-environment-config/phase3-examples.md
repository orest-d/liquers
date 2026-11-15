# Phase 3: Examples & Use-cases — py-default-environment-config

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Configure after imports | the problem example, working | AC-1, AC-8 |
| 2 | Example | Your own environment, configured late | `env.configure` and its limits | AC-4, AC-7 |
| 3 | Test | `test_CONFIGURE01_configure_after_import_time_registration` | | AC-1 |
| 4 | Test | `test_CONFIGURE02_keywords_apply_to_earlier_registrations` | | AC-2 |
| 5 | Test | `test_CONFIGURE03_registry_file_then_import_metadata` | | AC-3 |
| 6 | Test | `test_CONFIGURE04_configure_after_build_is_refused` | | AC-4 |
| 7 | Test | `test_CONFIGURE05_last_configure_wins` | | AC-5 |
| 8 | Test | `test_CONFIGURE06_invalid_configure_changes_nothing` | | AC-6 |
| 9 | Test | `test_CONFIGURE07_env_configure_is_per_environment` | | AC-7 |
| 10 | Test | `test_CONFIGURE08_serve_and_ui_use_configured_settings` | | AC-8 |
| 11 | Test | `test_CONFIGURE09_configure_after_failed_build` | | AC-9 |
| 12 | Test | `aio::tests::resize_sets_an_idle_pool_and_refuses_a_running_one` | | AC-2, AC-9 |
| 13 | Test | `environment::tests::configure_is_refused_once_built` | | AC-4 |

## Example 1: Configure after imports

A module registers a command when it is imported. The program then configures the default
environment from a file and serves it. This is the user guide's new §8.1 example; it runs under
`tests/test_GUIDE_examples.py`.

```python
import pathlib, tempfile
import liquers as lq

@lq.first_command                       # creates the default environment, as an import would
def hello() -> str:
    return "hello"

folder = pathlib.Path(tempfile.mkdtemp())
(folder / "liquers.yaml").write_text(
    "store:\n  stores:\n    - {type: file, root: " + str(folder / "data") + "}\n"
    "server: {port: 0}\npython: {pickle: true}\n"
)
lq.configure(config=folder / "liquers.yaml")
assert lq.evaluate("hello") == "hello"  # the first build: file store, pickle on
server = lq.serve()                     # port 0 from the file: a free port
assert server.port != 0
server.shutdown()
```

Expected: `hello` is in the configured environment; the server binds the file's port.

## Example 2: Your own environment, configured late

```python
import liquers as lq
import pytest

env = lq.Environment()

@env.first_command
def two() -> int:
    return 2

env.configure(job_capacity=8, conversion="strict")
assert env.evaluate("two") == 2
with pytest.raises(lq.ConfigurationError, match="already built"):
    env.configure(pickle=True)          # after the first build
env.shutdown()
```

## Edge and Error Cases

- **The default is process-wide.** Once it is built, it cannot be configured again. So every test
  of `lq.configure` runs its script in a fresh interpreter (`run_script(tmp_path, source)`, a
  helper in the new test file, as `test_GUIDE_examples.py` does), except AC-7, which uses its own
  environments.
- **A late registration after configure and build** still rebuilds under the configured settings
  (part of AC-4).
- **`store_set` and `render` before any evaluation** now build first. Covered by AC-8's store
  check and the existing `test_TEMPLATE_top_level.py` and `test_STORE_*` suites, which still
  pass unchanged.
- **Concurrency.** A thread calls `configure` while another makes the first evaluation. One of two
  outcomes is allowed: `configure` wins, or it raises "already built". It never half-applies.
  This is not tested separately: both run under one lock, which the Rust test of AC-4 exercises.
- **A failed build that started the loops.** `configure(async_loops=…)` with a different size is
  refused, the same size is accepted (`resize` test).

## Test Plan

- Python: `liquers-py/tests/test_CONFIGURE_environment.py`:
  - `test_CONFIGURE01_configure_after_import_time_registration` (AC-1): a subprocess registers
    with `@lq.first_command` at module level, then calls `lq.configure(config=Path)` with a file
    store. `lq.evaluate` returns the value, and `lq.default_environment().store_set` lands in the
    file store.
  - `test_CONFIGURE02_keywords_apply_to_earlier_registrations` (AC-2): a command registered
    before `configure(conversion="strict")` that returns an opaque object raises
    `ConversionError`. Under the default it does not. An `async def` command registered before
    `configure(async_loops=2)` runs on a pool of 2 (`lq._native.live_loop_count` is not needed:
    the command reports `threading.get_ident()` of two concurrent calls). `job_capacity=8` reaches
    the asset manager's options.
  - `test_CONFIGURE03_registry_file_then_import_metadata` (AC-3): `command_registry:` relabels a
    registered command, and a later `import_metadata` relabels it again. `command_metadata` shows
    the last label. A file describing a command registered after `configure` is accepted.
  - `test_CONFIGURE04_configure_after_build_is_refused` (AC-4): after `evaluate`,
    `configure(...)` raises `ConfigurationError` matching "already built". `evaluate` still works,
    and a command registered after that is evaluated.
  - `test_CONFIGURE05_last_configure_wins` (AC-5): `configure(config=a)` then
    `configure(pickle=True)` drops `a`, and pickle is on.
  - `test_CONFIGURE06_invalid_configure_changes_nothing` (AC-6), parametrized:
    - an unparsable YAML text;
    - an unknown section;
    - `async_loops=0`;
    - a missing `command_registry:` file.

    Each raises `ConfigurationError` or `ParseError`, and the earlier settings stay in force.
  - `test_CONFIGURE07_env_configure_is_per_environment` (AC-7): configuring `env` leaves the
    default and a second environment unchanged. `lq.configure is not None`, and its keywords
    match `Environment.configure`'s (`inspect.signature`).
  - `test_CONFIGURE08_serve_and_ui_use_configured_settings` (AC-8): a subprocess configures
    `server: {port: 0, prefix: /x}`. `lq.serve()` serves under `/x`, and `lq.serve(prefix="/y")`
    overrides it. `lq.ui()` shows a query over the configured file store.
  - `test_CONFIGURE09_configure_after_failed_build` (AC-9): a store type that fails to build.
    `evaluate` raises, then `configure` with a memory store succeeds and `evaluate` works.
- Rust unit tests:
  - `liquers-py/src/aio.rs` `resize_sets_an_idle_pool_and_refuses_a_running_one` (AC-2, AC-9).
  - `liquers-py/src/environment.rs` `configure_is_refused_once_built` (AC-4).
- Run: `bash scripts/test-liquers-py.sh`, or for the loop `cd liquers-py && pytest
  tests/test_CONFIGURE_environment.py` after `maturin develop`.

## Learning Log

- The default environment cannot be reset in-process. Tests of it need a subprocess per scenario.
