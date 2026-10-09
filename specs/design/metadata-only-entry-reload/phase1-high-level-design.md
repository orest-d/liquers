# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — bug fix in `liquers-core` `try_fast_track`; no API change
  (already implemented)
- **Leading issue:** None
- **Explanation:** With `memory-store-metadata-only-entry`, every store answers a metadata-only key
  with `KeyNotFound` on `get`, so the fast track can recognize the case without a new metadata
  field. The fix also removes a latent failure on file stores (below).
- **Open questions:** None. The issue suggested a marker or a new status. The store-level answer
  makes both unnecessary, and a marker would be a serialized-format addition. That rejection is
  recorded in Phase 2 for the reviewer.

## Problem

When a keyed value has no byte form, `set_state` (step 8) writes metadata only, with the value's
own status (typically `Ready`). On the next request, `AssetData::try_fast_track`
(`liquers-core/src/assets.rs`):

- **memory store:** `store.get` returns empty bytes, `deserialize_stored_value` fails, and the asset
  logs "fast-track failed to deserialize stored value (treated as corrupted)" before recomputing;
- **file store (found during this design):** `store.get` returns `KeyNotFound`, which propagates
  through `?` out of `try_fast_track` and `fast_track`, so the manager's `get` **fails** instead of
  recomputing.

More values take this path over time: UI elements, egui widgets, foreign handles, and non-manifest
`RecordSource`s without `stored: false`.

## Expected behaviour and acceptance

1. A metadata-only stored entry with a reusable status: the fast track returns `Ok(false)` without
   a corruption message, and the asset is evaluated from its recipe. This holds on memory and file
   stores.
2. A stored entry whose bytes exist but do not deserialize still reports corruption (unchanged).
3. A metadata-only entry for a key with **no recipe** (a `Source` whose bytes are gone): `get`
   reports `KeyNotFound` for the key, as for an absent key. It does not report a corruption.
4. Diagnostic: one `eprintln!` line stating "stored without bytes; recomputing" (debug-level
   wording, stderr).

## Scope

`try_fast_track` only. The write path (step 8) is unchanged.

## Design Dependencies

- `memory-store-metadata-only-entry` — **requires**. It makes `KeyNotFound` the uniform answer.
  **Recommended merge candidate**; must land together at minimum (see that design's findings).
- `type-info-write-only-formats` — **overlaps**. A write-only format also reaches the load path,
  with non-empty, unreadable bytes. It is handled there by skipping deserialization for
  write-only formats, not by this design.

## Documentation assessment

- Reference: `specs/reference/ASSETS.md`, the fast-track description: one sentence ("an entry
  with metadata but no data object is re-derived from its recipe").
- No guide.

## Consolidated Findings

- The fix is a `match` on `store.get(&key).await` that treats `ErrorType::KeyNotFound` as "no
  bytes". It must not swallow other errors (I/O, permission), which keep propagating.
- `contains(&key)` is true for a metadata-only key (STORE_SEMANTICS), so the branch is reached.
- Acceptance 3 follows from the existing no-recipe path once the fast track returns `Ok(false)`.
  Verify in Phase 4 that the resource-without-recipe path then reports `KeyNotFound` rather than a
  confusing evaluation error.
