---
id: SEARCH-READ-DEPENDENCIES
kind: design
title: ns-search results depend on the folders and files they read
form: compact
workflow: liquers-project
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [lib/commands, core/assets, core/plan]
issues: [SEARCH-COMMANDS-RECORD-NO-DEPENDENCY-ON-READ-FILES]
affects_docs: [reference/SEARCH.md, guides/SEARCH_GUIDE.md, reference/DEPENDENCIES_STATUS.md]
created: 2026-10-17
---
# ns-search results depend on the folders and files they read

All four phases drafted in one pass at the user's request (2026-10-17), none approved yet; the
approach was checked with a throwaway prototype (Phase 2, Certainty).

## Phase 1: High-Level Design

### Purpose

A search result is a cached asset, so it must record what it read or it is served stale. Context
packs and indexes (`agent-memory-mvp`, `liquers-mustache`) are built on it and rely on recomputation.

### Problem Example

A store holds `specs/issues/C-STORE.md` (`# Store listing … Nothing to see here.`). Evaluate:

```
-R-key/specs/issues/-/ns-search/catalog-t-t
```

The row for `C-STORE.md` has the content `Nothing to see here.`. Rewrite the file through Liquers
(`AssetManager::set_binary`) with `Changed text.` and evaluate the same query again.

- **Today:** the cached asset is returned, `Ready`, with the old content; its metadata records only
  the command's own keys. Adding or removing a file is not seen either, nor by any query built on it.
- **Expected:** the write expires the catalog, and the second evaluation shows `Changed text.`.

### Scope and Acceptance Criteria

In scope: `catalog`, `field_extract`, `md_split`, `search` over a folder. Out: `commands` (the
registry is fixed per environment) and edits behind the asset manager's back while it runs
(`ASSET-MANAGER-CANNOT-BE-SYNCHRONIZED-WITH-THE-STORE`).

- **AC-1** Content follows a rewrite
  WHEN a file under a cataloged folder is rewritten through Liquers
  THEN the next `catalog-t-t` of that folder shows the new content.
- **AC-2** Added file
  WHEN a file is added to a cataloged folder or one of its subfolders
  THEN the next catalog has a row for it.
- **AC-3** Removed file
  WHEN a file is removed through Liquers
  THEN the next catalog has no row for it.
- **AC-4** Metadata columns without content
  WHEN a file is rewritten and the catalog was evaluated with `content=false`
  THEN the next catalog shows the new `size` and `updated`.
- **AC-5** Document commands
  WHEN a file changes
  THEN `field_extract`, `md_split` and `search` over the folder are recomputed.
- **AC-6** Chain across a cut predecessor
  WHEN a file changes
  THEN `…/catalog-t-t/search-<terms>`, whose catalog prefix is a separate (cut) asset, is recomputed.
- **AC-7** Store root
  WHEN a file is added at the root of the store
  THEN `ns-search/catalog-f` (no input: the whole store) lists it.
- **AC-8** Keyed result
  WHEN a recipe key's query is a search and a file it read changes
  THEN the key is recomputed on next access, and its stored metadata lists what it read.
- **AC-9** Reuse while nothing changes
  WHEN nothing under the folder changes
  THEN the second evaluation is served from the cached asset.
- **AC-10** Still never evaluates
  WHEN a folder holds a key only a recipe declares
  THEN cataloging it runs no recipe (`SEARCH.md` §Non-evaluation).
- **AC-11** Records are visible
  WHEN a catalog is evaluated
  THEN its metadata `dependencies` hold `-R-dir/<folder>` for every folder walked and `-R/<key>` for
  every file listed.
- **AC-12** Cut equals expanded (core)
  WHEN a command in a cut predecessor records a dependency at run time
  THEN the dependent records it too and expires when it changes, as with `cut_predecessors: false`.

### Scope Changes

- **AC-6, AC-12 added (2026-10-17).** The prototype showed chains stay stale: a cut predecessor
  (`Step::Evaluate`) hands its dependent only its own query (version 0); the records its commands
  add at run time stay on the predecessor, whose expiry never reaches the dependent. Its *static*
  records are already copied (`DEPENDENCIES_STATUS.md`, record table, row `Evaluate(query)`), so this
  is a core cut/expanded equivalence gap. Triage case 1: every `SEARCH_GUIDE.md` recipe is a chain.

### Design Dependencies

- `ROOT-LISTING-DEPENDENCY-KEY-HAS-TWO-SPELLINGS` (filed today): **overlaps**; designed around.
- `RUNTIME-DEPENDENCY-RECORD-AGAINST-SUPERSEDED-VERSION-IS-NOT-CHECKED` (filed today): **overlaps**;
  a residual race shared with every runtime record.
- `external-index-sync` (draft): **overlaps**; anything kept *between* searches belongs there.
- `liquers-mustache` (D3), `agent-memory-mvp` (AC-10, AC-21, AC-22): **required-by**.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. The fix spans `liquers-core` and `liquers-lib` (rule 6), and Q1
  is open (rule 5). Once Q1 is decided, each of the two parts alone satisfies rules 1-4 and 6: a bug
  fix with no new `pub` item, no command change and no new data structure.
- **Leading issue:** Q1.
- **Explanation:** The solution works end to end in a prototype that passes all `ns-search` and
  `liquers-core` tests. One choice changes what keyed results persist, so the user should see it.
- **Open questions:**
  1. **Q1 (proposed resolution): where a cut predecessor's run-time records reach its dependent.**
     - **(a) Recommended:** in core: both `Step::Evaluate` arms copy the predecessor's records into
       the dependent's context, fixing every command that records at run time. Keyed results computed
       through a cut then persist the records an expanded plan already persists.
     - **(b)** in `ns-search` only: `search`, `field_extract`, `md_split` copy their input's records.
       One crate (auto-fixable), but other commands stay stale behind a cut, with no issue filed.
  2. Resolved autonomously: `catalog` records **every** listed file, not only those whose text it
     read. Its `size`, `updated`, `status` and `title` columns come from the file (AC-4). The cost is
     one version read per entry, measured in Phase 4 step 6.

## Phase 2: Architecture

### Solution

Use the dependency machinery that exists. A search command records what it reads in its own
context, through public API only:

- **Folders.** Each folder walked is read by evaluating `-R-dir/<folder>` through
  `Context::get_dependency_state`. `Step::GetAssetDirectory` registers the listing version (with its
  three-read race handling) and `refresh_listing_version` moves it on every write or removal. The
  listing carries `is_dir`, replacing a per-entry `AsyncStore::is_dir`, and evaluates no member (AC-10).
- **Files.** For each file listed or read, the command records `-R/<key>` with
  `AssetManager::version(key)`, which reads metadata and never evaluates. An unknown version is
  safe: any later registration of the key expires the dependent.
- **Core (Q1 a).** `Step::Evaluate` copies the predecessor's `MetadataRecord.dependencies` into the
  dependent's context with `Context::add_dependency`. That method upserts, and keeps a known version
  over an unknown one. `Step::GetAsset` is unchanged: a keyed predecessor is a graph node, and the
  nearest addressable dependency is that key.

Rejected: `volatile` commands (never reused, and contagious to every context pack); an expiry
(stale inside the window); a new `Context` listing API (`-R-dir/` already does it); reading files
through `get_dependency_state(-R/<key>)` (evaluates recipe keys, breaking AC-10).

Known-issue preflight: `ROOT-LISTING-DEPENDENCY-KEY-HAS-TWO-SPELLINGS` would leave AC-7 failing.
It is designed around: `listing` copies the `-R-dir` asset's own records (spelled `-R-dir/`, the
graph's spelling) into the caller. It does not block. `DIRECTORY-KEY-CANNOT-BE-EVALUATED-AS-A-RESOURCE`
is unrelated (the `-R-key/` form is used). Namespaces involved: `search` only. No command signature
changes, so `specs/command_registry.yaml` is untouched, and no search command uses `version: auto`.

### Changes

`liquers-core/src/interpreter.rs`:
- New private `async fn inherit_dependency_records<E: Environment>(context: &Context<E>, state: &State<E::Value>)`:
  for a `Metadata::MetadataRecord`, `context.add_dependency(record.clone())` for each dependency.
  Legacy metadata has none.
- `do_step`, `Step::Evaluate` arm, and `do_step_state`, `Step::Evaluate` arm: call it after
  `get_dependency_state`, **before** `state.value()?`, so a failed predecessor still records.

`liquers-lib/src/search/mod.rs`:
- New `pub(crate) async fn listing<E: Environment<Value = Value>>(dir: &Key, context: &Context<E>) -> Result<Vec<AssetInfo>, Error>`:
  evaluates `-R-dir` for the root, else `DependencyKey::from_dir_key(dir).to_query()?`; copies the
  listing state's records (the root workaround); returns its `SimpleValue::AssetInfo` list, any
  other value being `Error::conversion_error_with_message`.
- New `pub(crate) async fn record_read<E: Environment<Value = Value>>(key: &Key, context: &Context<E>) -> Version`:
  records `DependencyRecord::new(DependencyKey::from(key), version)`. A store error records
  `Version::unknown()`, which only makes invalidation more eager.
- `read_text`: brackets `AsyncStore::get` with `record_read`; if the version moved, reads again (at
  most three reads, as `Step::GetAssetDirectory`).

`liquers-lib/src/search/catalog.rs`:
- `walk`: names and `is_dir` come from `listing`, replacing `AssetManager::listdir` and
  `AsyncStore::is_dir`. Names are still sorted.
- `infos`: inside the same `CONCURRENCY`-bounded stream, calls `record_read` for each entry that is
  not a directory.
- `extract.rs`, `md_split.rs`, `search_command.rs`: unchanged (they use `catalog_view`, `read_text`).

Errors and async: no new error kind; a listing error propagates as `listdir`'s did; no lock is held
across an await. `folder_of` is unchanged.

Documents: `SEARCH.md` gets `## Dependencies` and `## Non-evaluation` names `-R-dir/`;
`SEARCH_GUIDE.md` §7 says results are cached and recomputed on a change through Liquers, with the
uncovered cases; `DEPENDENCIES_STATUS.md`'s `Evaluate(query)` row adds "and the records its run
added". Each with a History row and a `reviewed:` bump.

### Risks

- **Cost.** A catalog of *n* files carries *n* + folders records (persisted for a keyed result),
  one `contains` + `get_metadata` per entry (through `CONCURRENCY`), and one cached `-R-dir` asset
  per folder. Phase 4 step 6 measures 1 000 files.
- **Not covered (documented):** edits made outside Liquers while the manager runs; a recipe-only
  entry whose status later becomes `Ready` (a first registration is not a change, so the row keeps
  `Recipe` until something else changes); the race in
  `RUNTIME-DEPENDENCY-RECORD-AGAINST-SUPERSEDED-VERSION-IS-NOT-CHECKED`.
- **Compatibility.** No API, command, format or binding change. Results stored earlier carry no
  records and stay stale until recomputed once (noted in Phase 5).
- **Certainty: high.** A discarded prototype passed the 25 existing `search_commands` tests and
  probes for content, add/remove, chains and root; all 48 `liquers-core` suites passed with the core
  change. Without it the chain probe failed; without the copy in `listing`, the root probe.

## Phase 3: Examples and Tests

### Examples

Primary: the problem example. Secondary: `-R-key/specs/issues/-/ns-search/field_extract/search-expiration`
gains a row when an issue mentioning `expiration` is added. Edge cases: the store root (AC-7), a
recipe-only key (AC-10), a write outside the folder (AC-9). Queries checked with `liquers-validate`.

### Tests

In `liquers-lib/tests/search_commands.rs` (gated on `records`) except the last; a `write` helper
changes files through `AssetManager::set_binary`.

- `search_catalog_content_follows_a_rewrite` — AC-1
- `search_catalog_follows_added_and_removed_files` — AC-2, AC-3
- `search_catalog_metadata_columns_follow_a_rewrite` — AC-4
- `search_document_commands_follow_a_rewrite` — AC-5: `field_extract`, `md_split`, `search` with a
  folder input
- `search_over_a_cut_catalog_follows_a_rewrite` — AC-6: `…/catalog-t-t/search-wombat` goes from 0
  hits to 1
- `search_root_catalog_follows_a_root_write` — AC-7
- `search_keyed_recipe_follows_a_rewrite` — AC-8: a fixture `recipes.yaml` declares a key whose
  query is the catalog; new content after a rewrite, and `-R/specs/issues/C-STORE.md` in its stored
  metadata
- `search_catalog_is_reused_while_nothing_changes` — AC-9: the same `AssetRef::id()` twice, and
  again after a write outside the folder
- `catalog_never_evaluates_recipe_keys` (existing) — AC-10, kept unchanged
- `search_catalog_records_listing_and_file_dependencies` — AC-11
- `cut_predecessor_runtime_dependency_reaches_dependent` in `liquers-core/tests/cache_strategy.rs`,
  for both managers — AC-12: a command reads `-R/<file>` through `context.get_dependency_state`, a
  second command cuts the chain; after a rewrite the chain has the new value, cut and expanded.

Run with `cargo test -p liquers-lib --test search_commands` and
`cargo test -p liquers-core --test cache_strategy`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-lib/tests/search_commands.rs` — add the `write` helper and the eleven tests
  above; confirm that AC-1 to AC-8 and AC-11 fail at HEAD — `cargo test -p liquers-lib --test search_commands`
- [ ] 2. `liquers-core/tests/cache_strategy.rs` — add `cut_predecessor_runtime_dependency_reaches_dependent`
  and confirm it fails with cutting on — `cargo test -p liquers-core --test cache_strategy`
- [ ] 3. `liquers-core/src/interpreter.rs` — `inherit_dependency_records` and the two `Step::Evaluate`
  arms — `cargo test -p liquers-core --lib --tests` (all green, AC-12 passes)
- [ ] 4. `liquers-lib/src/search/mod.rs` — `listing`, `record_read`, bracketed `read_text` —
  `cargo test -p liquers-lib --test search_commands`
- [ ] 5. `liquers-lib/src/search/catalog.rs` — `walk` through `listing`, `infos` records — the same
  command, all green
- [ ] 6. Measure: a throwaway test cataloging 1 000 files with and without step 5. Record the times
  and the record count in Phase 5. If recording costs more than 2x, stop and reopen open question 2 —
  `cargo test -p liquers-lib --test search_commands -- --ignored`
- [ ] 7. Documents — `SEARCH.md`, `SEARCH_GUIDE.md`, `DEPENDENCIES_STATUS.md`, each with a History
  row and a `reviewed:` bump — `python3 scripts/docs_index.py --check`

Rollback: revert the step's commit. Under Q1 (b), step 3 becomes input-record copying in
`search_command.rs`, `extract.rs` and `md_split.rs`.

### Validation

`cargo test -p liquers-lib --lib --tests`, `cargo test -p liquers-records --all-features --lib --tests`,
`cargo test -p liquers-core --lib --tests`, and `bash scripts/check-build-matrix.sh`
(`search` is behind `records`). `specs/command_registry.yaml` must not change, which
`cargo test -p liquers-lib --test registry_export` confirms. On completion, close
`SEARCH-COMMANDS-RECORD-NO-DEPENDENCY-ON-READ-FILES`, and update `design/liquers-mustache` (D3) and
`design/agent-memory-mvp` if they are still open.
