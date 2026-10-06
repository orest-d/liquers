# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** The fix applies an existing, documented rule (`live_status_defers_to_store`)
  to two more call sites. It adds no API and changes no data format.
- **Open questions:** None

## Problem

`AssetManager::get_any_status` and `get_binary_any_status` (the default trait methods in
`liquers-core/src/assets.rs`) answer from the live asset whenever `lookup_key_asset` finds one. A
concurrent `get` maps a placeholder asset (status `None` or `Recipe`) *before* it fast-tracks the
stored value. A recovery read that runs between those two steps returns `Ok(None)` for a key whose
value is in the store. The axum Assets API uses `get_binary_any_status`
(`liquers-axum/src/assets/key_handlers.rs`). Observed in a draft of
`external_change_integration::concurrent_reads_apply_external_change_once`.

## Expected behaviour

If the live asset's status is `None` or `Recipe`, it has not produced or loaded anything yet. The
recovery reads then take the store path, exactly as `remove` and `expire` already do.

Acceptance criteria:

1. A key with a stored `Ready` value and a live `None`/`Recipe` placeholder: both recovery reads
   return the stored value (bytes and metadata).
2. Same placeholder, nothing stored: `Ok(None)`, as today.
3. A live asset in any other status (including `Expired` with a retained value, `Error`,
   `Processing`) answers as today.
4. The recovery reads still never submit an evaluation and never register an asset.

## Scope and non-goals

- In scope: the two default `AssetManager` methods, which both built-in managers inherit.
- Out of scope: the in-flight statuses (`Submitted`…`Storing`). Their live answer (`None`, or the
  previous value) is the current contract. Changing it would be a separate decision about reading
  a value that is being recomputed.
- Out of scope: `AssetRef::get_any_status`. It is an asset-level read and has no store to defer to.

## Affected users and systems

Recovery-read callers: the axum Assets API, and tests. The change is internal to `liquers-core`.

## Design Dependencies

- `dependency-audit-and-expiry-provenance` — **overlaps** (complete; frozen). It introduced
  `verify_store_read` on the store path, which the deferred read now also passes through. That is
  intended: the store path already verifies.

## Documentation assessment

- Reference: extend `specs/reference/ASSETS.md`, at the recovery-read description, by one sentence
  stating the deferral rule. No new reference.
- Guide: none.
- Other documents: none.
- Updates: the source issue's resolution.

## Consolidated Findings

- The rule already exists as `live_status_defers_to_store(status)`, and its doc comment names
  `remove`. Reuse it; do not duplicate the status list.
- Reading the live status and then the store is not atomic. A placeholder may load in between,
  but the store holds the same value the fast track loads, so the answer is still correct. The
  window that remains is a concurrent `remove`, which is the same window as for an unmapped key
  today.
- Validation: one deterministic unit test per method, which installs a placeholder with
  `AssetRef::new_from_recipe` and maps it without running it. No timing-dependent test.
