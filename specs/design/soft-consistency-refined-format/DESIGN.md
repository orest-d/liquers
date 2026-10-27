---
id: SOFT-CONSISTENCY-REFINED-FORMAT
kind: design
title: The write-path soft consistency check derives the expected media type from the base format
form: compact
status: complete
gh_pr: [199]
readiness: ready
autofix: eligible
area: [core/assets]
issues: [SOFT-CONSISTENCY-CHECK-MISREADS-A-REFINED-DATA-FORMAT]
created: 2026-10-26
---
# The write-path soft consistency check derives the expected media type from the base format

> **Complete on 2026-10-27**: implemented by automatic fix in #199.

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written; Phase 4
implemented on 2026-10-27 (automatic fix).

## Phase 1: High-Level Design

### Purpose

`set_state` and `set_binary` add soft-tier warnings when the stored metadata disagrees with itself.
For a *refined* data format (`csv:comma`, `txt:utf8`) the expected media type is looked up with the
refinement still attached, so the lookup misses (`application/octet-stream`) and a consistent value
is reported as inconsistent. The rest of the system already treats a refinement as consistent with
its base: `MetadataRecord::effective_media_type` strips it, `soft_consistency_entries` compares the
extension against `data_format_base`, and `TypeInfo::supports_data_format` matches by base.

### Problem Example

Verified by reading HEAD (`70b9005`, code unchanged since `cd6d5da`):
`add_soft_consistency_warnings` and `add_soft_consistency_warnings_enum` (`liquers-core/src/assets.rs`)
both call `file_extension_to_media_type(&effective_data_format)`, whose `match` has no `"txt:utf8"`
or `"csv:comma"` arm and falls to `"application/octet-stream"` (`media_type.rs`).

`set_state(test/notes.txt, state)` on `DefaultAssetManager`, where `state` is `Value::Text("hello")`
with metadata `data_format: Some("txt:utf8")`, no media type, type `Text`. The enum form compares
`Metadata::get_media_type()` (derived from the base: `text/plain`) with the expected value
(`application/octet-stream`). Today the stored log holds
`media_type 'text/plain' differs from expected 'application/octet-stream' for data_format 'txt:utf8'`.
It should hold no such warning. `set_binary` with a `MetadataRecord` declaring `data_format:
txt:utf8` and media type `text/plain` logs the same false warning (the record form only compares a
*declared* media type).

### Scope and Acceptance Criteria

- **AC-1** Refined format, derived media type (`set_state`)
  WHEN `set_state` stores a value whose metadata declares `txt:utf8` and no media type
  THEN the stored metadata log has no "differs from expected" entry
- **AC-2** Refined format, declared consistent media type (`set_binary`)
  WHEN `set_binary` stores bytes whose record declares `txt:utf8` and media type `text/plain`
  THEN the stored metadata log has no "differs from expected" entry
- **AC-3** A real divergence is still reported
  WHEN the record declares `txt:utf8` and media type `application/x-custom`
  THEN the log has one warning naming `application/x-custom` and expecting `text/plain`, and the
  override is stored verbatim

Non-goals: a shared data-format vocabulary or a public base-format helper (`data-format-vocabulary`);
the record form's asymmetry of comparing only a declared media type (documented in the code as
deliberate); legacy metadata.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — bug fix in two private functions of `liquers-core/src/assets.rs`,
  restoring the documented rule that a refinement is consistent with its base
  (`VALUE_TYPE_SYSTEM.md` "A data format may carry a refinement"); integration tests in
  `liquers-core/tests/`; no `pub` item, type, command or format changes (rules 1-6). Priority P3.
- **Leading issue:** None
- **Explanation:** the private helper `data_format_base` already exists beside the two functions
  and is already used by `soft_consistency_entries` for the extension half of the same check.
- **Open questions:** None.

### Design Dependencies

- `query-data-format-command` — **overlaps** (weak): makes refined formats common from queries, so
  this warning would appear more often; no order.
- `data-format-vocabulary` — **overlaps** (weak): may later export a public base-format helper; this
  fix uses the existing private one and needs no change if that lands.
- `record-table-csv-refinements` — **overlaps** (weak): the other refinement defect filed by the
  same design; different crate, no order.

### Consolidated Findings

- Fix site: the `expected_media_type` line in both `add_soft_consistency_warnings` and
  `add_soft_consistency_warnings_enum`. The warning text keeps the full refined format (it names
  what the caller declared).
- A non-serializable `set_state` value still stores its metadata (`set_metadata`), so the log is
  observable in the store even when `as_bytes("txt:utf8")` is refused by the core serializer; the
  tests read `store.get_metadata`.
- Both managers (`DefaultAssetManager`, `ImmediateAssetManager`) call the same helpers, so one fix
  covers both.

## Phase 2: Architecture

### Solution

In both helpers, replace
`file_extension_to_media_type(&effective_data_format)` with
`file_extension_to_media_type(data_format_base(&effective_data_format))`.

Rejected: stripping the refinement inside `file_extension_to_media_type` (it maps file
extensions, not data formats, and is `pub`, so it would change an interface); comparing against
`MetadataRecord::effective_media_type` (that returns the declared override, so it would hide every
real divergence).

### Changes

- `liquers-core/src/assets.rs`: `add_soft_consistency_warnings`, `add_soft_consistency_warnings_enum`
  (private). No new item, no error change, sync code.
- `liquers-core/tests/type_consistency.rs`: three tests (Phase 3).
- No reference document states the bug; `VALUE_TYPE_SYSTEM.md` already states the intended rule.
  No document change.

### Risks

| Risk | Handling | Certainty |
|---|---|---|
| Likely files | `assets.rs` (two lines), `tests/type_consistency.rs` | high |
| Existing tests | `soft_consistency_entries_report_only_real_divergence` passes the expected value in directly and is unaffected; no test asserts the false warning (searched for "differs from expected") | high |
| Compatibility | only log entries change (one false warning fewer); no stored value or format changes | high |
| Unrefined formats | `data_format_base` returns the input unchanged without `:`, so every current unrefined result is identical | high |
| Recovery | revert the two lines | high |

## Phase 3: Examples and Tests

### Examples

The Problem Example is the primary example. Setup is `type_consistency.rs`'s `environment()`
(`SimpleEnvironment<Value>` over `AsyncMemoryStore`) and its `record(key, "Text")` helper.

### Tests

In `liquers-core/tests/type_consistency.rs`:

- `refined_format_is_consistent_through_set_state` — `set_state(test/notes.txt, State` of
  `Value::from("hello")` with metadata `data_format: Some("txt:utf8")`, type `Text`, status
  `Source`)`; read `store.get_metadata(&key)`; assert no log message contains
  `"differs from expected"`. Fails at HEAD. Covers AC-1.
- `refined_format_with_consistent_media_type_is_silent` — `set_binary(test/notes.txt, b"hello",
  record)` with `data_format: Some("txt:utf8")` and `with_media_type("text/plain")`; same
  assertion. Fails at HEAD. Covers AC-2.
- `refined_format_with_divergent_media_type_still_warns` — as above with
  `application/x-custom`; exactly one log message contains `"differs from expected 'text/plain'"`,
  and `stored.get_media_type() == "application/x-custom"`. Covers AC-3.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test type_consistency`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-core/tests/type_consistency.rs` — add the three tests; the first two fail at HEAD —
  `cargo test -p liquers-core --test type_consistency refined_format`
- [x] 2. `liquers-core/src/assets.rs` `add_soft_consistency_warnings` and
  `add_soft_consistency_warnings_enum` — wrap the lookup argument in `data_format_base(…)` — the
  step-1 tests pass, then `cargo test -p liquers-core --lib assets::tests::soft_consistency`
- [x] 3. Issue `SOFT-CONSISTENCY-CHECK-MISREADS-A-REFINED-DATA-FORMAT`: `status: closed` with a
  resolution note; regenerate the index — `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core --lib --tests`. Diff review: two changed lines in library code, no
`println!`, no `unwrap` outside tests.

### Implementation notes (2026-10-27)

- All three tests fail without the fix, not only the first two: at HEAD the divergent case
  warns against `application/octet-stream` (the lookup of `txt:utf8`) instead of `text/plain`,
  so the test's exact message does not match.
