---
id: JS-STORE-PAGE-METADATA-STATUS
kind: design
title: A page-implemented store's data is loadable without a Liquers-written status
form: compact
workflow: liquers-project
status: in_review
phase: high-level
area: [web, core/store]
issues: [JS-STORE-RESOURCE-NOT-FOUND-WITHOUT-A-RECIPE]
created: 2026-10-20
---
# A page-implemented store's data is loadable without a Liquers-written status

## Phase 1: High-Level Design

### Purpose

`STORE07` for the `js` store fails: a resource that a page-implemented store serves cannot be
evaluated. The guide requires every selected store to work in end-to-end evaluation, so the
README's `STORE` row is marked complete while one of its three stores does not.

### Problem Example

The e2e test `STORE07 a JavaScript store evaluates end to end`
(`liquers-web/tests/e2e/store.spec.ts`) registers this object, routes the `mem` prefix to it and
evaluates `-R/mem/greeting.txt/-/to_text`:

```js
{
  async get(key) {
    if (key !== 'mem/greeting.txt') return undefined;
    return { data: new TextEncoder().encode('from the page'),
             metadata: { media_type: 'text/plain' } };
  },
  async contains(key) { return key === 'mem/greeting.txt'; },
}
```

**Today** it fails with `No recipe found for key mem/greeting.txt`. **It should** return
`'from the page'`.

**Cause (from reading the code; the Phase 3 test confirms it at run time).** A keyed resource is
loaded by `AssetData::try_fast_track` (`liquers-core/src/assets.rs`). It calls `contains`, which
answers `true`, and then `get`. Then it reuses the bytes only when the stored status is `Ready`,
`Source` or `Override`. Otherwise it treats them as a value that must be recomputed and falls
through to the recipe provider. There is no recipe for this key, hence the error.

The page's metadata has no `status`, so it is `Status::None`. Two things in `liquers-web` let that
through:

1. **The read path passes the page's record through unchanged.** `JsStore::get`
   (`liquers-web/src/store/js_store.rs`) returns what `metadata_from_js_value` builds. Other
   stores never return data with status `None`:
   - `AsyncStore::finalize_metadata` (`liquers-core/src/store.rs`) turns `None` into `Source`
     whenever data is present.
   - `AsyncFileStore::get` applies it when it has to synthesise a record for a file without one.
   - `FetchStore` builds its record with `Source`.

   The `http` and `localstorage` cases of `STORE07` pass for that reason.
2. **A partial object skips the key and filename seeding.** `MetadataRecord` is
   `#[serde(default, deny_unknown_fields)]`, so `{ media_type: 'text/plain' }` parses as a
   *complete* record on `metadata_from_js_value`'s first branch. Its `key` and `filename` stay
   unset, and its data format falls back to `bin`. The overlay branch, which seeds the key, is
   reached only by objects that carry unknown fields.

The write direction has the same gap. `JsStore::set` seeds the filename but never sets a status.
So bytes a page writes through `env.store().set(…)` with partial metadata are stored with status
`None`, and evaluating them later fails the same way. The Node suites miss this because their
fixtures either write a full record (`eval_EVAL.rs` sets `Status::Source` by hand) or never
evaluate.

### Scope and Acceptance Criteria

In scope: `JsStore`'s read and write paths, and the shared `metadata_from_js_value` helper.
Out of scope: changing what `try_fast_track` accepts in core.

- **AC-1** A page store with only `get` and `contains` serves a resource query
  - WHEN a `js` store's `get` returns data with metadata that has no `status`, and
    `-R/<prefix>/<key>/-/to_text` is evaluated
  - THEN the result is the store's text, and no recipe is consulted
- **AC-2** Missing metadata is treated the same as partial metadata
  - WHEN `get` returns `{ data }` with no `metadata` at all
  - THEN the key evaluates the same way, and the record has the key, the filename and a data format
    derived from the extension
- **AC-3** A status the page declares is kept
  - WHEN `get` returns metadata with `status: 'error'` (or any status other than `None`)
  - THEN that status is unchanged, and fast-track refuses it as it does today
- **AC-4** A page write evaluates afterwards
  - WHEN `env.store().set(key, bytes, {})` writes to a `js` store with a `set` method, and the key
    is then evaluated
  - THEN the bytes are served. The record handed to the page's `set` has status `Source`, its key,
    its filename and its file size, as `AsyncFileStore` would write it
- **AC-5** The existing e2e test passes unchanged
  - WHEN `npx playwright test` runs `liquers-web/tests/e2e/store.spec.ts`
  - THEN `STORE07 a JavaScript store evaluates end to end` passes

### Open Questions

1. **Where the default lives.** I recommend the adapter: `JsStore` applies
   `AsyncStore::finalize_metadata` on `get` and `set`, the same as the core stores. The
   alternative is for core's `try_fast_track` to accept `None` when bytes are present. That
   would also cover any future language store, but it changes the reuse rule for every store,
   and a store written by Liquers can never hold `None` with data anyway.
2. **`getMetadata` without data.** A page's `getMetadata` answer is a metadata-only read, so the
   adapter cannot tell whether bytes exist. I recommend seeding the key and the filename there,
   but leaving `status` as the page gives it. Fast-track never reads it.
3. **Fixing `metadata_from_js_value` for every caller.** The helper is shared with
   `LiquersStore` (`src/store/wrapper.rs`). I recommend making it seed the key on both branches.
   That changes what a page receives back from `getMetadata` on the wrapper only by filling in
   `key`.

Readiness: not assessed. Complexity is `M`, mostly in tests: the fix is two calls in
`js_store.rs` and one in `store/mod.rs`, but AC-4 needs a new Node test and AC-5 needs the e2e
loop.
