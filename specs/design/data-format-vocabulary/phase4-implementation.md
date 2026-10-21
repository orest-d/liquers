# Phase 4: Implementation Plan — data-format vocabulary

## Overview

Core first, in the order vocabulary → `TypeInfo` → load path → write-path hint → validator →
constants in core `Value`; then `liquers-lib` declarations and constants; then documents. Each step
keeps the workspace building, so any prefix can land. Prerequisites: the maintainer's answers to
Phase 1 questions 1-2 (the plan assumes the recommended ones). No issue must be fixed first.
`polars-dataframe-ipc` may land before or after; its four spellings are already in the table.

## Progress

- [ ] Step 1: add `liquers-core/src/data_format.rs` with the table and helpers
- [ ] Step 2: `TypeInfo::write_only_data_formats`, builder and `can_read_data_format`
- [ ] Step 3: write-only formats degrade on the load path
- [ ] Step 4: the write-path refusal names an unknown format
- [ ] Step 5: `liquers-validate` warns on an unknown filename format
- [ ] Step 6: core `Value` uses the constants
- [ ] Step 7: `liquers-lib` declares write-only formats and uses the constants
- [ ] Step 8: documents, issues, index

## Implementation Steps

### Step 1: vocabulary module
- Files / symbols: new `liquers-core/src/data_format.rs`; `liquers-core/src/lib.rs` (`pub mod
  data_format;`).
- Change: constants, `TEXT_FORMATS`, `DataFormatInfo`, `DATA_FORMATS` (Phase 2 table),
  `base_format`, `data_format_info`, `canonical_data_format`, `is_known_data_format`,
  `suggest_data_format` (private two-row Levenshtein, no dependency); tests 3-5.
- Depends on: none.
- Proof: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib data_format::` (AC-2).
- Rollback: delete the module.

### Step 2: `TypeInfo` write-only formats
- Files / symbols: `liquers-core/src/type_system.rs` `TypeInfo`, `TypeInfo::new`,
  `with_write_only_data_formats`, `can_read_data_format`, `TypeRegistry::can_read_data_format`;
  remove the private `base_format` and import `data_format::base_format`; fix the doc comment of
  `supported_data_formats` ("written").
- Change: as Phase 2; tests 8-9.
- Depends on: 1.
- Proof: `cargo test -p liquers-core --lib type_system::` (AC-4 core half, AC-9); `cargo check -p
  liquers-lib -p liquers-axum` (builder-only construction elsewhere).
- Rollback: revert; no caller uses the field yet.

### Step 3: load path
- Files / symbols: `liquers-core/src/assets.rs` `deserialize_stored_value`, `degraded_reason`
  (new `data_format` parameter), its callers in `AssetData::try_fast_track` and
  `DefaultAssetManager::get_any_status`, the `degraded_reason` unit test.
- Change: `Ok(None)` when the registered type cannot read the stored format; the reason text
  "stores data format '{f}', which is write-only for this type".
- Depends on: 2.
- Proof: `cargo test -p liquers-core --lib assets::` and `--tests` (nothing declares write-only
  yet, so behaviour is unchanged; the reason test covers the new branch).
- Rollback: revert; the field stays inert.

### Step 4: write-path hint
- Files / symbols: `liquers-core/src/assets.rs` `validate_metadata_hard`; test 14 in
  `liquers-core/tests/type_consistency.rs`.
- Change: append the unknown-format sentence and suggestion to the existing message.
- Depends on: 1.
- Proof: `cargo test -p liquers-core --test type_consistency` (AC-7).
- Rollback: revert the message change.

### Step 5: validator warning
- Files / symbols: `liquers-core/src/validate/report.rs` `WarningSource::UnknownDataFormat`,
  `collect_step_warnings` (`Step::Filename` arm), the text renderer's match; test 15 in
  `liquers-core/tests/validate_integration.rs`.
- Change: as Phase 2; explicit match arms, no `_ =>`.
- Depends on: 1.
- Proof: `cargo test -p liquers-core --test validate_integration` (AC-1); then
  `cargo run -p liquers-core --features cli --bin liquers-validate -- -- '-R/data/sales.csv/-/ns-pl/head-5/data.jsonn'`
  prints the warning and exits 0.
- Rollback: revert; reports return to `Ok`.

### Step 6: constants in core `Value`
- Files / symbols: `liquers-core/src/value.rs` `Value::type_descriptions` (`TEXTUAL` →
  `TEXT_FORMATS`), `Value::as_bytes`, `Value::deserialize_from_bytes`; test 6.
- Change: literals → constants; no arm added or removed.
- Depends on: 1.
- Proof: `cargo test -p liquers-core --lib value::` — test 6 (AC-3) and the existing `vts7.*`
  tests (AC-8). If `core-value-json-identifier-dispatch` landed first, its new arms use `JSON` too.
- Rollback: revert (pure refactor).

### Step 7: `liquers-lib`
- Files / symbols: `liquers-lib/src/value/mod.rs` `ExtValue::type_descriptions` (`RecordView`:
  `html` write-only; polars-less `parquet` write-only); `liquers-lib/src/value/simple.rs`
  `as_bytes` / `deserialize_from_bytes` literals → constants; tests 7, 10, 11 (11 compares the
  round-trip failures with `can_read_data_format` instead of its local list), 12, 13 (new
  `liquers-lib/tests/write_only_format_reload.rs`).
- Depends on: 2, 3, 6.
- Proof: `CARGO_INCREMENTAL=0 cargo test -p liquers-lib --lib --tests` (AC-3 to AC-6, AC-8);
  `cargo test -p liquers-lib --no-default-features --features records-parquet --lib --tests`.
- Rollback: revert; core keeps working with no write-only declaration.

### Step 8: documents and records
- Files: `specs/reference/VALUE_TYPE_SYSTEM.md`, `specs/guides/TYPE_SYSTEM_GUIDE.md` (History
  rows, `reviewed:`), `.claude/skills/liquers-validate/SKILL.md`; close
  `TYPE-INFO-CANNOT-DECLARE-WRITE-ONLY-FORMATS` and `DATA-FORMAT-CONSTANTS-AND-TOOLING` (the
  latter with a resolution listing the follow-up issues for points 3-6).
- Depends on: 1-7.
- Proof: `python3 scripts/docs_index.py --check`.
- Rollback: revert the documents.

Steps 4, 5 and 6 are independent of each other after step 1 and may run in parallel.

## Testing Plan

After each step its proof command. At the end: `CARGO_INCREMENTAL=0 cargo test -p liquers-core
--lib --tests`; `cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh`
(a new `cfg(all(feature = "records-parquet", not(feature = "polars")))` and a changed `match` on
`ExtValue` declarations, wasm rows included); `cargo check -p liquers-axum --tests`; the
`liquers-py` and `liquers-web` builds as `CLAUDE.md` describes (they construct `TypeInfo` with the
builder, so only a compile check is needed); `cargo fmt --check` on the changed files.

## Rollback Plan

Whole change: revert in reverse step order. Partial landing is safe at any step boundary: steps
1-6 change no stored data and no default behaviour; step 7 is what makes `html` write-only.

## Documentation Updates

As step 8. `specs/command_registry.yaml` unchanged. `specs/index.csv` regenerated by
`python3 scripts/docs_index.py` when the issues close. Final diff review: only the files named in
steps 1-8, no debug output, no edits to `liquers-records`, polars or image codecs.

## Phase 5 Entry Criteria

None — four-phase design.
