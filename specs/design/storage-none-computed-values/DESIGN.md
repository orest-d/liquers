---
id: STORAGE-NONE-COMPUTED-VALUES
kind: design
title: A computed value is not written into a mount that keeps no metadata
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/store, core/assets]
issues: [RECIPE-RESULT-UNDER-STORAGE-NONE-READS-AS-SOURCE]
created: 2026-10-20
---
# A computed value is not written into a mount that keeps no metadata

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented.

## Phase 1: High-Level Design

### Purpose

A mount configured with `metadata: { storage: none }` re-derives every record from the data, so it
cannot remember that a file was *computed*. Writing a recipe's result there turns it into a user
value: it is served stale across restarts and `remove` deletes it as a document. The asset manager
should know which mounts cannot hold a computed value and not write one there.

### Problem Example

Persists at HEAD (18732dd), by code reading: `MetadataPipelineStore` over `NoStorage`
(`liquers-core/src/store/storage.rs`: `get` → `None`, `plan` → no writes), so the status of
`notes/summary.txt` is synthesized on read (`Source`, from `AsyncStore::finalize_metadata`). With a
recipe `summary.txt` in `notes/recipes.yaml` and the `memory/liquers-specs.yaml` layout (a writable
checkout mount with `storage: none`):

1. `-R/notes/summary.txt` evaluates; `AssetRef::save_to_store` writes the bytes (`Ready` is dropped).
2. A new process reads `-R/notes/summary.txt`: `try_fast_track` sees `Source`, serves the old bytes
   even after the recipe's inputs changed — no dependency records survived either.
3. `AssetManager::remove` sees `Source` + recipe → `RemoveAction::Delete`, not `DropComputed`;
   `get_asset_info` then reports what the store synthesizes, never `Recipe`.

The test `recipe_results_drop_back_to_recipe` (`liquers-lib/tests/mem_commands.rs`) avoids the case
by using a mount with stored metadata.

Should be (recommended, Q1): step 1 does not write — the asset is `Ready` in memory and its metadata
logs why it was not stored; step 2 recomputes; step 3 drops the live value and reports `Recipe`.

### Scope and Acceptance Criteria

- **AC-1** A store says whether it keeps records
  WHEN `AsyncStore::keeps_metadata(key)` is asked
  THEN a pipeline over `storage: none` answers `false`; every other in-tree store `true`; a router answers its owning mount's answer, and `SharedStore` / `ReadOnlyStore` forward
- **AC-2** A computed value is not written there
  WHEN a keyed asset with status `Ready`, `Expired` or `Volatile` is persisted to a key whose store does not keep metadata
  THEN nothing is written, the asset stays usable in memory, and its metadata log has an info entry naming the key and the reason
- **AC-3** User values are still written
  WHEN `set_state` / `set_binary` write a `Source` or `Override` value to such a key
  THEN it is written exactly as today
- **AC-4** The key drops back to its recipe
  WHEN `remove` is called on a computed key in such a mount
  THEN the live asset is dropped, nothing in the store is touched, and `get_asset_info` reports `Recipe`
- **AC-5** It is documented
  WHEN `STORE_CONFIG_FSD.md` (the `storage: none` section) or `AGENT_MEMORY_SERVICE.md` is read
  THEN they say a `storage: none` mount holds user values only and recipe results are recomputed per process

Non-goals: deriving `Ready` for a stored file a recipe can make (Q1, option B); refusing such a
write with an error; cleaning up files written before this change (a `Source` there now shadows the
recipe; the release note says to delete them); `liquers-web` stores (keep the default `true`).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a `pub` method to the `AsyncStore` trait (rules 3-4).
- **Leading issue:** Q1.
- **Explanation:** the mechanism (a per-key store answer read by the asset manager, like
  `version_verification`) is fully planned; the open choice is what a recipe result in such a mount
  should be.
- **Open questions:**
  1. **Open design - recipe results in a no-metadata mount:** **A** (recommended) never write them:
     always correct, user files at a recipe's key are never overwritten, but results do not appear on
     disk. **B** write them as exports and never trust them on read (a recipe-made key in such a
     mount is not fast-tracked; `remove` deletes the file): files appear for git, but a user's file
     at a recipe's key is recomputed over. `agent-memory-derived` (draft) wants `specs/index.csv`
     as a recipe; under A it is served by the asset, not committed as a file.
  2. **Proposed resolution - name:** `keeps_metadata(&self, key: &Key) -> bool` on `AsyncStore`,
     backed by `MetadataStorage::keeps_records(&self) -> bool` (default `true`, `NoStorage` `false`),
     rather than comparing the storage's `name()` to `"none"`.

### Design Dependencies

- `agent-memory-derived` (draft) — **required-by**: its index recipes run in `storage: none`
  checkouts; it must know Q1's answer before its Phase 2.
- `agent-memory-mvp` (complete, E4) — origin; not extended.
- `store-conditional-write` — **overlaps**: both add defaulted `AsyncStore` methods that every
  wrapper must forward; no order, but the wrapper audit is the same list.

## Phase 2: Architecture

### Solution

1. **Store answer.** `AsyncStore::keeps_metadata(&self, _key: &Key) -> bool { true }`, sync, a
   policy read like `version_verification`. Overrides: `MetadataPipelineStore` →
   `self.storage.keeps_records()`; `AsyncStoreRouter` → `find_store(key)`'s answer, `true` when no
   member owns the key; `SharedStore`, `ReadOnlyStore` → forward. `MetadataStorage::keeps_records`
   defaults `true`; `NoStorage` overrides `false`.
2. **Write guard.** In `AssetRef::save_to_store`, beside the `stored()` skip: when the asset's status
   is `Ready`, `Expired` or `Volatile` and `!store.keeps_metadata(key)`, add
   `LogEntry::info("Not stored: the mount holding '<key>' keeps no metadata, so a computed value would read back as a user value")`
   and return `SaveOutcome::Skipped`. Statuses written by `set_state` / `set_binary` (`Source`,
   `Override`, `Expired`, `Error`) are not routed here and are unaffected; `Error` metadata-only
   writes from those paths stay as they are.
3. **Removal.** Nothing to change: with nothing stored, `remove` finds the live `Ready` asset,
   chooses `DropComputed`, and `stored_metadata` is `None`, so no store call is made (AC-4).

Rejected: the pipeline refusing a `Ready` record in `set` (every evaluation would log a persistence
warning); the pipeline answering `Ready` for keys a provider can make (a store cannot see recipes;
layering); option B (Q1).

Known-issue preflight: `STORE-WRITE-HAS-NO-PRECONDITION` (design `store-conditional-write`) adds
trait methods to the same wrappers — coordinate, no block. No command, query or serialized format
is altered.

### Changes

`liquers-core/src/store.rs` (trait method, router and `SharedStore` overrides),
`store/storage.rs` (`MetadataStorage::keeps_records`, `NoStorage`), `store/pipeline.rs` (override),
`store/read_only.rs` (forward), `assets.rs` (`save_to_store` guard). Documents:
`STORE_CONFIG_FSD.md` (`storage: none` paragraph), `STORE_SEMANTICS.md` (one line on
`keeps_metadata` beside `version_verification`), `AGENT_MEMORY_SERVICE.md`,
`STORE_IMPLEMENTATION_GUIDE.md` §2 (forward it in a wrapper); History rows, `reviewed:`.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A wrapper outside core does not forward and answers `true` | `liquers-store` / `liquers-web` wrappers audited in step 2; default is today's behaviour | medium |
| Files already written by earlier builds shadow the recipe | release note; `remove` deletes them (`Source` + recipe) | high |
| Recomputation per process is costly for a large recipe | intended under A; a mount with stored metadata keeps results | high |

## Phase 3: Examples and Tests

### Examples

The Problem Example under A: the first evaluation's metadata log contains "Not stored: the mount
holding 'notes/summary.txt' keeps no metadata…"; the backend lists only `notes/recipes.yaml`.

### Tests

- `liquers-core/src/store/pipeline.rs` `mod tests`: `storage_none_does_not_keep_metadata` (`none`
  → `false`; `native`, `per_entry`, `grouped` → `true`): AC-1
- `liquers-core/src/store.rs` `mod tests`: `router_forwards_keeps_metadata` (a `none` mount and a
  memory mount; unowned key `true`), `shared_and_read_only_stores_forward_keeps_metadata`: AC-1
- `liquers-core/tests/storage_none_computed_values.rs` (new; a router with a `storage: none`
  pipeline mount over `AsyncMemoryStore`, `DefaultRecipeProvider`, a counting command):
  `computed_value_is_not_written_to_a_storage_none_mount` (backend unchanged, log entry present,
  value returned): AC-2; `fresh_environment_recomputes_instead_of_serving_a_source` (counter 2):
  AC-2; `set_state_still_writes_to_a_storage_none_mount`: AC-3;
  `remove_drops_a_computed_key_back_to_its_recipe` (`get_asset_info` status `Recipe`): AC-4
- `liquers-lib/tests/mem_commands.rs`: `recipe_results_drop_back_to_recipe_in_a_storage_none_mount`
  — the original scenario: AC-4
- Documentation (AC-5): the two documents updated, checked in the final diff review.

Run: `cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --test mem_commands`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `store/storage.rs` `MetadataStorage::keeps_records` + `NoStorage`; `store.rs` `AsyncStore::keeps_metadata`, router, `SharedStore`; `read_only.rs`; `pipeline.rs` — AC-1 unit tests — `cargo test -p liquers-core --lib keeps_metadata`. Rollback: revert (additive, defaults keep behaviour).
- [ ] 2. Audit store wrappers in `liquers-store/src/` and `liquers-web/src/store/` for forwarding (`grep -n "fn version_verification"` lists them) — `cargo check -p liquers-store`; web after `cargo clean`.
- [ ] 3. `assets.rs` `save_to_store` guard — integration tests of Phase 3 — `cargo test -p liquers-core --test storage_none_computed_values`. Depends on 1.
- [ ] 4. `liquers-lib/tests/mem_commands.rs` scenario test — `cargo test -p liquers-lib --test mem_commands`. Depends on 3.
- [ ] 5. Documents of Phase 2; close the issue with a resolution note — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`;
`cargo test -p liquers-agent` (its profiles use `storage: none`); diff review.
