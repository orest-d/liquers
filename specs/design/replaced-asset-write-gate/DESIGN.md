---
id: REPLACED-ASSET-WRITE-GATE
kind: design
title: A removed or replaced asset's late value write never reaches the store
form: compact
status: complete
gh_pr: [208]
readiness: ready
autofix: eligible
area: [core/assets, core/store]
issues: [REMOVED-OR-REPLACED-KEY-REVIVED-BY-A-LATE-BACKGROUND-SAVE]
created: 2026-10-26
---
# A removed or replaced asset's late value write never reaches the store

> **Complete on 2026-10-27**: implemented by automatic fix in #208.

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written; Phase 4
implemented on 2026-10-27 (automatic fix).

## Phase 1: High-Level Design

### Purpose

A keyed asset that has finished `Ready` may still be writing its value to the store (on the queued
manager in a spawned task; on the inline manager inside the evaluation future, after the status
flipped). `remove`, `set_state` and `set_binary` treat a finished asset as having nothing left to
do, so the late write lands after the mutation and puts the old value back. The store entry of a
key must belong to whoever mutated the key last.

### Problem Example

Established by reading HEAD (`cd6d5da`; neither PR #176 nor #179 touched this path), and matching
the observation recorded in the issue (16 of 20 runs of `evaluation_session.rs` at `2bc4228`).

`data/recipes.yaml` declares `c.txt` = `make_text/c.txt` (validated:
`liquers-validate --command make_text -- 'make_text/c.txt'`: `Ok`). Queued manager.

1. `get(data/c.txt)`: the run finalizes `Ready` and `persist_with_status_tracking(true)` spawns
   `save_to_store`, which serializes and is about to call `store.set(data/c.txt, …, Ready)`;
2. `remove(data/c.txt)` takes `key_mutation_lock`; `cancel_for_replacement` returns at once because
   the asset is not in flight; the asset is unmapped and the stored entry (if any) is rewritten as
   `Recipe`; the call returns `Ok`;
3. the spawned save resumes: it checks only `Status::Cancelled`, so it writes `Ready`.

Today: the store holds `Ready` again and the next `get` fast-tracks the removed value. With
`set_state(data/c.txt, "user")` in step 2, the store ends with the old computed value while memory
holds `"user"` (lost after a restart). Should be: the store holds `Recipe` (after `remove`) or the
user's value (after `set_state` / `set_binary`), whatever the timing.

### Scope and Acceptance Criteria

- **AC-1** Remove wins over an in-progress write
  WHEN a finished keyed asset's value write is in progress and `remove` of its key runs
  THEN `remove` returns after that write, the stored entry is `Recipe`, and the next `get` runs the recipe again
- **AC-2** A write that has not started after a replacement is skipped
  WHEN `cancel_for_replacement` has run on a finished asset and its value write starts afterwards
  THEN nothing is written, and its `persistence_status()` is `None`
- **AC-3** `set_state` wins over a late write
  WHEN a finished keyed asset's value write is in progress and `set_state` replaces the key
  THEN the store holds the supplied value with its written status after both complete
- **AC-4** `set_binary` wins over a late write
  WHEN the same happens with `set_binary`
  THEN the store holds the supplied bytes and metadata
- **AC-5** An asset that is not replaced still persists
  WHEN nothing mutates the key, or the asset is only evicted from the map (expiry, stale-terminal eviction)
  THEN its value is written and `persistence_status()` becomes `Persisted`, as today
- **AC-6** `to_override` keeps writing
  WHEN `to_override` pins a finished asset whose background save is still running
  THEN the stored entry is the asset's value with status `Override`, as the existing `to_override` tests require

Non-goals: an unregistered keyed asset (`cached: false`, volatile), which no mutation can reach
because it is not in the map — filed as `LATE-SAVE-OF-AN-UNREGISTERED-KEYED-ASSET-SURVIVES-REMOVAL`;
a write by another process (`store-conditional-write`); the metadata saver's own snapshots, which
`close` already stops.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — a bug fix in `liquers-core/src/assets.rs` restoring what
  `ASSET_SET_OPERATION.md` promises for a replaced asset ("nothing more of it is written to the
  store"): one private field on the existing `MetadataSaver`, one crate-private method, two changed
  bodies. No `pub` signature, type, command or format change (rules 1-6). Priority P1, so it is
  listed for the user to authorize rather than started.
- **Leading issue:** None
- **Explanation:** the replaced asset already has a per-asset "nothing more is written" switch, its
  `MetadataSaver::close`; it is only applied to in-flight assets, and the value write does not
  consult it. Making the value write check it under a per-asset gate that `close` waits on closes
  the race without a manager-wide lock.
- **Open questions:** None. Settled in Phase 2: the gate is per asset (a manager-wide lock would
  serialize every save with every mutation); `remove` / `set_*` wait for at most one store write of
  the replaced asset.

### Design Dependencies

- `asset-cancellation-outcome` (complete, E4) — origin of `cancel_for_replacement` and
  `MetadataSaver::close`; not extended.
- `uncached-write-back-rechecks-dependency-versions` — **overlaps** (finished Phase 4, not edited):
  adds a call after `save_to_store` in `persist_with_status_tracking`; this design edits inside
  `save_to_store`. Either order; a textual rebase only.
- `asset-registration-contract` — **overlaps**: rewords the ownership warning in `save_to_store`.
  Textual rebase only.
- `inline-metadata-save-without-runtime` — **overlaps**: edits `MetadataSaver::save_immediately`;
  this design edits `close` and adds a method. No order.
- `store-conditional-write` — **overlaps**: a conditional write is the cross-process answer; not
  needed in one process.
- `unregistered-keyed-write-gate` (design for
  `LATE-SAVE-OF-AN-UNREGISTERED-KEYED-ASSET-SURVIVES-REMOVAL`, filed with this design) —
  **required-by**: the unregistered case, left out (see Non-goals); it reuses this design's gate
  and lands after it. Kept separate under overlap exclusion E1.

### Consolidated Findings

- Ordering argument: `close` sets `closed` before taking the value gate. A writer holding the gate
  either read `closed == false` and finishes before `close` returns (the mutation then writes after
  it), or reads `true` and skips. No interleaving lets the old value land after the mutation.
- Lock order: the gate is taken while `key_mutation_lock` is held (by `close`, inside the
  mutations, and by `to_override`'s synchronous persist), and the holder of the gate only awaits
  `store.set`, which takes neither lock. The ownership check in `save_to_store`, which can take
  `key_mutation_lock` through `owned_key_asset`, stays before the gate.
- `to_override` must not close the saver (the asset stays the key's), so the gate does not affect
  it; its synchronous persist simply waits for a background write of the same asset.
- The inline manager has the same window (between finalization and `store.set`); the fix is in
  `AssetRef`, so both managers and any external manager that calls `cancel_for_replacement` get it.

## Phase 2: Architecture

### Solution

1. `MetadataSaver` (`liquers-core/src/assets.rs`, line ~468) gains a private field
   `value_write: Mutex<()>` (tokio, as `state`), initialised in `MetadataSaver::new`, and

   ```rust
   /// Supersede pending metadata, then take the value-write gate; `None` when closed.
   pub(crate) async fn begin_value_write(&self) -> Option<tokio::sync::MutexGuard<'_, ()>>;
   ```

   It calls `self.supersede().await`, then `self.value_write.lock().await`, then returns `None` if
   `closed` is set (checked after the lock is held), else `Some(guard)`.
2. `MetadataSaver::close`: after its current body, `drop(self.value_write.lock().await);` so it
   returns only once a value write already holding the gate has finished. Doc comment says so.
3. `AssetRef::save_to_store` (line ~4597): replace `saver.supersede().await;` with
   `let Some(_gate) = saver.begin_value_write().await else { return Ok(SaveOutcome::Skipped); };`
   and keep `_gate` alive across the metadata re-read and `store.set`; drop it before
   `refresh_listing_version`. The ownership-warning block stays before the gate. An `eprintln!`
   like the `Cancelled` skip reports the skip.
4. `AssetRef::cancel_for_replacement` (line ~4928): close the saver for every asset, then discard
   the run only when it is in flight:

   ```rust
   let saver = self.data.read().await.metadata_saver.clone();
   saver.close().await;
   if is_in_flight(self.status().await) { self.discard_run("replaced").await?; }
   Ok(())
   ```

   Doc comment: "A finished asset keeps its status, but nothing more of it is written: a value
   write in progress is waited for, a later one is skipped."

Rejected alternatives:

- **Take `key_mutation_lock` in `save_to_store`:** serializes every save with every keyed mutation
  of the manager, and deadlocks `to_override`, which persists synchronously while holding it.
- **`remove` / `set_*` wait for `persistence_status()` to leave `None`:** a status, not a
  synchronisation point; a save that has not started yet would still land later.
- **Re-check ownership (`owned_key_asset`) before writing:** a replaced asset's successor may not be
  mapped (`set_binary` maps nothing; `remove` unmaps), and an evicted asset is legitimately
  unmapped, so ownership cannot tell "replaced" from "evicted" (AC-5).
- **Conditional store write:** needs `store-conditional-write`, which is undecided; overkill in one
  process.

### Changes

- `liquers-core/src/assets.rs`: `MetadataSaver` (field, `new`, `close`, `begin_value_write`),
  `AssetRef::save_to_store`, `AssetRef::cancel_for_replacement`. Async, `tokio::sync::Mutex`
  (available on wasm32 with the `sync` feature, as `state` already is).
- Errors: none new; a skip is `Ok(SaveOutcome::Skipped)` → `PersistenceStatus::None`, as for a
  cancelled asset.
- `specs/reference/ASSET_SET_OPERATION.md` §In-Flight Asset Handling / §Discarding a late result: a
  finished replaced asset's in-progress write is waited for and a later one skipped.
  `specs/reference/ASSETS.md` §Remove Semantics (the `remove` steps) and the "A background save is
  not part of the run" sentence: add that a mutation of the key waits for it. History rows and
  `reviewed:`.
- No `pub` signature, command, registry, serialized format or binding change.

### Risks

| Risk | Handling | Certainty |
|---|---|---|
| Deadlock: `close` waits for a write that waits for a lock the mutation holds | the gate holder awaits only `store.set`; ownership check before the gate (Consolidated Findings) | high |
| `remove` / `set_*` latency grows by one store write of the old asset | bounded, and only when a write is in progress | high |
| A slow or hung store write now also hangs the mutation | same exposure as the mutation's own store write | medium |
| `to_override` regresses | it never closes the saver; AC-6 tests | high |
| Existing tests asserting `Persisted` after a replace | none found; `persistence_status` of a replaced asset is `None` now | medium |

Recovery: revert step 4 of Phase 4 (closing finished assets) to restore today's behaviour; the gate
alone changes nothing while only in-flight assets are closed.

## Phase 3: Examples and Tests

### Examples

The Problem Example, made deterministic with a store wrapper. `HoldingStore` delegates to an
`AsyncMemoryStore`; its `set` for `data/c.txt` with a stored status `Ready` notifies `entered` and
awaits `release` before delegating (modelled on `GatedStore` in
`liquers-core/tests/external_change_integration.rs`). Recipes: `data/recipes.yaml` with `c.txt` =
`make_text/c.txt` (the `env_with_at` helper of `asset_manager_remove_expire_describe.rs`, extended to
take the wrapped store). `make_text` counts its calls.

### Tests

In `liquers-core/tests/asset_manager_remove_expire_describe.rs` (queued manager,
`#[tokio::test(flavor = "multi_thread", worker_threads = 2)]`, every wait under `within`):

- `amr_late_save_does_not_revive_a_removed_key` — `get(data/c.txt)` and wait for `Ready`; await
  `entered`; spawn `remove(data/c.txt)`; yield, then `release`; await the remove. Stored status is
  `Recipe`; a second `get` runs `make_text` again (count 2). At HEAD it fails (stored `Ready`, count
  1). Covers AC-1.
- `amr_late_save_does_not_overwrite_set_state` — as above with `set_state(data/c.txt, "user")`:
  stored bytes `user`, status `Override`. Covers AC-3.
- `amr_late_save_does_not_overwrite_set_binary` — the same with `set_binary`. Covers AC-4.

Unit tests in `liquers-core/src/assets.rs` `tests`:

- `save_to_store_skips_after_replacement_of_a_finished_asset` — a `Ready` keyed asset (as in
  `save_to_store_reports_written_on_success`), `cancel_for_replacement()`, then
  `save_to_store()` is `Ok(SaveOutcome::Skipped)`, the store has no entry, status stays `Ready`.
  Covers AC-2.
- `metadata_saver_close_waits_for_a_value_write` — take `begin_value_write()`; spawn `close()`;
  assert it has not completed after a yield; drop the guard; `close` completes; a further
  `begin_value_write()` is `None`. Covers AC-1, AC-2 at unit level.
- Existing `save_to_store_reports_written_on_success`, `expiration_integration.rs` persistence
  tests (`old.persistence_status() == Persisted`) and the `to_override_*` tests pass unchanged.
  Covers AC-5, AC-6.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test asset_manager_remove_expire_describe`,
`--lib assets::tests`, `--test expiration_integration`, `--test evaluation_session`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-core/tests/asset_manager_remove_expire_describe.rs` — `HoldingStore` and the
  three `amr_late_save_*` tests; AC-1, AC-3, AC-4 fail at HEAD — `cargo test -p liquers-core --test asset_manager_remove_expire_describe amr_late_save`
- [x] 2. `assets.rs` `MetadataSaver` — `value_write` field, `begin_value_write`, `close` waits on
  the gate; unit test `metadata_saver_close_waits_for_a_value_write` — `cargo test -p liquers-core --lib metadata_saver_close`
- [x] 3. `assets.rs` `AssetRef::save_to_store` — use `begin_value_write`, hold the guard across
  `store.set` only — `cargo test -p liquers-core --lib assets::tests::save_to_store`
- [x] 4. `assets.rs` `AssetRef::cancel_for_replacement` — close for every asset, discard only in
  flight; unit test `save_to_store_skips_after_replacement_of_a_finished_asset`; step-1 tests pass —
  `cargo test -p liquers-core --test asset_manager_remove_expire_describe --test expiration_integration --test evaluation_session`.
  Rollback: revert this step.
- [x] 5. `ASSET_SET_OPERATION.md`, `ASSETS.md` (History rows, `reviewed:`); issue
  `status: closed` with a resolution note — `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core --lib --tests`, then `cargo test -p liquers-lib --lib --tests` (it
replaces keys through `ns-*` commands); `bash scripts/check-build-matrix.sh` wasm32 rows (the gate
uses `tokio::sync` on wasm). Diff review: no `println!`, no `unwrap` in library code.

### Implementation notes (2026-10-27)

- `remove` read the stored entry before `cancel_for_replacement`, so with nothing stored yet it would have left the late `Ready` copy in place. It now re-reads the store after the cancel; `amr_late_save_does_not_revive_a_removed_key` fails without that re-read.
- The unit test for AC-2 also checks `persistence_status()` is `None` after `persist_with_status_tracking`.
