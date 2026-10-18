---
id: WEB-RECORDS-SINGLE-CELL-SCALAR
kind: design
title: A single-cell record view reaches JavaScript as a scalar
form: compact
status: complete
gh_pr: [153]
readiness: ready
autofix: eligible
area: [web, records]
issues: [WEB-RECORDS-SINGLE-CELL-VIEW-CROSSES-AS-BATCH]
created: 2026-10-17
---
# A single-cell record view reaches JavaScript as a scalar

Bulk design (`specs/guides/autonomous_bulk_design.md`), merged in #145; implemented, every Phase 4
step done.

## Phase 1: High-Level Design

### Purpose

`LANGUAGE-INTEGRATION_GUIDE.md` §VALUE/RECORDS requires `RECORDS10`: a single-cell view reaches the
*language* as its own scalar. `liquers-lib` reads a one-row, one-payload-column view as its cell
(`liquers-lib/src/value/mod.rs` `record_view_is_single_cell`, used by
`ValueExtension::try_into_json_value`), but `liquers-web/src/default_value.rs`
`<ExtValue as JsExtensionBridge>::to_js_custom` materializes **every** `ExtValue::RecordView` into a
`LiquersRecordBatch` first, so the scalar path is never reached.

### Problem Example

A Rust command (or a test) produces a 1×1 view and the result crosses to JavaScript:

```rust
// schema [x: Float], one row, x = 1.5
let value = WebValue::from_record_view(Arc::new(batch_1x1) as Arc<dyn RecordView>);
let js = liquers_web::bridge::value_to_js(&value)?;
```

Today `js` is a `RecordBatch` handle (`numRows == 1`, `numColumns == 1`); the page must call
`columnCopy(0)[0]` to read `1.5`. It should be the JavaScript number `1.5`. A 2-row view must still
cross as a `RecordBatch`.

(No page query reaches a record view yet: the web environment registers no `ns-rec` commands. That
is `liquers-web-records-bridge`'s scope; this fix is observable through `value_to_js` and through any
Rust command a downstream crate registers.)

### Scope and Acceptance Criteria

- **AC-1** A single-cell view crosses as its scalar
  - WHEN a view with one row and one payload column (`Float` 1.5, `Int` 7, `Text` "a", `Bool` true) is
    converted with `value_to_js`
  - THEN the result is the JavaScript number, number, string or boolean of that cell — the same value
    `liquers-lib`'s `try_into_json_value` gives
- **AC-2** A larger view still crosses as a batch
  - WHEN a view of 2 rows × 1 column, or 1 row × 2 payload columns, is converted
  - THEN the result is a `RecordBatch` handle whose `numRows`/`numColumns` name its shape
- **AC-3** Null and binary cells keep the VALUE mapping
  - WHEN the single cell is `Null`, or `Bytes`
  - THEN the result is JavaScript `null`, or a `Uint8Array` (`VALUE04`), not `undefined` or an array
    of numbers

Non-goals: the `RecordSource` wrapper, the JS→Rust direction, and registering `ns-rec` commands
(`liquers-web-records-bridge`); the i64-beyond-2^53 policy (`WEB-OUTBOUND-INT64-BEYOND-DOUBLE-UNTESTED`,
which applies to an `Int` cell exactly as to any integer).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — bug fix restoring the guide's `RECORDS10` behaviour in
  `liquers-web/src/default_value.rs` with a private helper and one test; no new type, no `pub`
  change, one crate.
- **Leading issue:** None
- **Explanation:** The scalar meaning already exists in `liquers-lib` (`try_into_json_value`) and the
  shape predicate is five lines over public `RecordView` API; the bridge only has to ask before it
  materializes.
- **Open questions:** None.

### Design Dependencies

- `liquers-web-records-bridge` — **required-by**: that design edits the same `to_js_custom` arms and
  extends `records_RECORDS.rs`; it assumes this fix landed. Kept separate under overlap exclusion E1
  (this one is eligible for automatic fixing, that one is not). T3 (same change site) held.
- `web-outbound-int64-precision` (`WEB-OUTBOUND-INT64-BEYOND-DOUBLE-UNTESTED`) — **overlaps** (weak): an `Int` cell leaves through the
  same `serialize_to_js` path.

### Consolidated Findings

- Do not reuse `liquers-lib`'s private `record_view_is_single_cell` by making it `pub` — that is an
  interface change in `liquers-lib` (auto-fix rule 4). Mirror it privately, as `records.rs` already
  does for `is_row_valid`, with a comment naming the original.
- Converting through `try_into_json_value` keeps dates as ISO text and refuses a `UInt` above
  `i64::MAX`, exactly as native scalar reading does; only `Null` and `Bytes` need special arms,
  because `value_to_js` handles those before JSON for every other value.

## Phase 2: Architecture

### Solution

In the `ExtValue::RecordView` arm of `to_js_custom`, test the shape first. For a single-cell view:
read `value.single_cell()?`; `FieldValue::Null` → `JsValue::NULL`; `FieldValue::Bytes` →
`js_sys::Uint8Array`; anything else → `self.try_into_json_value()?` through
`crate::bridge::serialize_to_js`, mapping its error to `ErrorType::ConversionError` as
`bridge::json_to_js` does. Every other view materializes as today.

Rejected: returning `Ok(None)` and letting the standard mapping run (a `Null` cell would become
`undefined`, a `Bytes` cell an array of numbers); making `record_view_is_single_cell` `pub` (widens
`liquers-lib`); converting in JavaScript (`records_column.js`) — the decision belongs to the bridge.

Known-issue preflight: no open issue blocks. `WEB-OUTBOUND-INT64-BEYOND-DOUBLE-UNTESTED` is
independent (same path, own fix).

### Changes

- `liquers-web/src/default_value.rs`: new private `fn is_single_cell(view: &dyn RecordView) -> bool`
  (same predicate as `liquers-lib/src/value/mod.rs` `record_view_is_single_cell`: `len() == 1` and
  one payload field, or none and an `Id` field or a sole field) and private
  `fn single_cell_to_js(ext: &ExtValue, view: &dyn RecordView) -> Result<JsValue, Error>`, both
  `#[cfg(feature = "records")]`; the `RecordView` arm calls them. The comment above the arm cites
  `RECORDS10`.
- `liquers-web/tests/records_RECORDS.rs`: the `records10_*` tests below.

No signature, type, command or dependency changes. Sync throughout.

### Risks

A page that relied on reading a 1×1 result as a batch now receives a scalar — the documented
behaviour, and the reason for the issue. Predicate drift from `liquers-lib`: the test cases include
the `Id`-only shape so a divergence fails. Certainty: high.

## Phase 3: Examples and Tests

### Examples

The Problem Example; and the 2-row case returning a handle with `numRows == 2`.

### Tests

In `liquers-web/tests/records_RECORDS.rs` (Node loop, no browser API), each building the view with
the file's existing `RecordBatch::new` pattern and converting with `liquers_web::bridge::value_to_js`:

- `records10_a_single_cell_view_reaches_javascript_as_its_scalar` (AC-1): Float 1.5 → `as_f64() ==
  Some(1.5)`; Int 7 → `Some(7.0)`; Text → `as_string() == Some("a")`; Bool → `as_bool()`; an `Id`-only
  one-column view → its id.
- `records10_a_larger_view_reaches_javascript_as_a_batch_naming_its_shape` (AC-2): 2×1 and 1×2 views
  → objects whose `numRows`/`numColumns` (via `Reflect::get`) are 2/1 and 1/2.
- `records10_null_and_binary_cells_keep_the_value_mapping` (AC-3): `Null` → `is_null()`; `Bytes` →
  `is_instance_of::<Uint8Array>()`.

Run: `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --test records_RECORDS`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-web/tests/records_RECORDS.rs` — add the three `records10_*` tests; confirm AC-1 and AC-3 fail and AC-2 passes — the run command above
- [x] 2. `liquers-web/src/default_value.rs` `to_js_custom` — add `is_single_cell` and `single_cell_to_js`, call them before `materialize()` — the run command above, all `records10_*` and existing `records05`/`records06` pass. Rollback: revert the arm.
- [x] 3. Close `WEB-RECORDS-SINGLE-CELL-VIEW-CROSSES-AS-BATCH` with a resolution note; in `LANGUAGE-INTEGRATION_GUIDE.md` §RECORDS "What `liquers-web` does", state that a single-cell view crosses as its scalar and add `RECORDS10` to the tests run there (History row, `reviewed:` bump) — `python3 scripts/docs_index.py --check`

### Validation

The full Node loop `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`
(after `cargo clean` of the native target, per `CLAUDE.md`); `bash scripts/check-build-matrix.sh`
wasm rows are unaffected (no `cfg` change beyond the existing `records` gate);
`python3 scripts/docs_index.py --check`.
