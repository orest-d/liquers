---
id: STORE-CONFORMANCE-WARNING-CLEANUP
kind: design
title: Build the store-conformance feature without warnings
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [core/store, build]
issues: [STORE-CONFORMANCE-FEATURE-BUILD-WARNINGS]
created: 2026-10-10
---
# Build the store-conformance feature without warnings

Bulk design (`specs/guides/autonomous_bulk_design.md`). Phases 1-4 are written but not approved, and
nothing is implemented.

## Phase 1: High-Level Design

### Purpose

The `store-conformance` feature of `liquers-core` builds with three warnings. Standing warnings in a
build-matrix row hide new ones, so the row should be clean.

### Problem Example

```bash
cargo check -p liquers-store --features store-conformance --lib --tests
```

Today it reports:

- `liquers-core/src/store_conformance/rules/explicit.rs` — unused `use crate::query::Key;`
- `liquers-core/src/store_conformance/rules/mod.rs` — unused `pub(crate) use rule;`
- `liquers-core/src/store_conformance/mod.rs` — `error_type_of` is never used

Should be: no warnings from `liquers-core/src/store_conformance/`.

### Scope and Acceptance Criteria

- **AC-1** Clean feature build
  - WHEN `cargo check -p liquers-store --features store-conformance --lib --tests` runs
  - THEN it emits no warning located in `liquers-core/src/store_conformance/`
- **AC-2** Behaviour unchanged
  - WHEN the conformance suites run (`cargo test -p liquers-store --features store-conformance --test store_conformance_CONF`)
  - THEN every rule reports the same outcome as before

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — tooling/hygiene in `liquers-core`, removes only `pub(crate)` and
  private items, no `pub` signature change, one crate.
- **Leading issue:** None
- **Explanation:** All three items are crate-private, and the compiler already proves them unused.
- **Open questions:** None.

### Design Dependencies

None.

### Consolidated Findings

`rule!` is a `macro_rules!` used inside `rules/mod.rs` itself, so the `pub(crate) use rule;`
re-export is unneeded there; removing the re-export does not remove the macro. `error_type_of` has
no caller anywhere in the workspace (grep), so deleting it is preferred over gating it.

## Phase 2: Architecture

### Solution

Delete the unused `Key` import in `rules/explicit.rs`, the `pub(crate) use rule;` line in
`rules/mod.rs`, and `store_conformance::error_type_of` in `mod.rs`. Rejected: `#[allow(...)]`
attributes (they hide the next real warning) and `#[cfg]`-gating `error_type_of` (no configuration
uses it).

Known-issue preflight: none open touches these files.

### Changes

Three deletions in `liquers-core/src/store_conformance/`. If removing `Key` from `explicit.rs`
turns out to break a `#[cfg(test)]` use in the same file, move the import into that test module
instead. No public item, signature, or command changes.

### Risks

A use hidden behind another feature combination would surface as a compile error in
`scripts/check-build-matrix.sh`; the validation runs the relevant rows. Certainty: high.

## Phase 3: Examples and Tests

### Examples

The Problem Example above.

### Tests

No new tests. Proof is the build plus the existing suite:

- `cargo check -p liquers-store --features store-conformance --lib --tests 2>&1 | grep store_conformance` is empty: AC-1
- `cargo test -p liquers-store --features store-conformance --test store_conformance_CONF` passes: AC-2

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/store_conformance/rules/explicit.rs` — remove unused `use crate::query::Key;` — the `cargo check` above
- [ ] 2. `liquers-core/src/store_conformance/rules/mod.rs` — remove `pub(crate) use rule;` — the `cargo check` above
- [ ] 3. `liquers-core/src/store_conformance/mod.rs` — delete `error_type_of` — the `cargo check` above, then the conformance test
- [ ] 4. Close `STORE-CONFORMANCE-FEATURE-BUILD-WARNINGS` with a resolution note — `python3 scripts/docs_index.py --check`

### Validation

`bash scripts/check-build-matrix.sh` rows for `liquers-store` and `liquers-core`; the conformance
suite; `python3 scripts/docs_index.py --check`.
