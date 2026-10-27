# Phase 4: Implementation Plan — Multi-language traceback

## Overview

Core first (type, rendering, persistence, attribution), then Python, then JavaScript. Steps 1-4
are one `liquers-core` change and leave every binding working unchanged. Step 5 (Python) needs
steps 1-3. Step 6 (JavaScript) needs steps 1-3 **and** `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`
fixed first. Steps 5 and 6 can run in parallel. Written under Phase 1's recommended answers; a
different answer to Q1 or Q3 changes steps 1, 5 and 6.

## Progress

- [ ] Step 1: Segment type, payload field and rendering in `error.rs`
- [ ] Step 2: Segments in `LogEntry`
- [ ] Step 3: Attribution in the interpreter
- [ ] Step 4: Core integration test
- [ ] Step 5: Python bridge and surface
- [ ] Step 6: JavaScript bridge and surface
- [ ] Step 7: Documentation and records

## Implementation Steps

### Step 1: Segment type, payload field and rendering in `error.rs`
- Files / symbols: `liquers-core/src/error.rs` `ErrorPayload`, `ErrorPayload::new`, `Error::with_traceback`, private `command_label`.
- Change: add `TracebackSegment` and `TracebackSegment::new`; add `ErrorPayload::traceback_segments`
  (serde as in Phase 2); add `Error::with_traceback_segment`, `Error::attribute_traceback` and the
  private `render_traceback`; `with_traceback` also clears the list. Doc comments state the
  derived-string invariant. Re-export nothing new beyond the module's existing `pub` items.
- Depends on: none.
- Proof: `cargo test -p liquers-core --lib error::tests` — tests 3-6; the existing
  `traceback_round_trips_as_a_flat_key` and `error_without_traceback_key_deserializes` still pass.
- Rollback: revert the file; nothing else uses the new items yet.

### Step 2: Segments in `LogEntry`
- Files / symbols: `liquers-core/src/metadata.rs` `LogEntry`, `LogEntry::from_error`, `impl Default for LogEntry`.
- Change: the field (serde `default`, `skip_serializing_if = "Vec::is_empty"`), copied in `from_error`, empty in `Default`.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-core --lib metadata::tests::error_traceback` — test 7 and the three existing tests.
- Rollback: revert the field; `Error` keeps its segments.

### Step 3: Attribution in the interpreter
- Files / symbols: `liquers-core/src/interpreter.rs`, the `ce.execute_async(&command_key, …).map_err` closure.
- Change: apply `e.attribute_traceback(&command_key)` first, before the existing `command_key.is_none()` branch and `with_alias`.
- Depends on: Step 1.
- Proof: Step 4's test.
- Rollback: remove the call; segments stay unattributed.

### Step 4: Core integration test
- Files / symbols: new `liquers-core/tests/traceback_segments.rs`.
- Change: test 8 (`boom` and `relay` as in Phase 3).
- Depends on: Steps 1 and 3.
- Proof: `cargo test -p liquers-core --test traceback_segments`; then `cargo test -p liquers-core --lib --tests`.
- Rollback: delete the file.

### Step 5: Python bridge and surface
- Files / symbols: `liquers-py/python/liquers/_errors.py` (`TracebackSegment`, `LiquersError.__init__`, `_from_native`), `liquers-py/python/liquers/__init__.py` (`__all__`, import), `liquers-py/src/error.rs` (`error_to_pyerr`, `liquers_error_fields`, `pyerr_to_error`), `liquers-py/src/metadata.rs` (`LogEntry` getter `traceback_segments`).
- Change: as Phase 2 §Integration Points. `pyerr_to_error` loses the `format!("{inner}\n{frames}")`
  concatenation; every formatted stack becomes `with_traceback_segment(TracebackSegment::new("python", frames))`.
  `liquers_error_fields` reads `traceback_segments` when it is a sequence of three-item entries,
  otherwise ignores it; it reads `traceback` as today, so a segment-less error keeps its flat text.
- Depends on: Steps 1-3.
- Proof: `bash scripts/test-liquers-py.sh` — tests 9-13 and every existing `test_ERROR*`, in
  particular `test_ERROR_nested_python_tracebacks_are_concatenated`; its comment's "plain
  concatenation for now" is updated to name segments.
- Rollback: revert the four files; core is unaffected.

### Step 6: JavaScript bridge and surface
- Files / symbols: `liquers-web/src/error.rs` (`js_error_to_liquers`, `js_value_as_liquers_error`, `LiquersError` getter `traceback_segments` with `js_name = tracebackSegments`), `liquers-web/src/typescript.rs` (`LiquersTracebackSegment`).
- Change: where the prerequisite issue sets the stack as `traceback`, create a `javascript`
  segment instead; read `traceback` and `tracebackSegments` back from a thrown-back `LiquersError`
  (reflectively, as `errorType` is today). `query`, `key` and `position` stay out of scope (the
  separate gap in Phase 1 Consolidated Findings).
- Depends on: Steps 1-3 and `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`.
- Proof: `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` (tests
  14-15), the same with `,records`, and `./liquers-web/scripts/check-stubs.sh --build`. Run after
  `cargo clean`, separately from the native loop (`CLAUDE.md`).
- Rollback: revert the two files; JavaScript keeps the prerequisite's flat traceback.

### Step 7: Documentation and records
- Files: as Phase 2 §Documentation Architecture; `specs/issues/MULTI-LANGUAGE-TRACEBACK-IS-A-FLAT-STRING.md`.
- Change: update the documents, each `reference/` and `guides/` file with a `## History` row and a
  `reviewed:` bump; README rows; close the issue with a resolution note (§4.3); `specs/README.md`
  row; `python3 scripts/docs_index.py`, then `--check`.
- Depends on: Steps 1-6 (or Step 5 only, if step 6 waits; the web half is then an issue, §5.6).
- Proof: `python3 scripts/docs_index.py --check`.
- Rollback: revert the documents.

## Testing Plan

Steps 1-4: `cargo test -p liquers-core --lib --tests`, then `cargo test -p liquers-lib --lib --tests`
(it builds core and exercises persistence through `liquers-lib` stores) and
`cargo test -p liquers-axum` (its `error_to_detail` tests read `traceback`). Step 5: the Python
script. Step 6: the `liquers-web` loops above. No `cfg(feature)` or optional dependency changes, so
`scripts/check-build-matrix.sh` is not required; `cargo check -p liquers-core --target
wasm32-unknown-unknown` covers the wasm build of the core change. Final diff review: no
`println!`, no `unwrap`/`expect` outside tests, no `_ =>` added.

## Rollback Plan

The steps revert independently in reverse order. If only steps 1-4 land, the field exists and is
persisted but nothing fills it: harmless. If step 5 lands without step 6, Python errors carry
segments and JavaScript errors carry the prerequisite's flat text, rendered as one unknown segment
when a later crossing appends to it (AC-7).

## Documentation Updates

`specs/reference/PYTHON_INTEGRATION.md`, `specs/guides/PYTHON_GUIDE.md`,
`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` (History rows, `reviewed:` bumps);
`liquers-py/README.md` and `liquers-web/README.md` `ERROR` rows; `specs/README.md`;
`specs/index.csv` regenerated. `specs/command_registry.yaml` is unchanged.

## Phase 5 Entry Criteria

None — four-phase design.
