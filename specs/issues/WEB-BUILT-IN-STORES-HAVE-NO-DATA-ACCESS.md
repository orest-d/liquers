---
id: WEB-BUILT-IN-STORES-HAVE-NO-DATA-ACCESS
kind: issue
title: The browser's built-in stores refuse data-only access, so they cannot take a metadata section
status: draft
priority: P3
complexity: S
area: [web, core/store]
design:
created: 2026-10-15
github:
---
## Problem

A `metadata` section requires the wrapped store to implement data-only access
(`AsyncStore::data_capabilities`). In `liquers-web` only `JsStore` does, when the page's object
has the five data methods. `LocalStorageStore` (`liquers-web/src/store/local_storage.rs`) and
`FetchStore` (`fetch.rs`) keep the refusing defaults, so `type: localstorage` with a `metadata` section fails at build.

## Impact

A browser application cannot choose a storage model or handlers for its persistent built-in store;
it must write a JavaScript store object to get them.

## Expected behaviour

`LocalStorageStore` implements `get_data`, `set_data`, `remove_data`, `listdir_data` and
`data_facts` over its existing entries, reports its capabilities honestly, and runs under the
`per_entry` and `grouped` conformance configurations like the native stores.

## Discovery

Found implementing step 15 of `design/store-adapters/`.
