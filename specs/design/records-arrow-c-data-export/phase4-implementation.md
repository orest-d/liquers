# Phase 4: Implementation Plan — records-arrow-c-data-export

## Overview

Two PRs with one contract. PR A (Steps 1-5) builds the C Data module in `liquers-records` and
proves it with Rust tests alone. PR B (Steps 6-7) switches liquers-py's record-view transport to it,
and it starts only after `py-record-values-to-python` has landed. Prerequisite for the whole plan:
the maintainer's answer to question 1. Under (b) Steps 1-5 are the same code in
`liquers-py/src/cdata.rs`, with the feature dropped.

## Progress

- [ ] Step 1: feature, module skeleton and the two structs
- [ ] Step 2: the exporter
- [ ] Step 3: the importer
- [ ] Step 4: the release and ownership tests
- [ ] Step 5: build matrix and records documentation
- [ ] Step 6: liquers-py capsule transport
- [ ] Step 7: Python tests and Python documentation

## Implementation Steps

### Step 1: feature, module skeleton and the two structs
- Files / symbols: `liquers-records/Cargo.toml` `[features]`; `liquers-records/src/lib.rs`;
  new `liquers-records/src/cdata.rs` (`ArrowSchema`, `ArrowArray`, `Drop`, `Send`, `empty`,
  `is_released`, `from_raw`, `as_mut_ptr`); `liquers-records/src/buffer.rs`
  `AlignedBuffer::strong_count` (`#[cfg(test)]`).
- Change: `cdata = []`; `#[cfg(feature = "cdata")] pub mod cdata;`. The module doc states that it
  is the crate's single exception to the no-`unsafe` rule, cites [CDATA], and requires a
  `// SAFETY:` comment on every `unsafe` block.
- Depends on: none.
- Proof: `cargo check -p liquers-records --features cdata` and `cargo check -p liquers-records`.
- Rollback: delete the module and the feature.

### Step 2: the exporter
- Files / symbols: `cdata.rs` `export_record_batch`, private `export_column`, `SchemaPrivate`,
  `ArrayPrivate`, `release_schema`, `release_array`, `encode_metadata`.
- Change: the mapping table of Phase 2. Every node's `ArrayPrivate` owns a clone of its `Column`,
  and `buffers` holds `as_bytes().as_ptr()` of the validity, offsets, values and data buffers. The
  `Vector` node gets an `f` child that also owns the column. Root metadata: `liquers.schema`
  (`serde_json::to_string(&*batch.schema)`) and `liquers.chunk_id`. The match over `Column` is
  exhaustive, with no `_ =>`.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-records --features cdata --lib cdata::tests::format_strings_follow_the_mapping`
  and `…::export_points_at_the_batch_buffers`.
- Rollback: revert the step; Step 1 still compiles.

### Step 3: the importer
- Files / symbols: `cdata.rs` `import_record_batch`, private `import_column`, `decode_metadata`,
  `copy_bitmap`, `embedded_schema_matches` (the count, name and type rule of `formats/ipc.rs`
  `check_embedded_schema`, restated here so `cdata` does not depend on the `ipc` feature).
- Change: Phase 2 §Import. Each length is checked before a `std::slice::from_raw_parts`. Every
  error path drops the owned `ArrowArray`, which releases it.
- Depends on: Step 2 (the round-trip test uses the exporter as its producer).
- Proof: `…cdata::tests::round_trip_every_column_type`,
  `…::unsupported_formats_are_refused_and_released`, `…::import_honours_a_nonzero_offset`.
- Rollback: revert the step; export stays usable.

### Step 4: the release and ownership tests
- Files / symbols: `cdata.rs` tests `export_outlives_the_batch_and_release_frees_it`,
  `moved_child_survives_parent_release`, and a hand-written test producer with a release counter
  (shared by Step 3's refusal test).
- Change: tests only, plus fixes to the release callbacks that the tests expose.
- Depends on: Steps 2-3.
- Proof: `cargo test -p liquers-records --features cdata --lib cdata`; where a nightly toolchain
  exists, `cargo +nightly miri test -p liquers-records --features cdata --lib cdata` reports no
  undefined behaviour.
- Rollback: not needed; tests only.

### Step 5: build matrix and records documentation
- Files / symbols: `scripts/check-build-matrix.sh` `RECORDS_CONFIGS` (two rows);
  `specs/reference/RECORD_STREAMS.md` (crates table, §"Arrow interoperability", §Limits, `## History`,
  `reviewed:`).
- Change: the rows `--features cdata --tests` and `--target wasm32-unknown-unknown --features cdata`.
  In the reference, the C Data route becomes **built (feature `cdata`)**, with the mapping table and
  the metadata keys, and the crate row reads "No `unsafe` outside `cdata`".
- Depends on: Step 4.
- Proof: `bash scripts/check-build-matrix.sh` (the records rows); `grep -rn "unsafe" liquers-records/src`
  lists `cdata.rs` plus the existing doc comments only (AC-9); `python3 scripts/docs_index.py --check`.
- Rollback: revert the rows and the reference text with the code.

### Step 6: liquers-py capsule transport
- Files / symbols: `liquers-py/Cargo.toml` (`liquers-records` with `cdata`); `liquers-py/src/value.rs`
  private `ArrowExport` pyclass, `fn record_view_to_pyarrow`, and the record-view arm of `ext_to_py`
  that `py-record-values-to-python` added.
- Change: Phase 2 Part B. Before writing it, check the pyo3 0.30.1 `PyCapsule::new_with_destructor`
  signature and that `pyarrow.record_batch` accepts an object with `__arrow_c_array__` in pyarrow 14.
  If either differs, stop and revise Phase 2. The version gate parses `pyarrow.__version__`'s
  leading integer. No `unsafe` in liquers-py.
- Depends on: Step 5 and `py-record-values-to-python` merged.
- Proof: `cargo check -p liquers-py`; Step 7's tests.
- Rollback: restore the IPC-only arm; drop the dependency line.

### Step 7: Python tests and Python documentation
- Files / symbols: `liquers-py/tests/test_VALUE_records_cdata.py`; `scripts/test-liquers-py.sh`
  (installs `pyarrow>=14` beside polars and pillow, unless the py-record design already does);
  `specs/reference/PYTHON_INTEGRATION.md`, `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §RECORDS,
  `liquers-py/README.md` (History rows and `reviewed:` bumps for the first two).
- Change: the two tests of Phase 3. The documents state the transport and the pyarrow minimum.
- Depends on: Step 6.
- Proof: `bash scripts/test-liquers-py.sh -k records_cdata`; `python3 scripts/docs_index.py --check`.
- Rollback: revert with Step 6.

## Testing Plan

Steps 2-4 run `cargo test -p liquers-records --features cdata --lib cdata` after each step. PR A
ends with `cargo test -p liquers-records --all-features --lib --tests`,
`cargo test -p liquers-records --lib --tests` (feature off) and `bash scripts/check-build-matrix.sh`,
because a feature flag is added. PR B ends with `bash scripts/test-liquers-py.sh`.
`cargo test -p liquers-lib` is not affected: liquers-lib does not enable `cdata`.

## Rollback Plan

PR A is additive behind an off-by-default feature, so it is reverted alone. PR B is reverted alone,
and liquers-py returns to the IPC route. If only PR A has landed, the export is a Rust API with no
user. That is a valid state, and the issue stays open until PR B lands.

## Documentation Updates

`RECORD_STREAMS.md` (Step 5), `PYTHON_INTEGRATION.md`, `LANGUAGE-INTEGRATION_GUIDE.md` and
`liquers-py/README.md` (Step 7), each reference and guide with a History row and a `reviewed:` bump.
`specs/README.md` links this design. `specs/index.csv` is regenerated with `docs_index.py`. No
`specs/command_registry.yaml` change. The source issue is closed by the implementer, with a
resolution note naming both PRs.

## Phase 5 Entry Criteria

None — four-phase design.
