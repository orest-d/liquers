---
id: SEARCH-READ-DEPENDENCIES
kind: design
title: ns-search results depend on the folders and files they read
form: compact
workflow: liquers-project
status: complete
readiness: ready
autofix: not-eligible
area: [lib/commands, core/assets, core/plan]
issues: [SEARCH-COMMANDS-RECORD-NO-DEPENDENCY-ON-READ-FILES]
affects_docs: [reference/SEARCH.md, guides/SEARCH_GUIDE.md, reference/DEPENDENCIES_STATUS.md]
created: 2026-10-17
---
# ns-search results depend on the folders and files they read

All four phases drafted in one pass at the user's request (2026-10-17); the approach was checked
with a throwaway prototype (Phase 2, Certainty). Pre-approved (`proceed all`) on 2026-10-17, after
Q1, Q3 and Q4 were decided.

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
- `DEPENDENT-ASSET-WEAK-REFERENCES-ARE-NEVER-PRUNED` (filed today): **overlaps** (Q4).
- `external-index-sync` (draft): **overlaps**; anything kept *between* searches belongs there.
- `liquers-mustache` (D3), `agent-memory-mvp` (AC-10, AC-21, AC-22): **required-by**.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible: two crates (rule 6), and Q3 a adds a
  `pub` method to `Context` (rule 4).
- **Leading issue:** None. Q1, Q3 and Q4 were decided by the maintainer on 2026-10-17: all as recommended (a).
- **Explanation:** The solution works end to end in a prototype that passes all `ns-search` and
  `liquers-core` tests. Its cost is measured in [`performance.md`](performance.md): with the linear
  merges below, a 5 000-file chain costs 85 ms instead of 36 ms on a memory store.
- **Decisions (resolved 2026-10-17):**
  1. **Q1 (decided: a): where a cut predecessor's run-time records reach its dependent.**
     - **(a) Recommended:** in core, where a **non-keyed** dependency's state is received
       (`Context::get_dependency_state`). This covers cut `Evaluate` steps, links and a command's own
       `Context::evaluate`, which have the same gap. It stops at keys, so it is not the transitive
       recording of the 2026-10 incident. Keyed results built through a cut persist the records an
       expanded plan already persists. Requires the linear merges (performance finding 1).
     - **(b)** in `ns-search` only: `search`, `field_extract`, `md_split` copy their input's records.
       One crate (auto-fixable), but other commands stay stale behind a cut, with no issue filed.
  2. Resolved autonomously: `catalog` records **every** listed file, not only those whose text it
     read. Its `size`, `updated`, `status` and `title` columns come from the file (AC-4). The cost is
     one version read per entry (+38 % on a file store at 5 000 files).
  3. **Q3 (decided: a): how the catalog lists a folder.** Evaluating `-R-dir/<folder>`
     builds an `AssetInfo` per entry that the catalog then builds again: +150 % on a file store.
     - **(a) Recommended:** a `pub` core helper, `Context::record_listing(&Key) -> Result<Vec<String>, Error>`,
       factored out of `Step::GetAssetDirectory`: list, register the version, record. No infos: +38 %.
     - **(b)** reuse the infos of the `-R-dir/` value, lib only: +75 %, and sequential.
  4. **Q4 (decided: a): memory.** Each non-keyed asset holds a weak reference per record,
     never pruned (`DEPENDENT-ASSET-WEAK-REFERENCES-ARE-NEVER-PRUNED`, filed). *Q* distinct searches
     over *N* files hold about *Q·N* per step. **(a) Recommended:** accept, and fix pruning separately.
     **(b)** cascade expiry through query nodes for non-keyed dependents and inherit only into keyed
     ones: O(N) memory, more code (performance.md, last section).

## Phase 2: Architecture

### Solution

Use the dependency machinery that exists. A search command records what it reads in its own
context:

- **Folders (Q3 a).** Each folder walked is read with `Context::record_listing`, the listing half of
  `Step::GetAssetDirectory`: it registers the listing version (with its three-read race handling)
  and records `-R-dir/<folder>`; `refresh_listing_version` moves it on every write or removal. It
  evaluates no member (AC-10). Under Q3 b, the folder is read by evaluating `-R-dir/<folder>`.
- **Files.** For each file listed or read, the command records `-R/<key>` with
  `AssetManager::version(key)`, which reads metadata and never evaluates. An unknown version is
  safe: any later registration of the key expires the dependent.
- **Core (Q1 a).** When `Context::get_dependency_state` receives a non-keyed dependency's state, it
  merges that state's `MetadataRecord.dependencies` into the context, keeping known versions over
  unknown ones. A keyed dependency (`-R/`) is a graph node and is not inherited. Both record merges
  (`Context` pending records, and the metadata merge in `AssetRef` finalization) become linear,
  using a `HashMap` index built per merge.

Rejected: `volatile` commands (never reused, and contagious to every context pack); an expiry
(stale inside the window); reading files
through `get_dependency_state(-R/<key>)` (evaluates recipe keys, breaking AC-10).

Known-issue preflight: `ROOT-LISTING-DEPENDENCY-KEY-HAS-TWO-SPELLINGS` does not affect Q3 a, which
records the graph's spelling directly (under Q3 b, the copied `-R-dir` records work around it). `DIRECTORY-KEY-CANNOT-BE-EVALUATED-AS-A-RESOURCE`
is unrelated (the `-R-key/` form is used). Namespaces involved: `search` only. No command signature
changes, so `specs/command_registry.yaml` is untouched, and no search command uses `version: auto`.

### Changes

`liquers-core/src/context.rs`:
- `wait_for_dependency_recording`: when `dep_key` is not a pure key, merge the received state's
  records before returning, so a failed dependency still records. `Context::add_dependency` keeps its
  signature; the pending list gains a private key index (O(1) upsert).
- New `pub async fn record_listing(&self, dir: &Key) -> Result<Vec<String>, Error>` (Q3 a), the
  listing half of `Step::GetAssetDirectory`, which then calls it.

`liquers-core/src/assets.rs`: the finalization loop over `metadata.add_dependency` becomes one
indexed merge (private helper).

`liquers-lib/src/search/mod.rs`:
- New `pub(crate) async fn record_read<E: Environment<Value = Value>>(key: &Key, context: &Context<E>) -> Version`:
  records `DependencyRecord::new(DependencyKey::from(key), version)`. A store error records
  `Version::unknown()`, which only makes invalidation more eager.
- `read_text`: brackets `AsyncStore::get` with `record_read`; if the version moved, reads again (at
  most three reads, as `Step::GetAssetDirectory`).

`liquers-lib/src/search/catalog.rs`:
- `walk`: names come from `Context::record_listing` instead of `AssetManager::listdir`.
- `infos`: in the same `CONCURRENCY`-bounded stream, reads each file's version; `catalog_view`
  records them in one batch.
- `extract.rs`, `md_split.rs`, `search_command.rs`: unchanged (they use `catalog_view`, `read_text`).

No new error kind; no lock held across an await; `folder_of` unchanged.

Documents: `SEARCH.md` gets `## Dependencies` and `## Non-evaluation` names `-R-dir/`;
`SEARCH_GUIDE.md` §7 says results are cached and recomputed on a change through Liquers, with the
uncovered cases; `DEPENDENCIES_STATUS.md`'s `Evaluate(query)` row adds "and the records its run
added". Each with a History row and a `reviewed:` bump.

### Risks

- **Cost** ([`performance.md`](performance.md)). At 5 000 files: catalog 2x on a memory store and
  +38 % on a file store (Q3 a); the first load of a stored keyed index after a restart reads every
  recorded file's metadata (1.6 s instead of 6 ms), once per process; memory is Q4.
- **Not covered (documented):** edits made outside Liquers while the manager runs; a recipe-only
  entry whose status later becomes `Ready` (a first registration is not a change, so the row keeps
  `Recipe` until something else changes); the race in
  `RUNTIME-DEPENDENCY-RECORD-AGAINST-SUPERSEDED-VERSION-IS-NOT-CHECKED`.
- **Compatibility.** No API, command, format or binding change. Results stored earlier carry no
  records and stay stale until recomputed once (noted in Phase 5).
- **Certainty: high.** A discarded prototype passed the 25 existing `search_commands` tests, probes
  for content, add/remove, chains and root, and all 48 `liquers-core` suites; a second one measured
  the cost (`performance.md`).

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

- [x] 1. `liquers-lib/tests/search_commands.rs` — add the `write` helper and the eleven tests
  above; confirm that AC-1 to AC-8 and AC-11 fail at HEAD — `cargo test -p liquers-lib --test search_commands` — 005b476
- [x] 2. `liquers-core/tests/cache_strategy.rs` — add `cut_predecessor_runtime_dependency_reaches_dependent`
  and confirm it fails with cutting on — `cargo test -p liquers-core --test cache_strategy` — 005b476
- [x] 3. `liquers-core/src/context.rs`, `assets.rs` — linear merges, inheritance in
  `wait_for_dependency_recording`, `record_listing` — `cargo test -p liquers-core --lib --tests`
  (all green, AC-12 passes) — 91f1810
- [x] 4. `liquers-lib/src/search/mod.rs` — `record_read`, bracketed `read_text` —
  `cargo test -p liquers-lib --test search_commands` — 7db6ec0
- [x] 5. `liquers-lib/src/search/catalog.rs` — `walk` through `record_listing`, batched records — the same
  command, all green — 7db6ec0
- [x] 6. Re-run the `performance.md` bench (5 000 files, memory and file store) and record it in
  Phase 5. Stop if the chain exceeds 3x HEAD or the file-store catalog 1.5x —
  `cargo test -p liquers-lib --test search_commands -- --ignored` — dccdd1c; chain 1.9x, file-store
  catalog **1.54x** (median of three, at the limit; see Phase 5)
- [x] 7. Documents — `SEARCH.md`, `SEARCH_GUIDE.md`, `DEPENDENCIES_STATUS.md`, each with a History
  row and a `reviewed:` bump — `python3 scripts/docs_index.py --check` — dccdd1c

Rollback: revert the step's commit. Under Q1 (b), step 3 becomes input-record copying in the search commands.

### Validation

`cargo test -p liquers-lib --lib --tests`, `cargo test -p liquers-records --all-features --lib --tests`,
`cargo test -p liquers-core --lib --tests`, and `bash scripts/check-build-matrix.sh`
(`search` is behind `records`). `specs/command_registry.yaml` must not change, which
`cargo test -p liquers-lib --test registry_export` confirms. On completion, close
`SEARCH-COMMANDS-RECORD-NO-DEPENDENCY-ON-READ-FILES`, and update `design/liquers-mustache` (D3) and
`design/agent-memory-mvp` if they are still open.

## Phase 5: Documentation

**Built versus approved.** As approved, in four commits (005b476 tests, 91f1810 core, 7db6ec0 search,
dccdd1c bench and documents). Search commands record each folder walked (`-R-dir/<folder>`, through
the new `Context::record_listing`, which `Step::GetAssetDirectory` now calls) and each file listed
or read (`-R/<key>`); `read_text` re-reads a file whose version moved, at most three times. In core,
`wait_for_dependency_recording` merges a non-keyed dependency's records into its dependent, and the
pending list and the finalization merge are indexed. All twelve scenarios have passing tests;
AC-1 to AC-8, AC-11 and AC-12 failed before the change.

**Differences.**
- `Context::add_dependencies` (batch, one lock) is a second new `pub` method beside
  `record_listing`; the catalog records its files with it.
- `record_read` takes the version as an argument; `read_recorded` brackets the read and records
  once, with an unknown version if the file kept moving (recording before the read would have left
  a version newer than the bytes).
- The final metadata merge of `interpreter::evaluate` was quadratic too, and uses the same
  indexed merge.
- AC-4 is tested on `updated`: the memory store records no `size`. AC-5's `search` over a folder
  matches a new file's name, because a folder input is cataloged without content.
- `submit_records_dependency_under_the_key_wait_uses` (core unit test) now allows the inherited
  command records beside the one upgraded record.

**Cost** ([`performance.md`](performance.md), last section): catalog chain 1.9x (limit 3x);
file-store catalog **1.54x**, at the 1.5x limit. The per-file version read is four fifths of it. Accepted
by the maintainer on 2026-10-17; the double read is left to `SEARCH-CATALOG-READS-EACH-FILE-METADATA-TWICE`.

**Documents** (`affects_docs`, reviewed against the code): `reference/SEARCH.md` (new
§Dependencies, §Non-evaluation), `guides/SEARCH_GUIDE.md` (§7),
`reference/DEPENDENCIES_STATUS.md` (§What a dependency record holds), each with a History row and
`reviewed: 2026-10-17`. No new reference or guide document was needed.

**Issues.** Closed: `SEARCH-COMMANDS-RECORD-NO-DEPENDENCY-ON-READ-FILES`. Filed during the work:
`NEW-FOLDER-WRITE-DOES-NOT-REFRESH-THE-PARENT-LISTING` (core; a file written into a new subfolder
is missed; not this design's scope, a defect of every `-R-dir` dependent) and
`SEARCH-CATALOG-READS-EACH-FILE-METADATA-TWICE` (cost). Filed in Phase 2:
`ROOT-LISTING-DEPENDENCY-KEY-HAS-TWO-SPELLINGS` (no longer affects search),
`RUNTIME-DEPENDENCY-RECORD-AGAINST-SUPERSEDED-VERSION-IS-NOT-CHECKED`,
`DEPENDENT-ASSET-WEAK-REFERENCES-ARE-NEVER-PRUNED`. The `liquers-mustache` (D3) and
`agent-memory-mvp` notes live on their own open PRs (#139, #140) and are updated there.

**Learning.** Any command that records at run time inside a cut predecessor was stale before this
change, not only search; the cut and expanded plans now record the same. Record merges must stay
linear once a single asset can hold thousands of records.
