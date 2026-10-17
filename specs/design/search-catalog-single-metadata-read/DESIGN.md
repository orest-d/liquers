---
id: SEARCH-CATALOG-SINGLE-METADATA-READ
kind: design
title: The catalog takes each file's info and version from one metadata read
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [lib/commands, core/assets]
issues: [SEARCH-CATALOG-READS-EACH-FILE-METADATA-TWICE]
created: 2026-10-17
---
# The catalog takes each file's info and version from one metadata read

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-17 backlog compaction. Phases 1-4
are written but not approved, and nothing is implemented.

## Phase 1: High-Level Design

### Purpose

`liquers-lib/src/search/catalog.rs` `infos` calls `search::file_version` (→ `AssetManager::version`)
and then `search::info_of` (→ `AssetManager::get_asset_info`) per entry. For a stored file that is
`contains` + `get_metadata`, then `contains` + `is_dir` + `get_metadata` (the `AsyncStore` default
`get_asset_info`, `liquers-core/src/store.rs`): two metadata reads and three stats.

### Problem Example

`-R-key/docs/-/ns-search/catalog-t-f` over an `AsyncFileStore` with 5 000 files, debug, cold:
2.5 s today (1.7 s before `design/search-read-dependencies/`); 1.9 s with the version read removed
(the issue's measurement). Should be: about 1.9 s, with each recorded version taken from the same
read as the row's info.

### Scope and Acceptance Criteria

- **AC-1** One metadata read per stored file
  - WHEN the catalog lists a stored file with no live asset
  - THEN the store sees one `get_metadata` (or `get`-free metadata read) for it, and one `contains`
- **AC-2** Same answers
  - WHEN the new call is compared with `get_asset_info` + `version` on files, folders, live assets,
    recipe-only keys and absent keys
  - THEN it returns the same info and version (folders and recipes: version `None`)
- **AC-3** Consistent snapshot
  - WHEN a file is rewritten while the catalog runs
  - THEN the recorded version is that of the metadata the row's info came from (which also meets the
    existing "never newer than the info" rule)

Non-goals: `AssetInfo` carrying a version; the folder listing cost.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a `pub` method to the `AssetManager` trait in
  `liquers-core` (rule 4) and changes two crates (rule 6).
- **Leading issue:** where the combined read lives.
- **Explanation:** A working design exists under the recommended answer; the choice adds public
  API, so it is the maintainer's.
- **Open questions:**
  1. **Open design - API shape:** (a) `AssetManager::get_asset_info_and_version` (default trait
     method), (b) a `version` field in `AssetInfo` (serialized format, HTTP and bindings change).
     *Recommended:* (a), as the issue says: smaller, no format change.
  2. **Proposed resolution - store overrides:** (a) builds a stored file's info from its one
     `get_metadata`, as the `AsyncStore` default `get_asset_info` does, so a store that overrides
     `get_asset_info` *for files* is not heard. No production store does (the router forwards to
     the owner, `ReadOnlyStore` mirrors the default; only a test decorator in `store.rs` overrides
     it). *Recommended:* accept, and document it on the method; the alternative is a new
     `AsyncStore` method with router and decorator forwarding, which is larger.

### Design Dependencies

None (`search-read-dependencies` is complete; this tunes its step 6 result).

### Consolidated Findings

Folders and recipe keys keep today's path (`get_asset_info`, version `None`), so only the stored-file
branch changes. `RecordingStore` (`liquers-core/src/store/test_support.rs`) counts store calls,
which makes AC-1 a deterministic test instead of a timing.

## Phase 2: Architecture

### Solution

`liquers-core/src/assets.rs`, trait `AssetManager`, new default method beside `version`:

```rust
/// The asset info of `key` and, for a file, its version, from one read where possible.
async fn get_asset_info_and_version(&self, key: &Key)
    -> Result<(AssetInfo, Option<Version>), Error>;
```

Default body: a live asset → one read lock (a new `pub(crate)` `AssetRef` helper returning
`get_asset_info` and the metadata version together; `None` version for a directory info). Else, if
`store.contains(key)?`: `store.is_dir(key)?` → `(store.get_asset_info(key)?, None)`; otherwise one
`store.get_metadata(key)?`, from which `info` (`Metadata::get_asset_info`, key set, `is_dir =
false`, as the store default) and `version().unwrap_or_else(Version::unknown)`. Else recipe →
`(rp.get_asset_info(..)?, None)`; else `KeyNotFound`. Errors keep `version`'s rule: a store error is
`Err`, never `Version::unknown()`.

`liquers-lib/src/search/catalog.rs` `infos`: one call per key, `version.unwrap_or_else(unknown)`
dropped for files since the read is shared. `search::file_version` stays for its other caller
(`search/mod.rs`, the live-asset text read) and `info_of` for `extract.rs`. On `Err` the row fails,
as `info_of` fails today; the only difference is that a failure of the *version* read alone, which
today records `Version::unknown()`, cannot occur separately any more, since it is the same read.

Rejected: (b) `AssetInfo.version` (format change); caching metadata in the catalog (stale reads).

### Changes

`AssetManager::get_asset_info_and_version` (new default); a `pub(crate)` `AssetRef` helper;
`catalog::infos`. Docs: `specs/reference/SEARCH.md` cost note; `ASSETS.md` method list, each with a
History row.

### Risks

A store overriding `get_asset_info` for files diverges (decision 2). The mid-evaluation case
(`version_of_a_mid_evaluation_asset_falls_through_to_the_store`) must hold for the new method too.
Certainty: high.

## Phase 3: Examples and Tests

### Examples

The Problem Example; the existing `search_commands.rs` `bench_search_dependency_cost` (ignored) for
the timing.

### Tests

`liquers-core/src/assets.rs` tests:

- `info_and_version_reads_stored_file_metadata_once` — `RecordingStore` over `AsyncMemoryStore`,
  counts `get_metadata` == 1 and `contains` == 1 — AC-1
- `info_and_version_matches_separate_calls` — file, folder, live asset, recipe-only, absent — AC-2
- `info_and_version_of_mid_evaluation_asset_falls_through_to_the_store` — AC-2

`liquers-lib/tests/search_commands.rs`: existing `search_catalog_records_listing_and_file_dependencies`,
`search_catalog_metadata_columns_follow_a_rewrite`, `search_catalog_content_follows_a_rewrite` —
AC-2, AC-3.

Run: `cargo test -p liquers-core --lib assets`, `cargo test -p liquers-lib --test search_commands`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Decisions 1-2 answered (recommended: a, accept)
- [ ] 2. `liquers-core/src/assets.rs` `AssetRef` helper and `AssetManager::get_asset_info_and_version` — `cargo test -p liquers-core --lib assets`
- [ ] 3. `liquers-lib/src/search/catalog.rs` `infos` — `cargo test -p liquers-lib --test search_commands`
- [ ] 4. Re-run `bench_search_dependency_cost` with `--ignored` on the file store; record the figure in the issue's resolution
- [ ] 5. `SEARCH.md`, `ASSETS.md` and History rows; close the issue — `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-lib --lib --tests`. Rollback:
revert step 3 (the new method is additive).
