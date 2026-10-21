---
id: JS-STORE-RESOURCE-NOT-FOUND-WITHOUT-A-RECIPE
kind: issue
title: A resource served by a page-implemented js store fails with "No recipe found"
status: draft
priority: P2
complexity: S
area: [web, core/assets]
design:
created: 2026-09-27
github:
---
# A resource served by a page-implemented `js` store fails with "No recipe found"

## Problem

The e2e test `STORE07 a JavaScript store evaluates end to end`
(`liquers-web/tests/e2e/store.spec.ts`) registers a store object implementing only `get` and
`contains`. It routes the `mem` prefix to that object and evaluates `-R/mem/greeting.txt/-/to_text`.
The object answers `contains('mem/greeting.txt')` with `true` and `get` with the data. The query
fails with `No recipe found for key mem/greeting.txt`.

So the asset layer does not see the resource as present in the store, and it falls through to
recipe evaluation. One likely cause is that a method the object does not implement (`getMetadata`,
for one) is read as "absent" rather than derived from `get`. This is unconfirmed.

## Root cause, from reading the code at HEAD (2026-10-20)

Not a missing method, and not in core. `JsStore::get` (`liquers-web/src/store/js_store.rs`) turns
the page's `metadata` (here `{ media_type: 'text/plain' }`, or nothing) into a record through
`metadata_from_js_value` (`liquers-web/src/store/mod.rs`), which starts from `MetadataRecord::new()`
and so leaves `status` at its default, `Status::None`. It never calls `finalize_metadata`, which is
where every other store turns "data present, no status" into `Status::Source`
(`liquers-core/src/store.rs` `AsyncStore::finalize_metadata`; `AsyncOpenDALStore::get_metadata`,
the memory store and `LocalStorageStore` all go through it, and `FetchStore`'s `infer_metadata`
sets `Source` itself).

`AssetData::try_fast_track` (`liquers-core/src/assets.rs`) only adopts a stored entry whose status
is `Ready`, `Source` or `Override`; `None` logs "has stored status None, fast-track disabled" to
stderr and returns `Ok(false)`. The immediate manager then runs the asset's recipe, finds none, and
reports `No recipe found for key mem/greeting.txt`. `contains` and `get` were called and answered
correctly, which is why the symptom looked like the store being invisible.

So the fix lands in `liquers-web` alone: finalize the record read back from `get` (and from
`getMetadata`) the way `finalize_metadata(update = false)` does, so a page's data with no declared
status reads as `Source`, and add a Node-harness test (`tests/store_js_STORE.rs`) that evaluates
`-R/<prefix>/<key>/-/to_text` through a `get`-only object, so the e2e test is no longer the only
guard. Complexity re-estimated `M` → `S` for that reason: one adapter, one test.

The core issues with the same message — `MISSING-RESOURCE-REPORTED-AS-GENERAL-ERROR` (the
not-found error type and key) and `DEGRADED-STORED-VALUE-WITHOUT-RECIPE-REPORTS-NO-RECIPE` (the
fast-track refusal reason is dropped) — are separate causes. Either fix would have made this one
self-explaining (the refusal reason would have named `status None`), but neither fixes it.

## Problem Example

A store object `{ get(k) { return { data: bytes, metadata: { media_type: 'text/plain' } } },
contains(k) { return true } }` mounted at `mem`, then `env.evaluate('-R/mem/greeting.txt/-/to_text')`:
today it rejects with `No recipe found for key mem/greeting.txt`; it should resolve to the text.

## Expected behaviour

A store object implementing `get` and `contains` serves a resource query, as the e2e test
expects. Otherwise, the store should refuse the object at registration, naming the
method it lacks.

## Discovery

Found 2026-09-27 in record-streams Step 8.3, the first full `liquers-web` e2e run in some time.
The design's base commit (`686c038`) fails identically, so this predates `record-streams`.
