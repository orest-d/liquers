# Phase 4: Implementation Plan — liquers-py-store

## Overview

`liquers-py` only. The shared call path comes first, because the store adapter and the recipe
provider adapter (`liquers-py-recipes`, implemented after Step 4 here) both use it and the
existing command suite proves the extraction changed nothing. Then the declaration helper, the
adapter (testable from Rust alone), and the factory together with the `lq.Store` handle, which
the Python tests go through. No issue has to be fixed first (Phase 2 preflight). Steps 2 and 3
can run in parallel. The web counterpart of Decision 3 is not a step: it stays with
`WEB-JS-STORE-WITH-ONLY-GET-CANNOT-BE-QUERIED`, for its own PR after this one.

## Progress

- [ ] Step 1: extract the Python call path into `pycall.rs`
- [ ] Step 2: derive factory declarations in `_factories.py`
- [ ] Step 3: the `PyStore` adapter and its method table
- [ ] Step 4: the store factory, registration, the build guard and `lq.Store`
- [ ] Step 5: exports, docstrings and the conformance row

## Implementation Steps

### Step 1: extract the Python call path into `pycall.rs`

- Files / symbols: new `liquers-py/src/pycall.rs` (`PyMethod`, `FromPyReturn`, `call`,
  `LiveHandle`); `src/command.rs` (`PyCallable`, `LIVE_HANDLES`, `live_handle_count`, and the
  `async def` machinery `Completer`, `start_coroutine`, `coroutine_result`); `src/aio.rs`
  (`Pending`: its command `Context` becomes optional); `src/exit.rs` (`py_call`);
  `scripts/test-liquers-py.sh` (the Rust half gains `--features debug-handles`); the
  `debug-handles` comment in `Cargo.toml`.
- Change: the `def` and `async def` paths move behind `pycall::call` (Phase 2 signature: the
  GIL-side `FromPyReturn` converts the result and maps the exception, returning `Error`);
  `PyCallable` keeps its own entry point on the same helpers. `LiveHandle` increments
  `LIVE_HANDLES` on creation and decrements on drop, a zero-sized no-op without
  `debug-handles`; `PyCallable` holds one. `exit::py_call(subject, f)` is `command_call` naming a
  store or provider, with no asset.
- Depends on: none.
- Proof: `bash scripts/test-liquers-py.sh` passes unchanged (`test_RUNTIME_runtime.py` handle
  counts, `test_ASYNCCMD_commands.py`), including the new
  `pycall::tests::live_handle_counts_create_and_drop` under `cargo test -p liquers-py
  --no-default-features --features debug-handles --lib pycall::`.
- Rollback: revert the commit; nothing else uses `pycall` yet.

### Step 2: derive factory declarations in `_factories.py` (parallel with Step 3)

- Files / symbols: new `python/liquers/_factories.py` `_declare(factory, arguments, *, exempt,
  reserved)`; `src/store/mod.rs` `StoreArgumentInfo` pyclass with its `#[new]` (Phase 2).
- Change: the Phase 2 rules; returns `(arguments, takes_kwargs, takes_prefix)` as plain data the
  Rust side turns into `StoreArgumentInfo`s; `arguments=` accepts a list of `lq.StoreArgumentInfo`.
- Depends on: none.
- Proof: `bash scripts/test-liquers-py.sh -k declare_from_signature`
  (`test_STORE_declare_from_signature`, which covers `arguments=` too).
- Rollback: revert; unused until Step 4.

### Step 3: the `PyStore` adapter and its method table (parallel with Step 2)

- Files / symbols: new `src/store/adapter.rs` (`StoreMethods`, `DataAccess`, `PyStore::adapt`,
  `impl AsyncStore for PyStore`); `src/error.rs` `pyerr_to_store_error(py, err, key, store, write)`.
- Change: resolve the protocol once (Phase 2's refusals); every method per the protocol table,
  `Key::as_absolute` checked before any Python call, `default_metadata` and
  `finalize_metadata(update=false)` for metadata `get` omits, derived data-only access only with
  `get` and `listdir` and without `data_access = False`; `is_supported` through
  `exit::attach_tracked`, `false` once exiting.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-py --no-default-features --features debug-handles --lib store::`
  — `method_table_records_presence_and_kind`, `data_access_is_derived_only_with_get_and_listdir`,
  `get_stored_metadata_needs_a_provided_get_metadata`. Their fixtures are Python classes defined
  with `PyModule::from_code`, never `import liquers` (another binary's pyclasses would not extract).
- Rollback: revert; not reachable from Python yet.

### Step 4: the store factory, registration, the build guard and `lq.Store`

- Files / symbols: new `src/store/factory.rs` (`PyStoreType`, `PyStoreFactory`), new
  `src/store/handle.rs` (`Store`, `StoreSource`, `Store.from_object`, `close`);
  `src/environment.rs` (`Declarations.store_types`, `EnvCell::build`, `envref`, `invalidate`,
  `shutdown`, a thread-local `BUILDING` list, `Environment.register_store_factory`,
  `#[getter] store`, `store_types`); `src/exit.rs` (the registry of `Direct` loop pools).
- Change: `build()` passes `ChainedStoreFactory::new()` chaining `default_store_factory()` then
  `PyStoreFactory` to `with_store_config`. Registration refuses a name in
  `default_store_factory().store_types()`, one starting with `opendal_`, or a registered type;
  calls `_declare`; stores the type; marks `Pending`; keeps `declarations.store`. `BUILDING` is
  pushed after the lock is taken and popped after `build()`; `envref`, `invalidate` and
  registrations raise `ConfigurationError` for their own cell before `lock_py_attached`.
  `shutdown()` `mem::take`s `store` and `store_types` and drops them after the guard. The handle
  per Phase 2: an `Environment` source resolves `cell.envref(py)` on the calling thread before an
  `_async` future is spawned; a `Direct` handle owns a lazy loop pool and its own `Shut` (for
  `block_on`), both stopped by `close()`, on drop or at exit.
- Depends on: Steps 1-3.
- Proof: `bash scripts/test-liquers-py.sh -k STORE` — `test_STORE_python_protocol.py`,
  `test_STORE_conformance.py`, `test_STORE_configured.py`, `test_STORE_registration.py`; and on
  3.14t `test_FREETHREAD_concurrency.py` unchanged.
- Rollback: revert Steps 4-5 together; Steps 1-3 stand alone.

### Step 5: exports, docstrings and the conformance row

- Files / symbols: `src/lib.rs`, `python/liquers/__init__.py` (`Store`, `StoreArgumentInfo`),
  `tests/test_DOCS_docstrings.py` coverage, `liquers-py/README.md` `STORE` row (directions 1-3,
  `STORE18` `NA`).
- Change: register and export the classes (Step 4's tests import them from `liquers._native`
  until then); a docstring on every new class and method.
- Depends on: Step 4.
- Proof: `test_DOCS_docstrings.py`; the whole `bash scripts/test-liquers-py.sh`.
- Rollback: with Step 4.

## Testing Plan

Each step runs its own proof. After Step 5: `bash scripts/test-liquers-py.sh` on 3.10, 3.14 and
3.14t (CI). No `cfg(feature)` or optional dependency changes, so `scripts/check-build-matrix.sh`
is not needed; `liquers-core`, `liquers-store` and `liquers-lib` are untouched (their suites run
in CI).

## Rollback Plan

Revert the implementation commits; nothing persistent changes (no file format, no core API).
Steps 1-3 can land alone (an internal refactor and an unused adapter); Steps 4-5 land together.

## Documentation Updates

Phase 5, from Phase 2's documentation architecture: `specs/reference/PYTHON_INTEGRATION.md`
(§Stores, §3, §8), `specs/guides/PYTHON_GUIDE.md` (chapter "Stores"),
`specs/reference/STORE_CONFIG_FSD.md`, `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` (§STORE, §6
Python), `liquers-py/README.md`, `specs/README.md` (the line becomes built), issues
`PY-NO-LANGUAGE-DEFINED-STORE` (closed) and `LANGUAGE-STORE-TYPE-NOT-DEFINABLE` (narrowed to
web). Each changed `reference/` or `guides/` document gets a `## History` row and a `reviewed:`
bump; `python3 scripts/docs_index.py` regenerates `specs/index.csv`. No command changes, so
`specs/command_registry.yaml` is untouched.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
