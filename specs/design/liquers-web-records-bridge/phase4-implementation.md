# Phase 4: Implementation Plan — liquers-web-records-bridge

## Overview

Six steps in `liquers-web`, ordered so each lands and is useful alone: the clear refusal first
(the issue's "meanwhile"), then the `ns-rec` commands every page example needs, the handle
registry, the source and stream handles, the JS→Rust direction, and finally the documentation and
dispositions. Prerequisite: `web-records-single-cell-scalar` merged (same `to_js_custom` arm).
Steps 4 and 5 may run in parallel after Step 3.

## Progress

- [x] Step 1: Refuse a `RecordSource` with a message naming it
- [x] Step 2: Register the `ns-rec` commands in the web environment
- [x] Step 3: Handle registry and the batch handle's id and origin getters
- [x] Step 4: `RecordSource` and `RecordStream` handles with the async iterator
- [x] Step 5: Handles cross back through `from_js_custom`
- [x] Step 6: Declarations, README `## Conformance` rows, guide update, issue closure

## Implementation Steps

### Step 1: Refuse a `RecordSource` with a message naming it
- Files / symbols: `liquers-web/src/default_value.rs` `to_js_custom`, `RecordSource` arm.
- Change: return `Err(Error::conversion_error("RecordSource", …))` (Phase 2 text) instead of
  `Ok(None)`.
- Depends on: none.
- Proof: a temporary assertion in `records_RECORDS.rs`
  `records_a_record_source_is_refused_by_name` (replaced in Step 4).
- Rollback: revert the arm.

### Step 2: Register the `ns-rec` commands in the web environment
- Files / symbols: `liquers-web/src/builtins.rs` `register_builtin_commands`.
- Change: `#[cfg(feature = "records")] liquers_lib::register_records_commands!(cr)?;` after
  `register_core_commands!`. Record the quickstart `.wasm` size before and after in the PR.
- Depends on: none.
- Proof: `records_ns_rec_commands_run_in_the_web_environment`;
  `./liquers-web/examples-web/quickstart/build.sh`.
- Rollback: remove the line. Registration was decided by the maintainer (2026-10-17); the size is
  recorded for information.

### Step 3: Handle registry and the batch handle's id and origin getters
- Files / symbols: `liquers-web/src/records.rs` — private `RecordHandle`, `HANDLES`,
  `next_handle_id`; `LiquersRecordBatch { inner, id }`, `From<Arc<RecordBatch>>` inserts,
  unconditional `Drop` removes (live count still behind `debug-handles`); getters
  `__liquersRecordHandle`, `chunkIdJson`, `sourcesJson`.
- Depends on: none.
- Proof: existing `records05`/`records06` pass; `records_RECORDS` builds.
- Rollback: revert `records.rs`.

### Step 4: `RecordSource` and `RecordStream` handles with the async iterator
- Files / symbols: `liquers-web/src/records.rs` `LiquersRecordSource`, `LiquersRecordStream`;
  new `liquers-web/src/records_stream.js` (`asyncIterable(stream)` with `next()`/`return()`);
  `default_value.rs` `RecordSource` arm returns `LiquersRecordSource` (replacing Step 1's error).
  `stream`/`materialize` resolve the web environment (`with_global()`), build
  `EnvResolver::new(envref)`, and use `wasm_bindgen_futures::future_to_promise`. `nextBatch`
  takes the stream out of its `RefCell`, awaits `StreamExt::next`, and puts it back.
- Depends on: Step 3.
- Proof: `records07_…`, `records08_…`, `records_source_materialize_is_bounded_by_max_rows`,
  `records06_releasing_a_source_handle_releases_the_source`, `records.rs`
  `tests::next_batch_refuses_a_concurrent_call`.
- Rollback: restore Step 1's arm; the new types are unreferenced.

### Step 5: Handles cross back through `from_js_custom`
- Files / symbols: `liquers-web/src/default_value.rs` `from_js_custom`.
- Change: for an object with `__liquersRecordHandle`, read the id (a throwing getter means freed)
  and return `ExtValue::RecordView`/`RecordSource` from `HANDLES`; absent id → the freed-handle
  `ConversionError`. Non-objects and unmarked objects still return `Ok(None)`.
- Depends on: Step 3 (Step 4 for sources).
- Proof: `records01_…`, `records_a_freed_handle_is_refused_by_name`, `records_a_command_returning_its_batch_hands_rust_the_same_view`, the full Node loop (no `VALUE` regression).
- Rollback: restore `Ok(None)`.

### Step 6: Declarations, README `## Conformance` rows, guide update, issue closure
- Files / symbols: `liquers-web/src/typescript.rs` (`RecordSource`, the iterable, `chunks()`
  entry type); `liquers-web/README.md` records usage and `## Conformance` rows — `RECORDS01`, `02`,
  `04`-`08`, `10` with test names, `03` `NA` (no Arrow route; reverses if an Arrow export is
  added), `09` `NA` (JavaScript has an async model), `11` `NA` (JavaScript cannot implement the
  traits; reverses with a JS-implemented `RecordSource`); create the section if
  `web-store-conformance-tests` has not; `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §RECORDS
  "What `liquers-web` does" (History row, `reviewed:`); close
  `WEB-RECORDS-BRIDGE-ONE-WAY-AND-SOURCE-UNMAPPED` with a resolution note.
- Depends on: Steps 1-5.
- Proof: `./liquers-web/scripts/check-stubs.sh`; `python3 scripts/docs_index.py --check`.
- Rollback: documentation only.

## Testing Plan

After each step: `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --test records_RECORDS`.
At the end, after `cargo clean`: the full Node loop, `./liquers-web/examples-web/quickstart/build.sh`,
`./liquers-web/scripts/check-stubs.sh`, `cd liquers-web/tests/e2e && npx playwright test`, and the
wasm rows of `bash scripts/check-build-matrix.sh` (a new `cfg(feature = "records")` site in
`builtins.rs`).

## Rollback Plan

Steps revert independently in reverse order. If only Steps 1-2 land, the page gets a clear
refusal and the `ns-rec` commands; the remainder stays in this design's unticked steps and the
source issue stays open.

## Documentation Updates

`liquers-web/README.md`, `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` (History row and `reviewed:`
bump), `liquers-web/src/typescript.rs` declarations, `specs/index.csv` (regenerated). No change to
`specs/command_registry.yaml`: no command signature changes.

## Phase 5 Entry Criteria

- [x] Implementation finished and validated
- [x] User and review comments answered
- [x] Documentation checkable against implemented and tested behaviour
