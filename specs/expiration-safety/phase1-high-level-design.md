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
  (`assets.rs:619-659` as of the post-`async-wasm-refactor` merge) when called directly on a held
  ref, bypassing the manager-level guard.
  Needs to become a normal-path cache-miss, mirroring the `Error`/`Cancelled` treatment from WP-2.
- New recovery-only pair: `AssetRef::poll_state_any_status()` / `get_any_status()` and
  `AssetManager::get_any_status(key)`, for **keyed** assets only — reads the current value
  regardless of status (including `Expired`), without triggering evaluation. (Renamed from an
  initial `_also_expired` draft — see "Naming" below.)
- New user-facing flow: promote a keyed asset's current value (obtained via `get_any_status`) to
  `Status::Override` for the same key via `AssetManager::to_override(key)` — reusing the existing
  `AssetRef::to_override()` transition (`assets.rs:1932` onward, already handles `Ready`/`Expired`/
  etc. `-> Override` in-memory) plus persistence back to the store under that key. **Not**
  Expired-specific: pinning a still-`Ready` value works the same way.
- `TimedAsset<E>` in the expiration monitor heap (`assets.rs:2969`) holds a strong `AssetRef<E>`;
  switch to `WeakAssetRef<E>` so the monitor holds no strong references (`upgrade() == None` skips
  silently), per WP-3 item 1.
- Non-keyed expired assets: confirm/document they have no `get_any_status` path and are evicted
  the same way as today (already the case; add regression coverage only).

### Naming
The read/promote pair was initially drafted as `get_also_expired`/`override_expired`; renamed
during Phase 2 review per user feedback: `override_expired` was Expired-specific in name but not
in behavior (it must also pin a `Ready` value), so it was renamed to `to_override` to reuse the
verb already established by `AssetRef::to_override()`. The paired read method was renamed to
`get_any_status`/`poll_state_any_status`, following this file's existing `_unchecked` convention
(`State::data_unchecked()`) for "bypasses a normal guard" without the unsafe-adjacent connotation
`_unchecked` carries in Rust. See `phase2-architecture.md`'s Overview for the full rationale.

### Store System
No `AsyncStore` trait changes. `get_any_status` reads existing store bytes/metadata directly
(bypassing the `try_fast_track` status allow-list) rather than through fast-track.

## Crate Placement

`liquers-core` only (`src/assets.rs`, tests in `tests/expiration_integration.rs`). No changes to
`liquers-lib`, `liquers-store`, `liquers-axum`, or `liquers-py` public surfaces are required for
the recovery API itself; a caller audit will confirm.

## Open Questions

None remaining — both original open questions were resolved during Phase 1 review (see "Resolved
Questions" below).

## Resolved Questions

1. **Trait placement (resolved, updated after `async-wasm-refactor` landed on `main`):**
   `get_any_status()`/`to_override()` are `AssetManager<E>` trait methods (not
   `DefaultAssetManager`-only). `async-wasm-refactor` has since **landed** (it was "in progress"
   when this was first written — see the "async-wasm-refactor sync" note below) and added a real
   second implementor, `ImmediateAssetManager<E>`, plus new shared primitives
   (`lookup_key_asset`/`get_envref`/`insert_key_asset`) that the refactor itself uses to hoist most
   manager logic into **shared default trait methods**. Revised conclusion:
   `get_any_status`/`to_override` are written as **one shared default method each**, using those
   same primitives — not required-and-duplicated per manager as originally planned when no such
   primitive existed. See `phase2-architecture.md` for the full rationale and the no-double-
   serialization algorithm (unchanged).
2. **Override persistence (resolved):** promotion to `Override` must yield a consistent state but
   avoid double-serialization, reusing the existing `PersistenceStatus` already tracked per asset
   (`assets.rs:134-143`, set via `record_persistence_result`/`persist_with_status_tracking`):
   - `Persisted` — data bytes are already correct in the store; only the metadata's `status` field
     needs rewriting to `Override`, no re-serialization of the value.
   - `NonSerializable` — nothing is written to the store (matches today's silent skip); only the
     in-memory `AssetRef` transitions via the existing `to_override()`.
   - `NotPersisted` / `None` — treated as a retry opportunity: re-run the normal persist path
     (serialize + store) with the now-`Override` status.

## async-wasm-refactor sync note (added after Phase 4 approval, before execution)

`async-wasm-refactor` merged into `main` while this branch was open (79 commits,
`liquers-core/src/assets.rs` +1153/-279 lines). Re-audited the current code after merging main in:
- `ImmediateAssetManager<E>` (wasm/browser manager) now exists and is the second `AssetManager<E>`
  implementor anticipated above. It has **no monitor/timer at all** — expiration is checked lazily
  on access (`AssetRef::is_expired()` inside `get`/`get_asset`), so WP-3's weak-ref monitor fix
  (item 1) stays scoped entirely to `DefaultAssetManager`'s monitor (now
  `#[cfg(not(target_arch = "wasm32"))]`-gated) — unaffected.
- `poll_state()`'s `Status::Expired` bug is unchanged in substance (now at `assets.rs:619`, was
  `:596` — pure line-number drift from unrelated code inserted above it).
- The new `lookup_key_asset`/`get_envref`/`insert_key_asset` primitives change the trait-method
  strategy for `get_any_status`/`to_override` from "required, duplicated per manager" to "one
  shared default method" — see the revised Resolved Question 1 above and `phase2-architecture.md`.
- All other line-number citations across Phase 2-4 were refreshed against the post-merge file.

## References

- `specs/FEATURES/EXPIRATION-SAFETY.md`, `specs/FEATURES/EXPIRATION-SAFETY-IMPLEMENTATION-PLAN.md`
- `specs/expiration-mechanism/`, `specs/expiration-monitor-assetref/` (prior related designs)
- `specs/async-wasm-refactor/` (landed on `main` during this design's Phase 4 — see sync note above)
- `plan20260707.md` WP-3
