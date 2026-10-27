---
id: WEB-JS-STORE-WITH-ONLY-GET-CANNOT-BE-QUERIED
kind: issue
title: A page-implemented store with only get, documented as enough, fails every -R/ query
status: draft
priority: P2
complexity: S
area: [web]
design:
created: 2026-10-26
github:
---
## Problem

`LiquersStoreObject` in `liquers-web/src/typescript.rs`, `liquers-web/README.md` ("A page-implemented
store needs only `get`") and the `js_store.rs` module documentation all say only `get` is required.
A `-R/` query does not start with `get`: the recipe provider calls `contains` on the folder's
`recipes.yaml` and on the key, and a miss calls `isDir`. Each omitted method reports
`key_not_supported`. Measured on a wasm build (2026-10-26): with `{ get }` alone,
`evaluate("-R/notes/a.txt")` rejects with `key_not_supported … (no 'contains' method)`; with
`{ get, contains }` the read works, and a missing key rejects with `key_not_supported … (no 'isDir'
method)` instead of `asset_not_found`.

## Impact

Every store written from the documented minimum fails on its first query. Workaround: implement
`contains` and `isDir` too.

## Expected behaviour

Either the adapter derives `contains` from `get` (and answers `isDir` as `false`) when they are
omitted, or the declaration and the README name the methods a queried store needs. A `STORE` test
should evaluate a `-R/` query against a minimal store.

## Discovery

Running the examples of `specs/guides/JAVASCRIPT_GUIDE.md` under Node (2026-10-26).
