---
id: ROOT-LISTING-DEPENDENCY-KEY-SPELLING
kind: design
title: The root directory listing dependency has one spelling, the query's
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: not-eligible
area: [core/assets]
issues: [ROOT-LISTING-DEPENDENCY-KEY-HAS-TWO-SPELLINGS]
created: 2026-10-17
---
# The root directory listing dependency has one spelling, the query's

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-17 backlog compaction. Phases 1-4
are written but not approved, and nothing is implemented.

## Phase 1: High-Level Design

### Purpose

`DependencyKey::from_dir_key(&Key::new())` (`liquers-core/src/metadata.rs`) gives `-R-dir/`, while
a dependency recorded from the query `-R-dir` (`From<&Query>`, its encoding) is `-R-dir`. The
dependency manager keys its maps by the string, so the root listing is two nodes:
`refresh_listing_version` (`liquers-core/src/assets.rs`) refreshes only `-R-dir/`, and a dependent
that recorded `-R-dir` is never expired. `-R-dir/` is also not a parseable query, so
`DependencyKey::to_query` fails on it. Every listing node should have exactly one spelling.

### Problem Example

```rust
// A command reads the root listing through its context.
ctx.get_dependency_state(&parse_query("-R-dir")?).await?;   // records "-R-dir"
manager.set_binary(&parse_key("new.txt")?, b"x", meta).await?;
// refresh_listing_version(&Key::new()) registers "-R-dir/"; the dependent of "-R-dir" stays Ready.
```

Should be: the dependent is expired. `DependencyKey::from_dir_key(&Key::new()).to_query()` should
succeed.

### Scope and Acceptance Criteria

- **AC-1** One spelling for the root listing
  - WHEN `from_dir_key` is given the root key
  - THEN it returns `-R-dir`, equal to `DependencyKey::from(&parse_query("-R-dir")?)`, and
    `to_query` succeeds on it
- **AC-2** A root write expires a query-recorded dependent
  - WHEN an asset records the query `-R-dir` and a file is then written at the store root
  - THEN the asset is expired with root `-R-dir`
- **AC-3** Old records still work
  - WHEN a stored record spelled `-R-dir/` (or `-R/` for the root key) is loaded
  - THEN it is registered under the canonical spelling, so AC-2 holds for it too
- **AC-4** Non-root folders are unchanged
  - WHEN `from_dir_key` is given `a/b`
  - THEN it still returns `-R-dir/a/b`

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible — changes the string written into stored dependency records
  for the root listing (rule 4, serialized data), although old records stay readable.
- **Leading issue:** None
- **Explanation:** `-R-dir/` cannot be the canonical form, since it does not parse as a query and
  the query path cannot produce it; `-R-dir` is what every other path already accepts
  (`is_dir_key`, `dir_key`).
- **Open questions:** None.

### Design Dependencies

- `overlaps` `new-folder-write-ancestor-listing-refresh` (weak: that design's walk reaches the
  root listing; it works with either spelling, but its root test is clearer after this one).

### Consolidated Findings

One construction site (`from_dir_key`), plus `From<&Key>` for the root, plus normalization of old
records on load. Producers all go through `from_dir_key` (`plan.rs` `GetAssetDirectory` scan,
`Context::record_listing_with_version`, `refresh_listing_version`), so they change together.

## Phase 2: Architecture

### Solution

- `DependencyKey::from_dir_key`: `-R-dir` when `key.is_empty()`, else `-R-dir/{key}` as today.
- `From<&Key> for DependencyKey`: `-R` for the root key, else unchanged (`is_pure_key` already
  accepts `-R`).
- A `pub(crate) fn DependencyKey::canonical(self) -> Self` mapping `-R-dir/` to `-R-dir` and `-R/`
  to `-R`, applied in `DependencyManager::load_from_records` to each `record.key` (and in
  `track_asset`'s non-keyed branch), so records written before the change join the canonical node.
- The `DependencyKey` doc comment notes the root spellings.

Rejected: making the query path produce `-R-dir/` (unparseable); normalizing in `Deserialize`
(changes every reader of the type, including round-trip tests, for no gain over normalizing where
records enter the graph).

Known-issue preflight: `DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED` (closed) did
not consider the root; nothing open.

### Changes

`liquers-core/src/metadata.rs` (`from_dir_key`, `From<&Key>`, `canonical`, doc comment);
`liquers-core/src/dependencies.rs` (`load_from_records`, `track_asset`);
`specs/reference/DEPENDENCIES_STATUS.md` (the spelling, History row). No `pub` signature changes.

### Risks

Tests that assert the literal `-R-dir/` for the root (none found by `grep`; re-check at step 1).
A stored record spelled `-R-dir/` and audited by `stored_dependency_state` resolves through
`dir_key`, which accepts both. Certainty: high.

## Phase 3: Examples and Tests

### Examples

The Problem Example.

### Tests

- `metadata::tests::root_dir_dependency_key_is_the_query_encoding` — AC-1, AC-4
- `metadata::tests::canonical_maps_old_root_spellings` — AC-3 (unit)
- `liquers-core/tests/dependency_audit_integration.rs`
  `root_write_expires_a_dependent_of_the_root_listing_query` — a command calls
  `ctx.get_dependency_state(-R-dir)`; a root write expires it — AC-2
- `liquers-core/tests/dependency_audit_integration.rs`
  `stored_record_with_old_root_spelling_is_expired_by_a_root_write` — store a dependent whose
  record says `-R-dir/`, load it, write at the root — AC-3

Run: `cargo test -p liquers-core --lib metadata::tests` and
`cargo test -p liquers-core --test dependency_audit_integration`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Failing tests above; `grep -rn '"-R-dir/"' liquers-core liquers-lib` for literal
  asserts — commands above (AC-1, AC-2, AC-3 fail)
- [ ] 2. `liquers-core/src/metadata.rs` `from_dir_key`, `From<&Key>`, `canonical` — unit tests pass
- [ ] 3. `liquers-core/src/dependencies.rs` `load_from_records`, `track_asset` — integration tests
  pass. Rollback: revert steps 2-3 together.
- [ ] 4. `DEPENDENCIES_STATUS.md` line, History row, `reviewed:`; close the issue —
  `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests` (search
catalog over the root). Final diff review.
