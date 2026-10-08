---
id: STORE-KEY-FORMAT-SEEDING
kind: design
title: Every store seeds the data format from the key's extension when the metadata names none
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/store, web]
issues: [STORES-DISAGREE-ON-SEEDING-THE-DATA-FORMAT-FROM-THE-KEY]
created: 2026-10-08
---
# Every store seeds the data format from the key's extension when the metadata names none

Produced under [`guides/autonomous_bulk_design.md`](../../guides/autonomous_bulk_design.md) by the
2026-10-08 backlog compaction: Phases 1-4, reviewed without phase approval. Not an approval and not
an implementation.

## Phase 1: High-Level Design

### Purpose

The same bytes written under the same key with empty metadata should report the same effective
media type on every store.

### Problem Example

```rust
store.set(&parse_key("data/input.csv")?, b"a,b\n1,2\n", &Metadata::new()).await?;
store.get_asset_info(&parse_key("data/input.csv")?).await?.media_type
```

On liquers-web's HTTP store (`infer_metadata`, `liquers-web/src/store/fetch.rs`) this is
`text/csv`. On `AsyncMemoryStore` it is `application/octet-stream`, because
`AsyncStore::finalize_metadata` (`liquers-core/src/store.rs`) sets the key, size and status but no
filename, and the data format is seeded only from the filename.

### Scope and Acceptance Criteria

- **AC-1** Seeding from the key
  - WHEN a store writes data whose metadata has no filename, under a key whose last segment has an
    extension
  - THEN the stored metadata's filename is the key's last segment, so the data format and the
    effective media type derive from it
- **AC-2** Declared metadata wins
  - WHEN the written metadata already names a filename or data format
  - THEN the store keeps it
- **AC-3** Every in-tree store agrees
  - WHEN the conformance suite runs against every store
  - THEN AC-1 and AC-2 hold on each

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — it settles a store contract in `STORE_SEMANTICS.md` (the
  issue asks for the decision) and the fix spans `liquers-core` and `liquers-web` (rule 6).
- **Leading issue:** **Open design question — does a store seed the filename from the key?**
- **Explanation:** A working design is specified around the recommended answer. The alternative
  (never seed; the HTTP store stops inferring) is a smaller code change but makes hand-placed and
  uploaded files report `application/octet-stream` everywhere.
- **Open questions:**
  1. **Proposed resolution — seed in `finalize_metadata`.** When the metadata has no filename,
     `finalize_metadata` calls `with_filename(key.filename())`. It is the shared default every
     store already calls, so one change covers memory, file, OpenDAL and local-storage stores; the
     HTTP store's `infer_metadata` already does the same. This is level-2 seeding, as `with_filename`
     defines it, not a declared media-type override.

### Design Dependencies

- `overlaps` `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE` and
  `STORED-UNTYPED-FILE-OF-UNLISTED-FORMAT-CANNOT-BE-READ` (weak: a seeded data format is what lets
  an untyped stored file be read with the right format).

## Phase 2: Architecture

### Solution

In `liquers-core/src/store.rs` `AsyncStore::finalize_metadata` (default method, ≈line 356), after
`with_key`, when the metadata's filename is unset and `key.filename()` is `Some`, call
`with_filename(name.encode().to_string())`. Write the rule into `specs/reference/STORE_SEMANTICS.md`
and add a conformance rule.

Rejected: seeding at read time in `get_asset_info` (stores would still persist different metadata);
seeding in each backend (duplicates the default).

### Changes

- `liquers-core/src/store.rs` `finalize_metadata` (behaviour of a default method; no signature
  change).
- `liquers-core/tests/store_conformance_CONF.rs`: a rule for AC-1/AC-2, run on every suite.
- `specs/reference/STORE_SEMANTICS.md`: the seeding rule; History row.
- `liquers-web`: none expected beyond running its conformance suite.

### Risks

Stored metadata changes for files written without a filename; `metadata_version` is not affected.
`AsyncStoreRouter::finalize_metadata` delegates to the routed store, so it inherits the rule; the
obsolete sync `Store` trait keeps its own copy and is not changed. A backend overriding
`finalize_metadata` would not inherit it; none in-tree does today (re-check at step 1).

## Phase 3: Examples and Tests

### Examples

The Problem Example. Secondary: `data/README` (no extension) keeps no filename-derived format.

### Tests

- `conf_seeds_filename_from_key_extension` — AC-1, AC-3
- `conf_keeps_declared_filename_and_format` — AC-2, AC-3

Command: `cargo test -p liquers-core --test store_conformance_CONF`, then the liquers-web Node loop
(`CLAUDE.md`, "liquers-web").

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/store.rs` `finalize_metadata` — seeding — `cargo test -p liquers-core --lib`
- [ ] 2. Conformance rules — `cargo test -p liquers-core --test store_conformance_CONF`
- [ ] 3. `STORE_SEMANTICS.md` rule and History row; store guide status table counts
- [ ] 4. liquers-web conformance loop; issue resolution; `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-lib --lib --tests`, the web loop.
Rollback: revert the commit.
