---
id: AXUM-STORE-MAKEDIR-TEST-UNIGNORE
kind: design
title: Run the axum store makedir test that is ignored for a fixed limitation
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [axum, core/store]
issues: [AXUM-STORE-MAKEDIR-TEST-IGNORED-FOR-A-FIXED-LIMITATION]
created: 2026-10-10
---
# Run the axum store makedir test that is ignored for a fixed limitation

Bulk design (`specs/guides/autonomous_bulk_design.md`). Phases 1-4 are written but not approved, and
nothing is implemented.

## Phase 1: High-Level Design

### Purpose

`liquers-axum/tests/store_api_integration.rs` `test_store_makedir` is `#[ignore]`d with the note
"Ignored because MemoryStore doesn't support directory operations". The fixture is
`AsyncMemoryStore` (`create_test_store`), whose `makedir` now records a real directory
(`CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING`, closed). The reason is stale, so coverage is lost.

### Problem Example

```bash
cargo test -p liquers-axum --test store_api_integration
```

Today: `test_store_makedir ... ignored`. Should be: `test_store_makedir ... ok`, and the test should
also check that the directory exists afterwards, since asking only whether `makedir` returned `Ok`
is exactly what missed the original P0.

### Scope and Acceptance Criteria

- **AC-1** The test runs
  - WHEN `cargo test -p liquers-axum --test store_api_integration` runs
  - THEN `test_store_makedir` is executed, not ignored, and passes
- **AC-2** The test checks the effect
  - WHEN `makedir("test/newdir")` succeeds on the fixture store
  - THEN `is_dir` on that key returns `true`

Non-goals: the axum HTTP `makedir` route and the sibling tests that discard `makedir`'s result.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — test-only change in `liquers-axum/tests/`, no interface change,
  one crate.
- **Leading issue:** None
- **Explanation:** The limitation is fixed in `liquers-core/src/store.rs` `AsyncMemoryStore::makedir`
  and covered by the store conformance rule `explicit01`; only the test annotation is stale.
- **Open questions:** None. If the run in step 1 fails, the fix stops and the failure is filed
  (Phase 4 rollback), which is the issue's own instruction.

### Design Dependencies

None.

### Consolidated Findings

Test-only. The first step proves the premise with `--ignored` before anything is edited.

## Phase 2: Architecture

### Solution

Remove `#[ignore]` and the stale `NOTE` line from `test_store_makedir`, and add an `is_dir`
assertion after `makedir`. Rejected: deleting the test (the conformance suite covers the store, but
not this fixture), and leaving it ignored with a corrected note (there is no remaining reason).

Known-issue preflight: none open touches `AsyncMemoryStore` directories.

### Changes

`liquers-axum/tests/store_api_integration.rs` `test_store_makedir` only. No production code, no
signature, no command.

### Risks

If `AsyncMemoryStore::makedir` regressed, the test fails, which is the point. Certainty: high.

## Phase 3: Examples and Tests

### Examples

The Problem Example above.

### Tests

- `test_store_makedir` (existing, un-ignored and extended): AC-1, AC-2

Run: `cargo test -p liquers-axum --test store_api_integration`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Confirm the premise — `cargo test -p liquers-axum --test store_api_integration -- --ignored test_store_makedir`. Rollback: if it fails, stop, replace the note with the real reason and file it.
- [ ] 2. `liquers-axum/tests/store_api_integration.rs` `test_store_makedir` — remove `#[ignore]` and the note, add `assert!(store.is_dir(&dir_key).await.unwrap())` — `cargo test -p liquers-axum --test store_api_integration`
- [ ] 3. Close `AXUM-STORE-MAKEDIR-TEST-IGNORED-FOR-A-FIXED-LIMITATION` with a resolution note — `python3 scripts/docs_index.py --check`

### Validation

The test command above passes with no ignored test in the file; `python3 scripts/docs_index.py --check`.
