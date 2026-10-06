# Phase 1: High-Level Design - Skipped Store Writes Are Not Persists

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** The target status is already defined and used by the caller for the same
  situation (`persist_with_status_tracking` records `PersistenceStatus::None` for a cancelled or
  `stored: false` asset before calling `save_to_store`); the fix carries the skip out of
  `save_to_store` as a distinct outcome so the caller can record the same status.
- **Open questions:** None

## Problem and Evidence

`AssetRef::save_to_store` (`liquers-core/src/assets.rs` ≈3261, private) returns `Ok(())` without
writing in three places:

1. `is_cancelled()` at entry;
2. `lock.is_cancelled()` after serialization;
3. `!metadata.stored()` on the metadata produced alongside the binary.

`persist_with_status_tracking` (≈2555) passes the result to `record_persistence_result` (≈2535),
which maps every `Ok(())` to `PersistenceStatus::Persisted`. A skipped write is therefore recorded
as durable. `AssetManager::to_override` (trait default ≈6103, `DefaultAssetManager` ≈6794,
`ImmediateAssetManager` ≈8341) branches on `Persisted` and writes metadata straight to the store
with `set_metadata` — leaving a metadata-only entry for a key whose value was never written.
The `stored: false` trigger was fixed on 2026-09-27 *in the caller*; the three checks inside
`save_to_store` remain.

## Expected Behaviour and Acceptance Criteria

1. When `save_to_store` skips the write for any of the three reasons, the asset's
   `persistence_status()` is `PersistenceStatus::None` and `last_persistence_error` is unset.
2. A completed `store.set` still records `Persisted`; a failed one still records the classified
   error status with the error.
3. In the cancellation case, `to_override` subsequently takes the non-`Persisted` branch (goes
   through `persist_with_status_tracking`), so no metadata-only entry is written.
4. No log line, status or value of the asset changes except `persistence_status`.

## Affected Systems

`AssetRef` persistence bookkeeping and `to_override` in both managers. No API, store, query,
or serialization change.

## Scope and Non-Goals

In scope: the outcome type of the private `save_to_store` and its mapping. Non-goals: changing
when cancellation is checked, making cancellation an error, adding a new `PersistenceStatus`
variant, or the background-spawn race (a spawned save that started before cancellation) beyond
what the existing checks already cover.

## Compatibility

`PersistenceStatus` is public; its variants are unchanged. Observable change: a status that used
to read `Persisted` for a skipped write now reads `None`. `None`'s doc ("No persistence attempt
has been made yet") is the value the caller already uses for skips; its doc comment is widened to
say "or the most recent attempt was skipped (cancelled, `stored: false`)".

## Documentation Assessment

`reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` and `reference/ASSET_LIFECYCLE.md` — review
any statement of what `PersistenceStatus::None` means and align it. Close the issue.

## Design Dependencies

- `overlaps` `stale-dependency-status-finalization` (where the issue was found) — no ordering.
- `overlaps` `record-streams` — fixed the `stored: false` trigger in the caller; this design
  removes the remaining duplicate check's wrong result.
- `overlaps` `recipe-provider-listing-contract` — `save_to_store` calls
  `refresh_listing_version` after a successful `store.set`, and that design hooks the recipe
  provider's cache invalidation into it. This design keeps the call on the `Written` path only, so
  a skipped write notifies nobody. Either order of implementation works.
- `overlaps` `context-title-description` — the metadata a command sets through `Context` is what
  `save_to_store` writes; no interaction beyond sharing the file.

## Consolidated Findings

- Use a private outcome enum (`Written` / `Skipped`) rather than `Err(Error::cancelled)`:
  `classify_persistence_error` maps `ErrorType::Cancelled` to `NotPersisted` and would also set
  `last_persistence_error`, conflating a skip with a failure — exactly what the issue warns
  against.
- The second (post-serialization) cancellation check cannot be reached deterministically from a
  test without a production hook; it shares the same return value as the first, so the first
  check's test plus the type change prove it structurally. Do not add a test-only hook.
- The `stored: false` check inside `save_to_store` is now unreachable from
  `persist_with_status_tracking` (the caller returns first) but reachable from any future caller
  and from the metadata snapshot differing from the live one; it returns `Skipped` too.
- The one direct test caller (`assetref.save_to_store().await?` ≈11743) keeps compiling.

## Review

Small, private, and the target status is already established by the caller.
