---
id: UNREGISTERED-KEYED-WRITE-GATE
kind: design
title: A keyed mutation also stops the value writes of unregistered assets of the same key
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/assets, core/store]
issues: [LATE-SAVE-OF-AN-UNREGISTERED-KEYED-ASSET-SURVIVES-REMOVAL]
created: 2026-10-26
---
# A keyed mutation also stops the value writes of unregistered assets of the same key

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. Sibling of `replaced-asset-write-gate`, whose per-asset value-write
gate this design reuses; kept separate under overlap exclusion E1 (that design is `ready` and
eligible for automatic fixing, this one adds a trait method and a manager map).

## Phase 1: High-Level Design

### Purpose

A keyed asset the manager does not register — a volatile key (`get_volatile_resource_asset`) or a
recipe whose cache strategy keeps no result (`get_uncached_resource_asset`, and
`ImmediateAssetManager`'s equivalent) — still writes its value to the store
(`persist_with_status_tracking` → `save_to_store`, spawned on the queued manager). `remove`,
`set_state` and `set_binary` find the key's asset only through the key map (`lookup_key_asset`),
so they cannot stop such a write, and a write in progress lands after the mutation. The store entry
of a key must belong to whoever mutated the key last, registered writer or not.

### Problem Example

Verified by reading HEAD (`70b9005`): both constructors build the asset with `Some(key)` and never
insert it into `assets`; `save_to_store` writes any keyed asset with `stored() == true` (it only
skips the ownership warning for a volatile or uncached one); `remove` (trait default) consults
`lookup_key_asset` alone.

`data/recipes.yaml` declares `u.txt` = `make_text/u.txt` with `cached: none` (a strategy that keeps
no result). Queued manager.

1. `get(data/u.txt)` creates an unregistered keyed asset; it finishes `Ready` and its spawned
   `save_to_store` is about to call `store.set(data/u.txt, …, Ready)`;
2. `remove(data/u.txt)` sees no live asset, rewrites the stored entry as `Recipe`, returns `Ok`;
3. the save resumes and writes `Ready`.

Today: the store holds `Ready`; a fresh process fast-tracks the removed value. With
`set_state(data/u.txt, "user")` in step 2 the user's stored value is overwritten by the computed
one. Should be: `Recipe` after `remove`, the user's value after `set_state` / `set_binary`, whatever
the timing. For a volatile key the late entry is `Volatile`, which the fast track refuses, so the
effect is a stale stored copy rather than a wrong read.

### Scope and Acceptance Criteria

- **AC-1** Remove wins over an unregistered writer's in-progress write
  WHEN an unregistered keyed asset's value write is in progress and `remove` of its key runs
  THEN `remove` returns after that write, and the stored entry is `Recipe`
- **AC-2** `set_state` / `set_binary` win
  WHEN the same happens with `set_state` or `set_binary`
  THEN the store holds the supplied value after both complete
- **AC-3** A write that starts after the mutation is skipped
  WHEN an unregistered keyed asset is still evaluating when its key is mutated, and finishes later
  THEN its value is returned to its caller, nothing is written, and its `persistence_status()` is
  `None`
- **AC-4** Unmutated unregistered assets still persist
  WHEN nothing mutates the key
  THEN an unregistered keyed asset with `stored: true` writes its value as today
- **AC-5** The volatile path is covered
  WHEN the key's recipe is volatile instead of uncached
  THEN AC-1 and AC-3 hold for it too
- **AC-6** Both in-tree managers
  WHEN the scenarios run on `ImmediateAssetManager`
  THEN the results are the same

Non-goals: a write by another process (`store-conditional-write`); a key whose first value is being
written while `removedir` runs (it removes only keys the store already lists); assets of an
external `AssetManager` implementation that does not override the new trait method (they keep
today's behaviour).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a field (a per-key map) to both managers (rule 3) and a
  `pub` default method to the `AssetManager` trait (rule 4); readiness is `needs-decision`
  (rule 5).
- **Leading issue:** **Proposed resolution - stop the write, not the run:** recommended.
- **Explanation:** once `replaced-asset-write-gate` exists, an asset's value write is guarded by its
  `MetadataSaver` gate and `close` waits for a write in progress; the only missing piece is that a
  mutation can reach unregistered assets. A per-key list of weak references, filled when such an
  asset is created, gives it that reach.
- **Open questions:**
  1. **Proposed resolution - stop the write, not the run.** A mutation closes an unregistered
     writer's saver (no further store write) but does not cancel its evaluation. Recommended: the
     caller of `get` on an uncached or volatile key gets a fresh value by contract, and it is still
     correct for that caller; only its persistence is obsolete. Alternative: `cancel_for_replacement`
     (as for a registered in-flight asset), which turns a concurrent `get` into a `Cancelled` error
     for a caller that never shared the asset.
  2. **Proposed resolution - trait surface.** A new `AssetManager` method
     `unregistered_key_writers(&self, key) -> Vec<AssetRef<E>>` with a default returning an empty
     list, so external managers compile unchanged. Alternative: keep the list private to the two
     managers and duplicate the mutation code that consults it (the `remove` default is shared, so
     both managers would have to override `remove`). Recommended: the default method.

### Design Dependencies

- `replaced-asset-write-gate` — **requires**: provides `MetadataSaver::begin_value_write`, the
  `value_write` gate and a `close` that waits for a write in progress. Lands first.
- `store-conditional-write` — **overlaps** (weak): the cross-process answer; not needed in one
  process.
- `asset-registration-contract` — **overlaps** (weak): describes which keyed assets are registered;
  this design adds that unregistered ones are tracked for mutations only, never reused.

### Consolidated Findings

- Registration sites: `DefaultAssetManager::get_volatile_resource_asset`,
  `DefaultAssetManager::get_uncached_resource_asset`, and `ImmediateAssetManager`'s volatile and
  "keeps no result" branches of its resource-asset lookup. Query (non-keyed) assets never write and
  are not tracked.
- The list holds `WeakAssetRef`s (`AssetRef::downgrade`), pruned of dead entries on every insert and
  lookup, so it never keeps an asset alive and stays bounded by live assets.
- Lock order: the mutation already holds `key_mutation_lock`; closing a writer's saver takes the
  writer's gate, whose holder only awaits `store.set` (as in `replaced-asset-write-gate`), so no
  cycle. The writer's own `save_to_store` never takes `key_mutation_lock` for an unregistered asset
  (the ownership check is skipped for volatile and uncached assets).
- Ordering: the mutation closes the writers *before* its own store work, so a writer either
  finished before the mutation's write or skips.

## Phase 2: Architecture

### Solution

1. `DefaultAssetManager` and `ImmediateAssetManager` gain
   `unregistered_writers: scc::HashMap<Key, Vec<WeakAssetRef<E>>>` (initialised empty in their
   constructors) and a private `track_unregistered_writer(&self, key, &AssetRef<E>)` that prunes dead
   entries for the key and pushes `asset.downgrade()`. Called at the registration sites above,
   right after the asset is built.
2. `AssetManager` (`liquers-core/src/assets.rs`) gains

   ```rust
   /// Keyed assets of `key` that this manager created but did not register (volatile, or a cache
   /// strategy that keeps no result) and that are still alive. A keyed mutation closes their
   /// store writes. The default tracks none.
   fn unregistered_key_writers(&self, key: &Key) -> Vec<AssetRef<E>> { Vec::new() }
   ```

   Both managers override it: upgrade the live weak references, prune dead ones.
3. `AssetRef` gains `pub(crate) async fn close_store_writes(&self)`: clones the metadata saver and
   awaits `close()` (which, after `replaced-asset-write-gate`, waits for a value write in progress
   and makes later ones skip). It never changes status.
4. `remove` (trait default), both managers' `set_state` and `set_binary`: right after taking
   `key_mutation_lock`, `for writer in self.unregistered_key_writers(key) { writer.close_store_writes().await; }`.

Rejected: a per-key generation counter compared at write time (needs the write to hold a per-key
lock across compare and `store.set`, i.e. `key_mutation_lock` in every unregistered save);
registering unregistered assets in the key map (changes caching semantics); cancelling the run
(question 1).

### Changes

- `liquers-core/src/assets.rs`: the two manager structs and constructors; the registration sites;
  `AssetManager::unregistered_key_writers` (default + two overrides); `AssetRef::close_store_writes`;
  `remove`, `set_state` ×2, `set_binary` ×2. Async; `scc` already used for the key map.
- Errors: none new. A skipped write is `Ok(SaveOutcome::Skipped)` → `PersistenceStatus::None`.
- `specs/reference/ASSET_SET_OPERATION.md` and `ASSETS.md` §Remove Semantics: a mutation also stops
  the store writes of unregistered assets of the key (their values are still returned). History
  rows and `reviewed:`.
- No command, registry, serialized format or binding change. `liquers-py` and `liquers-web` do not
  implement `AssetManager` (checked: `grep -rn "impl.*AssetManager<" liquers-py liquers-web` finds none).

### Risks

| Risk | Handling | Certainty |
|---|---|---|
| Likely files | `liquers-core/src/assets.rs`; two reference documents; one test file | high |
| Map growth | weak refs, pruned on insert and lookup; an entry dies with its asset | high |
| Deadlock | as in `replaced-asset-write-gate`; the unregistered save takes no manager lock (Consolidated Findings) | high |
| Mutation latency | waits for at most one store write per live unregistered writer of the key | high |
| Existing tests | none asserts that a late unregistered write survives a mutation; `cache_strategy.rs` and volatile tests exercise persistence without mutation (AC-4) | medium |
| Ordering against the sibling | requires its gate; implemented before it, `close` would not wait and AC-1 would be racy | high |
| Recovery | drop step 4's loop; the map alone changes nothing | high |

## Phase 3: Examples and Tests

### Examples

The Problem Example, made deterministic with `HoldingStore` from `replaced-asset-write-gate`'s
Phase 3 (an `AsyncMemoryStore` wrapper whose `set` for one key with a computed status notifies
`entered` and awaits `release`). Recipes through `env_with_at` (`data/recipes.yaml`, `u.txt` =
`make_text/u.txt`), with the recipe's `cached` set to `CacheStrategy::None`, or `volatile: true`
for AC-5. Query validated: `liquers-validate --command make_text -- 'make_text/u.txt'` `Ok`.

### Tests

In `liquers-core/tests/asset_manager_remove_expire_describe.rs`
(`#[tokio::test(flavor = "multi_thread", worker_threads = 2)]`, every wait under `within`), each
run for both managers where the file's helpers allow (AC-6):

- `amr_unregistered_late_save_does_not_revive_a_removed_key` — `get(data/u.txt)` to `Ready`, await
  `entered`, spawn `remove`, yield, `release`, await the remove; stored status `Recipe`. Fails at
  HEAD. Covers AC-1.
- `amr_unregistered_late_save_does_not_overwrite_set_state` and `…_set_binary` — stored value is the
  supplied one. Covers AC-2.
- `amr_unregistered_write_after_mutation_is_skipped` — a `make_text` that waits on a `Notify`
  before returning; `get` in a task; `remove` while it waits; release; the `get` returns the value,
  the store holds `Recipe`, `persistence_status()` is `None`. Covers AC-3.
- `amr_unregistered_volatile_late_save_does_not_revive` — AC-1 with a volatile recipe. Covers AC-5.
- Existing `cache_strategy.rs` and volatile persistence tests pass unchanged. Covers AC-4.
- `ImmediateAssetManager` variants of the first two (inline save holds the gate inside `get`'s
  future; the test drives `get` and `remove` concurrently with `tokio::join!`). Covers AC-6.

Unit, `liquers-core/src/assets.rs` `tests`: `unregistered_writers_are_weak_and_pruned` — track an
asset, drop it, `unregistered_key_writers(key)` is empty and the map entry is gone.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test asset_manager_remove_expire_describe amr_unregistered`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Precondition: `replaced-asset-write-gate` implemented (`begin_value_write`, waiting
  `close`) — `cargo test -p liquers-core --lib metadata_saver_close`
- [ ] 2. Tests of Phase 3 in `asset_manager_remove_expire_describe.rs` (reuse `HoldingStore`); they
  fail — `cargo test -p liquers-core --test asset_manager_remove_expire_describe amr_unregistered`
- [ ] 3. `assets.rs`: `unregistered_writers` field in both managers, `track_unregistered_writer`,
  calls at the registration sites, `AssetManager::unregistered_key_writers` default and overrides;
  unit test `unregistered_writers_are_weak_and_pruned` — `cargo test -p liquers-core --lib unregistered_writers`
- [ ] 4. `assets.rs`: `AssetRef::close_store_writes`; the loop in `remove`, `set_state` ×2,
  `set_binary` ×2 — step-2 tests pass; then `cargo test -p liquers-core --test cache_strategy --test expiration_integration --test evaluation_session`.
  Rollback: remove the loops.
- [ ] 5. `ASSET_SET_OPERATION.md`, `ASSETS.md` (History rows, `reviewed:`); issue `status: closed`
  with a resolution note — `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-lib --lib --tests`;
`bash scripts/check-build-matrix.sh` wasm32 rows (`scc` and `tokio::sync` on wasm, as the key map
already is). Diff review: no `println!`, no `unwrap` in library code, explicit match arms.
