# Phase 1: High-Level Design - expiration-safety

## Feature Name

expiration-safety (WP-3 of `plan20260707.md`, closing `specs/FEATURES/EXPIRATION-SAFETY.md`)

## Purpose

Guarantee that expired assets are never silently reused as fresh data by normal evaluation or
dependency resolution, while still letting a user deliberately recover an expensive **keyed**
expired result (inspect it, or promote it to `Override` without recomputation). Code audit shows
most of the monitor-race and stale-read work is already implemented (earliest-deadline-wins
tracking, `manager.get`/`get_asset`/`get_dependency_asset` treat `Expired` as cache-miss,
`wait_for_dependency` already allows the tolerated in-flight race, fast-track already rejects
persisted `Status::Expired`). Three gaps remain, and this WP closes them.

## Core Interactions

### Asset System
- `AssetRef::poll_state()`/`get()` currently still return the stale value for `Status::Expired`
  (`assets.rs:596-636`) when called directly on a held ref, bypassing the manager-level guard.
  Needs to become a normal-path cache-miss, mirroring the `Error`/`Cancelled` treatment from WP-2.
- New recovery-only pair: `AssetRef::poll_state_also_expired()` / `get_also_expired()` and
  `AssetManager::get_also_expired(key)`, for **keyed** assets only — reads expired in-memory or
  persisted state without triggering evaluation.
- New user-facing flow: promote an expired keyed state (obtained via `get_also_expired`) to
  `Status::Override` for the same key — reusing the existing `AssetRef::to_override()` transition
  (`assets.rs:1868-1878` already handles `Expired -> Override` in-memory) plus persistence back to
  the store under that key.
- `TimedAsset<E>` in the expiration monitor heap (`assets.rs:2399`) holds a strong `AssetRef<E>`;
  switch to `WeakAssetRef<E>` so the monitor holds no strong references (`upgrade() == None` skips
  silently), per WP-3 item 1.
- Non-keyed expired assets: confirm/document they have no `get_also_expired` path and are evicted
  the same way as today (already the case; add regression coverage only).

### Store System
No `AsyncStore` trait changes. `get_also_expired` reads existing store bytes/metadata directly
(bypassing the `try_fast_track` status allow-list) rather than through fast-track.

## Crate Placement

`liquers-core` only (`src/assets.rs`, tests in `tests/expiration_integration.rs`). No changes to
`liquers-lib`, `liquers-store`, `liquers-axum`, or `liquers-py` public surfaces are required for
the recovery API itself; a caller audit will confirm.

## Resolved Questions

1. **Trait placement (resolved):** `get_also_expired()` is an `AssetManager<E>` trait method (not
   `DefaultAssetManager`-only). `specs/async-wasm-refactor` is adding a second manager
   implementation, so any new manager-facing capability must be a trait method with a sensible
   default (mirroring `get_dependency_asset`/`wait_for_dependency` defaults already on the trait)
   so both implementations stay compatible without duplicating logic.
2. **Override persistence (resolved):** promotion to `Override` must yield a consistent state but
   avoid double-serialization, reusing the existing `PersistenceStatus` already tracked per asset
   (`assets.rs:134-143`, set via `record_persistence_result`/`persist_with_status_tracking`):
   - `Persisted` — data bytes are already correct in the store; only the metadata's `status` field
     needs rewriting to `Override`, no re-serialization of the value.
   - `NonSerializable` — nothing is written to the store (matches today's silent skip); only the
     in-memory `AssetRef` transitions via the existing `to_override()`.
   - `NotPersisted` / `None` — treated as a retry opportunity: re-run the normal persist path
     (serialize + store) with the now-`Override` status.

## References

- `specs/FEATURES/EXPIRATION-SAFETY.md`, `specs/FEATURES/EXPIRATION-SAFETY-IMPLEMENTATION-PLAN.md`
- `specs/expiration-mechanism/`, `specs/expiration-monitor-assetref/` (prior related designs)
- `specs/async-wasm-refactor/` (in-progress second `AssetManager` implementation — trait-method
  constraint above)
- `plan20260707.md` WP-3
