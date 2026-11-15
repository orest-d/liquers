# Phase 4: Implementation Plan — py-default-environment-config

## Overview

The order is: bind conversion at build time first, a refactor with no visible change, then move
the settings under the lock, then add `configure`. Each step keeps the existing suite green.
`py-command-line-config` builds on Step 4 and lands in the same PR. Nothing has to be fixed
first.

## Progress

- [x] Step 1: `Conversion`, bound when a build registers a command (87988ea)
- [x] Step 2: `Settings` under the declarations lock (87988ea)
- [x] Step 3: `LazyLoopPool::resize` (87988ea)
- [x] Step 4: `EnvCell::configure`, `Environment.configure`, `lq.configure` (87988ea)
- [x] Step 5: Python tests and stubs (87988ea)
- [x] Step 6: documentation (documentation commit of PR #273)

## Implementation Steps

### Step 1: Bind conversion when a build registers a command
- Files / symbols:
  - `liquers-py/src/value.rs`: new `Conversion`;
  - `liquers-py/src/command.rs`: `PyCallable`, `PyCommandSpec::new`, `register_py_command`,
    `call_py_command` and its helpers;
  - `liquers-py/src/context.rs`: `with_conversion`;
  - `liquers-py/src/environment.rs`: `_register_command`, `configured_builder`.
- Change: as in Phase 2 Interfaces. `configured_builder` passes the cell's current conversion.
- Proof: `cargo check -p liquers-py --tests`, then `bash scripts/test-liquers-py.sh` (the Rust
  unit tests of the extension module link only there).
- Rollback: revert the commit.

### Step 2: Settings under the declarations lock
- Files / symbols: `environment.rs`:
  - new `Settings` and `Settings::resolve`, with the body moved from `Environment::new`;
  - `Declarations.settings`, with `EnvCell::config`, `conversion` and `pickle` removed;
  - `EnvCell::new(Settings)`, `envref_and_settings`;
  - `server_config`, `stored_state` and the two `build_request` calls read via
    `envref_and_settings`;
  - `apply_imported` runs over `registry_file`, then `imported`;
  - `read_command_registry` raises `configuration_error` for a missing file.

  Also the test call sites in `server.rs`, `records/resolver.rs` and `environment.rs` tests.
- Depends on: Step 1.
- Proof: `bash scripts/test-liquers-py.sh`: the whole existing suite, Rust unit tests included,
  passes unchanged.
- Rollback: revert the commit.

### Step 3: `LazyLoopPool::resize`
- Files / symbols: `liquers-py/src/aio.rs` `PoolSlot::Idle(NonZeroUsize)`, `get_with`, `resize`.
- Depends on: none; it can run in parallel with Step 2.
- Proof: `aio::tests::resize_sets_an_idle_pool_and_refuses_a_running_one`, run by
  `bash scripts/test-liquers-py.sh`.
- Rollback: revert the commit.

### Step 4: `configure`
- Files / symbols:
  - `environment.rs`: `EnvCell::configure` (the five ordered checks of Phase 2) and
    `Environment::configure` (`#[pymethods]`);
  - `liquers-py/python/liquers/_env.py` `configure`;
  - `__init__.py` `__all__`, and the stubs `__init__.pyi` (`configure`, `Environment.configure`)
    with their docstrings, so `test_STUBS_declarations.py` stays green at this step.
- Depends on: Steps 2 and 3.
- Proof: `environment::tests::configure_is_refused_once_built`, run by
  `bash scripts/test-liquers-py.sh`.
- Rollback: revert the commit.

### Step 5: Python tests and stubs
- Files: `liquers-py/tests/test_CONFIGURE_environment.py` (the nine tests of Phase 3).
- Depends on: Step 4.
- Proof: `pytest tests/test_CONFIGURE_environment.py tests/test_STUBS_declarations.py`, run in
  `liquers-py` after `maturin develop`.
- Rollback: revert the commit.

### Step 6: Documentation
- Files: as listed in Phase 2's Documentation Architecture.
- Depends on: Step 5.
- Proof:
  - `python3 scripts/check_api_docs.py liquers-py/src/environment.rs liquers-py/python/liquers/_env.py`;
  - `pytest tests/test_GUIDE_examples.py`;
  - `cargo run -p liquers-docs -- check`;
  - `python3 scripts/docs_index.py --check`.
- Rollback: revert the commit.

## Testing Plan

Step 1 checks that the crate and its tests compile. Steps 2 and 5 each run the full Python suite once
(`bash scripts/test-liquers-py.sh`). Only `liquers-py` changes here, and no `cfg(feature)` is
touched, so the build matrix is not needed. CI's `liquers-py-tests.yml` runs it on 3.10, 3.14 and
3.14t.

## Rollback Plan

The steps are separate commits in one PR. If only Steps 1 to 3 land, behaviour is unchanged,
since they are refactors. Step 4 adds the public API, and reverting it removes `configure` and
nothing else.

## Documentation Updates

- The user guide (chapters 02, 08, 12), `PYTHON_GUIDE.md` and `PYTHON_INTEGRATION.md`, each with
  a `## History` row and a `reviewed:` bump where the document carries them (§9.2).
- `docs/guides/python/HISTORY.md` gains a row.
- Module docs as in Phase 2, checked with `check_api_docs.py`.
- Regenerate `specs/index.csv`. `specs/command_registry.yaml` is not affected, since no command
  changes.

## Phase 5 Entry Criteria

- [x] Implementation finished and validated
- [x] User and review comments answered
