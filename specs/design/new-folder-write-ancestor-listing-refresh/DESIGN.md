---
id: NEW-FOLDER-WRITE-ANCESTOR-LISTING-REFRESH
kind: design
title: A write that creates or empties a folder refreshes the listings above it
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [core/assets]
issues: [NEW-FOLDER-WRITE-DOES-NOT-REFRESH-THE-PARENT-LISTING]
created: 2026-10-17
---
# A write that creates or empties a folder refreshes the listings above it

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-17 backlog compaction. Phases 1-4
are written but not approved, and nothing is implemented.

## Phase 1: High-Level Design

### Purpose

Every write and removal through the asset manager calls
`AssetManager::refresh_listing_version(&key.parent())` (`liquers-core/src/assets.rs`, a default
trait method; nine call sites). It refreshes that one folder's registered listing version, and
returns at once when nothing registered it. A write that *creates* the folder also changes the
listing of the folder above, and nothing refreshes that, so a dependent of `-R-dir/a` (a plan's
`GetAssetDirectory`, `Context::record_listing`, a recursive `ns-search/catalog`) keeps a listing
without the new subfolder. The mirror case is a removal that empties a folder.

### Problem Example

```
index.txt = -R-dir/data/-/index_files        # evaluated: lists data/a.txt
set_binary("data/new/x.txt", ...)            # creates data/new
```

Today: `index.txt` stays `Ready`, without `data/new`, because `data/new` has no registered listing
and the refresh stops there. Should be: `index.txt` is expired (`Cascaded`, root `-R-dir/data`).

### Scope and Acceptance Criteria

- **AC-1** New folder
  - WHEN a write creates folder `p/n` and the listing of `p` is registered
  - THEN the listing of `p` is refreshed and its dependents expire
- **AC-2** Deep new chain
  - WHEN a write creates `p/n1/n2/x` where `n1` and `n2` are new
  - THEN the listing of `p` is refreshed
- **AC-3** Emptied folder
  - WHEN a removal empties `p/n`, and the store then no longer lists `n` in `p`
  - THEN the listing of `p` is refreshed and its dependents expire
- **AC-4** An ordinary write stays one refresh
  - WHEN a write adds or replaces a file in an existing, non-empty, registered folder
  - THEN only that folder's listing is read
- **AC-5** No cost without listings
  - WHEN no folder on the key's path has a registered listing
  - THEN no listing is read

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — bug fix in the body of a default trait method in
  `liquers-core/src/assets.rs`, a private helper, no signature or structure change, one crate.
- **Leading issue:** None
- **Explanation:** Whether a level was created or emptied can be read from its registered listing
  version and its new listing (the empty listing has a known version), so the walk stops where
  membership above cannot have changed. The issue's recommended rule is refined accordingly.
- **Open questions:** None.

### Design Dependencies

- `overlaps` `root-listing-dependency-key-spelling` (weak: the walk can reach the root listing).

### Consolidated Findings

The walk climbs past a registered level only when its old or new listing is empty (it may have
been created or removed), and past an unregistered level only when some higher level is
registered. Extra cost is at most one listing read of the nearest registered ancestor when the
levels between are unregistered.

## Phase 2: Architecture

### Solution

Rewrite the body of `refresh_listing_version(dir)` as a loop from `dir` towards the root:

1. `directory_changed(level)` on the recipe provider, as today for `dir`.
2. If `level`'s listing is registered (`get_version` is `Some(old)`): `listdir`, register
   `new = listing_version(names)`, expire as today. Continue upward only if
   `old == listing_version(&[])`, `old.is_unknown()` or `names.is_empty()`; otherwise stop.
3. If it is not registered: continue upward only if some strict ancestor's listing is registered
   (map lookups only, no I/O); otherwise stop (AC-5).
4. Stop after the root.

Soundness: a level whose listing did not become or stop being empty existed before and after the
write, so its parent's membership did not change; an absent folder lists as empty
(`AsyncMemoryStore`, `AsyncFileStore` `listdir`). The per-level body is a private free function
`refresh_one_listing(manager, level) -> Option<(Version, Version, bool)>` (registered old, new,
now empty), keeping the error branch (`eprintln!`, stop) as today.

Rejected: the issue's "parent's listing version moved" test (an ordinary new file moves its folder's
version, so it would read one more listing on most writes); having each write site pass "created"
(nine call sites, and the store, not the manager, knows whether a folder existed).

Known-issue preflight: none open touches `refresh_listing_version`.

### Changes

`liquers-core/src/assets.rs` `AssetManager::refresh_listing_version` body and doc comment, and the
private helper. `specs/reference/DEPENDENCIES_STATUS.md`: the ancestor rule; History row.

### Risks

A store where an empty folder persists (explicit `makedir`) still works: its listing becomes
empty, so the walk continues, and the parent's version simply does not move. Tests likely touched:
`liquers-core/tests/dependency_audit_integration.rs` (listing section). Certainty: high.

## Phase 3: Examples and Tests

### Examples

The Problem Example; secondary: removing `data/new/x.txt` again expires the re-evaluated index.

### Tests

In `liquers-core/tests/dependency_audit_integration.rs`, reusing `index_store`, `index_env`, `put`
and `evaluate_index`:

- `writing_into_a_new_folder_expires_the_parent_index` — AC-1
- `writing_into_a_new_deep_chain_expires_the_index` — `data/n1/n2/x.txt` — AC-2
- `emptying_a_folder_expires_the_parent_index` — remove `data/new/x.txt` — AC-3
- `ordinary_write_reads_one_listing` — a counting store wrapper over `listdir` (as the listing
  cost tests in this file count reads): writing `data/b.txt` reads only `data` — AC-4
- `write_without_registered_listings_reads_none` — same counter, no index evaluated — AC-5

Run: `cargo test -p liquers-core --test dependency_audit_integration`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. The five tests — command above (AC-1, AC-2, AC-3 fail; AC-4, AC-5 pass)
- [ ] 2. `liquers-core/src/assets.rs` `refresh_listing_version` loop and `refresh_one_listing` —
  same command, all pass. Rollback: revert the body.
- [ ] 3. `DEPENDENCIES_STATUS.md` rule, History row, `reviewed:`; close the issue —
  `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests` (the
`ns-search` catalog tests, `search_catalog_follows_added_and_removed_files`). Final diff review.
