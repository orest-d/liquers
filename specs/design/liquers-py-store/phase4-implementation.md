# Phase 4: Implementation Plan — liquers-py-store

## Overview

`liquers-py` only. The shared call path comes first, because the store adapter and the recipe
provider adapter (`liquers-py-recipes`, implemented after Step 4 here) both use it and the
existing command suite proves the extraction changed nothing. Then the declaration helper, the
adapter (testable from Rust alone), the factory and its wiring into `EnvCell`, and last the
`lq.Store` handle that most Python tests go through. No issue has to be fixed first (Phase 2
preflight). Steps 2 and 3 can run in parallel.

## Progress

- [ ] Step 1: extract the Python call path into `pycall.rs`
- [ ] Step 2: derive factory declarations in `_factories.py`
- [ ] Step 3: the `PyStore` adapter and its method table
- [ ] Step 4: the store factory, registration and the build guard
- [ ] Step 5: the `lq.Store` handle, `env.store` and `store_types()`
- [ ] Step 6: exports, docstrings and the conformance row
- [ ] Step 7: follow-up PR for the web store with only `get`

## Implementation Steps

### Step 1: extract the Python call path into `pycall.rs`

- Files / symbols: new `liquers-py/src/pycall.rs` (`PyMethod`, `call`, `FromPyReturn`,
  `LiveHandle`); `src/command.rs` (`PyCallable`, `LIVE_HANDLES`, `live_handle_count`);
  `src/exit.rs` (`py_call`); `Cargo.toml` `debug-handles` comment.
- Change: the `def`-on-`spawn_blocking` and `async def`-on-the-loop-pool paths of
  `command.rs` move behind `pycall::call` (signature in Phase 2); `PyCallable` keeps its own
  entry point on the same helpers. `LiveHandle` is a guard incrementing `LIVE_HANDLES` on
  creation and decrementing on drop; `PyCallable` holds one. `exit::py_call(subject, f)` is
  `command_call` generalized to name a store or provider.
- Depends on: none.
- Proof: `bash scripts/test-liquers-py.sh` passes unchanged (`test_RUNTIME_runtime.py` handle
  counts, `test_ASYNCCMD_commands.py`), and a new `pycall::tests::live_handle_counts_create_and_drop`.
- Rollback: revert the commit; nothing else uses `pycall` yet.

### Step 2: derive factory declarations in `_factories.py` (parallel with Step 3)

- Files / symbols: new `python/liquers/_factories.py` `_declare(factory, arguments, *, exempt,
  reserved)`; `src/store/mod.rs` `StoreArgumentInfo` pyclass wrapping
  `liquers_core::store_factory::StoreArgumentInfo`.
- Change: the Phase 2 rules; returns `(arguments, takes_kwargs, takes_prefix)` as plain data the
  Rust side turns into `StoreArgumentInfo`s.
- Depends on: none.
- Proof: `test_STORE_declare_from_signature`.
- Rollback: revert; unused until Step 4.

### Step 3: the `PyStore` adapter and its method table (parallel with Step 2)

- Files / symbols: new `src/store/adapter.rs` (`StoreMethods`, `DataAccess`, `PyStore::adapt`,
  `impl AsyncStore for PyStore`); `src/error.rs` `pyerr_to_store_error(py, err, key, store, write)`.
- Change: resolve the protocol once (refusals of Phase 2's error table); every method per the
  protocol table, `Key::as_absolute` checked before any Python call, `default_metadata` and
  `finalize_metadata(update=false)` for metadata `get` omits; derived data-only access only with
  `get` and `listdir` and without `data_access = False`.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-py --no-default-features --lib store::` —
  `method_table_records_presence_and_kind`, `data_access_is_derived_only_with_get_and_listdir`,
  `get_stored_metadata_needs_a_provided_get_metadata`.
- Rollback: revert; not reachable from Python yet.

### Step 4: the store factory, registration and the build guard

- Files / symbols: new `src/store/factory.rs` (`PyStoreType`, `PyStoreFactory`);
  `src/environment.rs` (`Declarations.store_types`, `EnvCell::build`, `envref`, `invalidate`,
  `shutdown`, a thread-local `BUILDING` list, `Environment.register_store_factory`).
- Change: `build()` passes `ChainedStoreFactory` of `default_store_factory()` then
  `PyStoreFactory` to `with_store_config`. Registration checks the name against
  `default_store_factory().resolve` and the registered types, calls `_declare`, stores the type,
  marks `Pending`, keeps `declarations.store`. `BUILDING` is pushed after the lock is taken and
  popped after `build()`; `envref`, `invalidate` and registrations raise `ConfigurationError` for
  their own cell before `lock_py_attached`. `shutdown()` `mem::take`s `store` and `store_types` and
  drops them after the guard.
- Depends on: Steps 1-3.
- Proof: `test_STORE_python_protocol.py`, `test_STORE_registration.py`, and on 3.14t
  `test_FREETHREAD_concurrency.py` unchanged.
- Rollback: revert Steps 4-6 together; Steps 1-3 stand alone.

### Step 5: the `lq.Store` handle, `env.store` and `store_types()`

- Files / symbols: new `src/store/handle.rs` (`Store`, `StoreSource`, `Store.from_object`,
  `close`); `src/environment.rs` (`#[getter] store`, `store_types`).
- Change: the Phase 2 handle; an `Environment` source resolves `cell.envref(py)` on the calling
  thread before an `_async` future is spawned; a `Direct` handle owns a lazy loop pool stopped by
  `close()`, on drop or at exit.
- Depends on: Step 4.
- Proof: `test_STORE_conformance.py`, `test_STORE_configured.py`.
- Rollback: with Step 4.

### Step 6: exports, docstrings and the conformance row

- Files / symbols: `src/lib.rs`, `python/liquers/__init__.py` (`Store`, `StoreArgumentInfo`),
  `tests/test_DOCS_docstrings.py` coverage, `liquers-py/README.md` `STORE` row (directions 1-3,
  `STORE18` `NA`).
- Change: register and export the classes; a docstring on every new class and method.
- Depends on: Step 5.
- Proof: `test_DOCS_docstrings.py`; the whole `bash scripts/test-liquers-py.sh`.
- Rollback: with Step 4.

### Step 7: follow-up PR for the web store with only `get`

- Files / symbols: `liquers-web/src/store/` (`JsStore`), issue `WEB-JS-STORE-WITH-ONLY-GET-CANNOT-BE-QUERIED`.
- Change: in its own PR after this one merges, give `JsStore` the same "only `get` is
  mandatory" rule (Decision 3); `WEB-JS-STORE-BYTES-WITHOUT-METADATA-READ-AS-BINARY` after
  verifying it.
- Depends on: Step 6 merged.
- Proof: `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`.
- Rollback: revert that PR alone.

## Testing Plan

Each step runs its own proof. After Step 6: `bash scripts/test-liquers-py.sh` on 3.10, 3.14 and
3.14t (CI), and `cargo test -p liquers-py --no-default-features --lib`. No `cfg(feature)` or
optional dependency changes, so `scripts/check-build-matrix.sh` is not needed; `liquers-core`,
`liquers-store` and `liquers-lib` are untouched (their suites run in CI).

## Rollback Plan

Revert the implementation commits; nothing persistent changes (no file format, no core API).
Steps 1-3 can land alone (internal refactor and unused adapter); Steps 4-6 land together, since a
registered factory without the handle is reachable only through queries.

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
