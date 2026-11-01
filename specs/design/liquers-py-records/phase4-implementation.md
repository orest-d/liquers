# Phase 4: Implementation Plan — liquers-py-records

## Overview

This design lands second in M5, after `liquers-py-package`, whose `_extras.py` the Arrow exits
use. The work runs in this order:
1. The Rust additions in `liquers-records` and the `rowid` change, testable without Python.
2. The `Scope` plumbing, which changes signatures but not behaviour, so the existing suite proves
   it.
3. The read side, from schema classes to views and columns.
4. Construction.
5. Arrow.
6. Sources.

No issue has to be fixed first (Phase 2 preflight). `POLARS-BRIDGE-TRUNCATES-SUB-MICROSECOND-DATETIMES`
is fixed in its own PR, in parallel. Until it merges, `from_polars` of a nanosecond column truncates
as today, and `test_constructors_agree` uses microsecond data. The steps run in order; only Step 1
(Rust only) can run in parallel with Step 2.

Every step that adds classes also exports them from `python/liquers/__init__.py` (and `__all__`)
with their docstrings, since `test_DOCS_docstrings.py` checks every exported name.

## Progress

- [x] Step 1 (36cead9): `liquers.origins` in IPC, `read_chunk`, and `rowid` on it
- [x] Step 2 (ad45f3a): `Scope`, `SourceOrigin` and `PyChunkResolver`, threaded through every conversion
- [x] Step 3 (ad45f3a): the schema classes
- [x] Step 4 (ad45f3a): `RecordView`, `RecordColumn`, `RecordBuffer`, and records crossing the bridge
- [x] Step 5 (af95ce1): construction from plain Python data, with `fields=`
- [x] Step 6 (af95ce1): Arrow, pandas and polars exits and entrances
- [x] Step 7 (1ec82f1): `RecordSource`, `RecordStream` and `RecordChunk`
- [x] Step 8 (0d0d8c7): conformance rows and the README

Deviations from the steps below, found while implementing:
- Writing to a read-only buffer raises `TypeError` from `struct.pack_into` ("read-write buffer"),
  not `BufferError`; `RECORDS04` checks that and `memoryview(...).readonly`.
- A view with no fields has no rows (a `RecordBatch` takes its length from its columns).
- The `liquers.chunk_id` metadata `to_arrow` writes is the serde form of the key (its segments),
  the same JSON the IPC batch message holds, not the encoded key string.
- The polars bridge reports an unbridgeable dtype as `NotSupported`; `from_polars` maps it to
  `ConversionError`, as on the Arrow route.
- The resolver's context path is tested through a dependency cycle on the context's own query:
  only the context resolver can see that cycle.
- Manifest chunks in the tests are views: raw CSV bytes cannot be serialized as `csv`
  (`BYTES-VALUE-HAS-NO-NAMED-DATA-FORMAT`, filed).

## Implementation Steps

### Step 1: `liquers.origins` in IPC, `read_chunk`, and `rowid` on it

- Files / symbols:
  - `liquers-records/src/formats/ipc.rs`: `LIQUERS_ORIGINS_KEY`, and the writer, reader and
    `concat` paths;
  - `liquers-records/src/sources.rs`: `pub async fn read_chunk(source: Arc<dyn RecordSource>,
    resolver: Arc<dyn ChunkResolver>, index: usize) -> Result<Arc<dyn RecordView>, Error>`,
    reusing `ManifestSource`'s private `read_chunk`;
  - `liquers-records/src/formats/csv.rs`: `format_value` becomes the public
    `format_field_value`;
  - the module docs of `ipc.rs` (the `liquers.origins` key);
  - `liquers-lib/src/records/commands.rs`: `rowid`, with `chunk_id_at` / `walk_to_chunk` removed;
  - `ipc_writes_the_python_fixture` (only with `LIQUERS_UPDATE_FIXTURES=1`), which writes
    `liquers-py/tests/data/records_all_types.feather`, and `python_fixture_is_current`.
- Change: Phase 2's additions. The IPC reader fills each batch's `sources` before `read_ipc`
  concatenates. `rowid` keeps its errors' meaning; their text becomes `read_chunk`'s neutral
  wording.
- Depends on: none.
- Proof:
  - `cargo test -p liquers-records --all-features --lib` (the five `ipc.rs` tests and the two
    `read_chunk` tests);
  - `cargo test -p liquers-lib --lib --tests rowid`;
  - `cargo test -p liquers-records --lib --tests`, which runs without `ipc`.
- Rollback: revert; the key is optional to readers.

### Step 2: `Scope`, `SourceOrigin` and `PyChunkResolver`, threaded through every conversion

- Files / symbols:
  - `liquers-py/src/value.rs`: `Scope` and the `value_to_py` signature;
  - new `src/records/{mod,resolver}.rs`;
  - `asset.rs`: `State::from_core`, `AssetRef::new`;
  - `aio.rs`: the `ToPy` variants carry a `Scope`, plus `ToPy::Next` and `ToPy::View`;
  - `command.rs`: `prepare_call` always builds and invalidates the `Context` wrapper;
    `context.rs`: `Context::is_valid` (`pub(crate)`);
  - the callers in `environment.rs` and `context.rs`.
- Change: Phase 2's signatures. Record values are still refused, so nothing a user sees changes.
- Depends on: none.
- Proof:
  - `bash scripts/test-liquers-py.sh` passes unchanged, on 3.10, 3.14 and 3.14t;
  - `cargo test -p liquers-py --no-default-features --features debug-handles --lib resolver` runs
    the three `resolver.rs` tests. They build a `PyEnvironment`, register a Rust command that
    captures a `PyChunkResolver` from its context, and evaluate through it while the context is
    valid, after the command returns, and after `shutdown()`.
- Rollback: revert; signatures only.

### Step 3: the schema classes

- Files / symbols: `src/records/schema.rs` (`FieldType`, `KeyRole`, `FieldSchema`,
  `RecordSchema`, `ChunkOrigin`); `lib.rs` registers them.
- Change: Phase 2's members and constructors. `KeyRole.Data` maps to `KeyRole::None`.
- Depends on: Step 2.
- Proof: `bash scripts/test-liquers-py.sh -k "RECORDS and schema"` runs Phase 3's
  `test_schema_classes_construct`, `test_schema_to_dict_matches_liquers_schema` and
  `test_schema_refuses_duplicate_id`.
- Rollback: revert.

### Step 4: `RecordView`, `RecordColumn`, `RecordBuffer`, and records crossing the bridge

- Files / symbols:
  - `src/records/{mod,column,buffer}.rs`;
  - `value.rs`: `ext_to_py` (view or scalar, through liquers-lib's `record_view_is_single_cell`,
    made `pub`) and `py_to_value` (the same `Arc` back);
  - `asset.rs`: `State` keeps an `Option<Arc<dyn RecordView>>`, and gains `record_view()`;
  - `lib.rs` registers the classes; `live_record_handle_count` (`debug-handles`);
  - `scripts/test-liquers-py.sh` adds `pyarrow` and `pandas` to its wheel-only install loop.
- Change:
  - views, `value(row, col)`, narrowing, `to_list` / `to_dict`;
  - columns, lent when the view is batch-backed;
  - the buffer protocol;
  - the single-cell rule at conversion only.
- Depends on: Steps 1 (the fixture) and 3.
- Proof:
  - `bash scripts/test-liquers-py.sh -k "RECORDS and (views or columns)"`: every test in
    `test_RECORDS_views.py` and `test_RECORDS_columns.py`, on 3.10, 3.14 and 3.14t (the
    concurrent read on 3.14t).
- Rollback: revert; records are refused again.

### Step 5: construction from plain Python data, with `fields=`

- Files / symbols: `src/records/build.rs` (`cast`, the inference table, the `Id` null check);
  `RecordView.from_records` / `from_dict`.
- Change: Phase 1 question 9's steps, with Phase 2's details.
- Depends on: Step 4.
- Proof: `bash scripts/test-liquers-py.sh -k "RECORDS_build and not (pandas or polars or arrow)"`
  runs `test_to_list_and_to_dict_round_trip`, `test_fields_inferred_declared_or_given[from_records]`,
  the plain build edge cases, and `test_constructors_agree[from_records|from_dict]`. Rust unit tests cover `cast`: one
  per row of the cast table.
- Rollback: revert.

### Step 6: Arrow, pandas and polars exits and entrances

- Files / symbols:
  - `src/records/arrow.rs`;
  - new `python/liquers/_records.py` (`ipc_to_arrow`, `arrow_to_ipc`, `pandas_to_arrow`);
  - `RecordView.to_arrow` / `to_pandas` / `to_polars` / `from_arrow` / `from_pandas` /
    `from_polars`;
  - `py_to_value` for `pyarrow.Table` / `RecordBatch`.
- Change: Phase 2's routes and normalization; `_records.py` re-raises `require`'s
  `ModuleNotFoundError` as `lq.ConversionError`.
- Depends on: Step 5, and `liquers-py-package` Step 3 (`_extras.py`).
- Proof: `bash scripts/test-liquers-py.sh -k "RECORDS and (arrow or build)"` runs all of
  `test_RECORDS_arrow.py` and the rest of `test_constructors_agree` and
  `test_fields_inferred_declared_or_given[from_pandas]`. On 3.14, where pyarrow and pandas are
  installed, `-rs` must show none of them skipped; a leg without wheels skips them by name.
- Rollback: revert; the exits raise "not available".

### Step 7: `RecordSource`, `RecordStream` and `RecordChunk`

- Files / symbols: `src/records/mod.rs` (the source classes); `lib.rs` registers them; the
  `ToPy::Next` / `ToPy::View` handling in `aio.rs`.
- Change: Phase 2's members: `check_blocking_call`, then `block_on` with the lock released; and
  `try_lock` on the stream.
- Depends on: Step 6 (the blocking-call test covers `to_arrow`), and so Steps 1-5.
- Proof: `bash scripts/test-liquers-py.sh -k RECORDS_sources` runs every test of
  `test_RECORDS_sources.py`, including Example 2's per-chunk counts and the shutdown cases.
- Rollback: revert; sources are refused again.

### Step 8: conformance rows and the README

- Files / symbols: `liquers-py/README.md` (the RECORDS row); the `na(...)` skips for `RECORDS09`
  and `RECORDS11`; the module docs of `liquers-py/src/records/mod.rs`.
- Change: as listed. Tests that prove a conformance scenario already carry its `RECORDSnn` ID.
- Depends on: Steps 3-7.
- Proof: `bash scripts/test-liquers-py.sh` passes in full, including `test_DOCS_docstrings.py`,
  and every RECORDS ID of `LANGUAGE-INTEGRATION_GUIDE.md` appears in a test name or an `na` skip.
- Rollback: revert.

## Testing Plan

- Each step's proof as it lands.
- After Step 4 and again at the end: `bash scripts/test-liquers-py.sh` on 3.10, 3.14 and 3.14t,
  the three CI legs.
- At the end:
  - `cargo test -p liquers-records --all-features --lib --tests`, and `cargo test -p
    liquers-records --lib --tests` without features and `cargo test -p liquers-lib
    --lib --tests`;
  - `bash scripts/check-build-matrix.sh`, because `ipc.rs` sits behind `liquers-records`' `ipc`
    feature;
  - the liquers-web Node loop with `--features debug-handles,records`, after `cargo clean`, since
    liquers-web reads IPC through the same reader.

## Rollback Plan

Steps 3-8 are additive within `liquers-py`. Reverting any suffix of them leaves records refused
as they are today, or partly supported, with no data format changed. Step 2 changes signatures
only. Step 1's IPC key is optional to every reader, so files written while it was present still
read after a revert.

## Documentation Updates

Phase 2's Documentation Architecture, in Phase 5:
- `PYTHON_INTEGRATION.md` §Records and §5;
- `PYTHON_GUIDE.md`, the records chapter and §7;
- `RECORD_STREAMS.md` §Arrow IPC, §ChunkOrigin and `read_chunk`;
- `LANGUAGE-INTEGRATION_GUIDE.md` §RECORDS and §6;
- `liquers-py/README.md` and `specs/README.md`;
- the API: docstrings (with each step) and the module docs of `ipc.rs` (Step 1) and
  `liquers-py/src/records/mod.rs` (Step 8).

Each gets a History row and a `reviewed:` bump, and `specs/index.csv` is regenerated. No command
signature changes, so `specs/command_registry.yaml` stays as it is. `rowid` has no `version:
auto`, so its new body changes no `impl_version`. The two records issues close at Phase 5.

## Phase 5 Entry Criteria

- [x] Implementation finished and validated
- [x] User and review comments answered
- [x] Documentation checkable against implemented and tested behaviour
