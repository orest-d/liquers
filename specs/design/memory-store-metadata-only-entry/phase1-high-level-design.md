# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** `specs/reference/STORE_SEMANTICS.md` already describes a metadata-only key as
  one that "appears in listings as a key `get` cannot read". `AsyncFileStore` behaves that way;
  `AsyncMemoryStore` does not. Aligning the memory store and pinning the rule in the conformance
  suite applies an existing contract. It is not a new decision.
- **Open questions:** None. Removing the `no_bytes_by_design` heuristic in `assets.rs` is part of
  this design, conditional on the new rule passing on every store in the suite (Phase 4 step 4).

## Problem

`AsyncMemoryStore::set_metadata` on a key with no data inserts an empty data object, so `get` and
`get_bytes` return `Ok(vec![])`. On a memory store, "metadata but no data object" (a
non-serializable value stored metadata-only by `set_state`) cannot be told from "the value is
the empty byte string". Part G (`verify_stored_versions`) works around it with the
`no_bytes_by_design` heuristic, which misclassifies a hand-emptied value under a timestamp-looking
legacy version.

## Expected behaviour and acceptance

1. Memory store: after `set_metadata(k, m)` on a new key, `contains(k)` is true, `get_metadata(k)`
   is `m`, `listdir` of the parent lists `k`, and `get(k)` / `get_bytes(k)` fail with
   `ErrorType::KeyNotFound`.
2. `set(k, b"", m)` stores an empty data object: `get_bytes(k) == Ok(vec![])`.
3. `set_metadata` on a key that has data keeps the data (unchanged).
4. A new conformance rule (`sidecar05`, see Phase 2) checks 1–2 on every store that accepts
   metadata-only writes, and passes for every store in `tests/store_conformance_CONF.rs`.
5. `no_bytes_by_design` is removed and Part G treats `KeyNotFound` on read as "no bytes, skip".

## Scope and non-goals

- In scope: `AsyncMemoryStore`, one conformance rule, the Part G heuristic.
- Not in scope: the fast-track handling of a metadata-only entry. That belongs to
  `metadata-only-entry-reload`, which depends on this design.

## Design Dependencies

- `metadata-only-entry-reload` — **required-by**. With a uniform "no data object" answer, the
  fast track can recognize a metadata-only entry without a metadata marker. **Recommended merge
  candidate.** The two fixes are one behaviour seen from the store and from the asset.
- `dependency-audit-and-expiry-provenance` — **overlaps** (complete). It introduced
  `no_bytes_by_design`.

## Documentation assessment

- Reference: `specs/reference/STORE_SEMANTICS.md`: make the existing sentence normative ("`get`
  and `get_bytes` report `KeyNotFound`") and add `sidecar05` to the "Enforced by" list.
- Guide: `STORE_IMPLEMENTATION_GUIDE.md` §9 table (rule counts change; hand-maintained).
- Updates: `liquers-core/src/assets.rs` comments that mention the memory store's empty bytes.

## Consolidated Findings

- The existing memory-store test asserts the old behaviour
  (`assert_eq!(store.get_bytes(&metadata_only_key).await?, Vec::<u8>::new())`, `store.rs` tests).
  It must change to expect `KeyNotFound`.
- **Update 2026-10-06 — wrong value, and the companion fix has landed.** A metadata-only `Text`
  entry stored as `txt` is *served as `""`* by the memory store (empty bytes deserialize as empty
  text), so the issue is a correctness bug, not just a log message (priority raised to P2). The
  `try_fast_track` change of `metadata-only-entry-reload` is implemented, so this design no longer
  has to land together with it. Add the reproduction as test T7: a memory-store
  `metadata_only_entry_on_memory_store_is_recomputed` in `liquers-core/tests/metadata_only_entry_reload.rs`,
  which returns `""` today.
- **Latent defect found (fixed 2026-10-06, `FAST-TRACK-FAILS-ON-METADATA-ONLY-FILE-STORE-ENTRY`):**
  `AssetData::try_fast_track` called `store.get(&key).await?`. On a file
  store a metadata-only `Ready` entry makes `get` return `KeyNotFound`, which propagates out of the
  fast track and fails the asset's `get` instead of recomputing. After this design the memory
  store behaves the same, so `metadata-only-entry-reload` **must be implemented together** or the
  memory store gains the file store's failure. This is the strongest argument for merging the two.
- The JS/browser stores (`JsStore`, `LocalStorageStore`) and OpenDAL must be checked by the new
  rule. A store that answers empty bytes either gets fixed in the same PR (if trivial) or is listed
  as an allowed failure with a filed issue, and `no_bytes_by_design` then stays until it is fixed.
