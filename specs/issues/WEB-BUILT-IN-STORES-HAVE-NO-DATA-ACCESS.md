---
id: WEB-BUILT-IN-STORES-HAVE-NO-DATA-ACCESS
kind: issue
title: The browser's built-in stores refuse data-only access, so they cannot take a metadata section
status: closed
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

## Resolution (2026-10-16)

Fixed in this PR.

- `LocalStorageStore` implements every data-only method over its `d` entries — `get_data`,
  `set_data`, `remove_data`, `rename_data`, `listdir_data`, `data_facts` — and
  `get_stored_metadata` over its `m` entry, so `native` storage works. None of the data methods
  reads or writes an `m` entry. Capabilities: `data_access`, `atomic_write` (one `setItem`),
  `rename` (copy then remove with no `.await` between, so nothing on the page sees the middle),
  `etag` (FNV-1a of the stored envelope, so a same-size overwrite is noticed); not
  `modified_time`, because the store keeps no write time.
- `FetchStore` implements the read half: `get_data`, `listdir_data` (the configured key set),
  `data_facts` (one `HEAD`: `Content-Length`, `ETag`, `Last-Modified` as a seconds-precision
  `BackendTime`, falling back to `GET` as `get_metadata` does) and `get_stored_metadata` (the
  record it has always derived from the response headers, so `native` and `read_only` see what
  `get_metadata` returned before). `set_data` / `remove_data` refuse with `NotSupported`;
  `rename_data` keeps the default refusal. `data_access` is reported; `modified_time` and `etag`
  are reported when the server sends them but not promised.

Tests:

- `liquers-web/tests/store_local_STORE.rs` (browser, `--features browser-tests`):
  `data_only_methods_round_trip_without_metadata`,
  `store16_local_storage_under_metadata_sections_round_trips` (`per_entry`, `grouped`, `native`).
- `liquers-web/tests/store_conformance_browser_CONF.rs` (browser): `C11`,
  `c11_local_storage_store_under_metadata_sections` — the conformance suite under `per_entry` and
  `grouped`.
- `liquers-web/tests/store_conformance_CONF.rs` (Node): `fetch_store_data_only_read_half`,
  `fetch_store_under_grouped_metadata_section`.

`specs/reference/STORE_SEMANTICS.md` §11 still says the built-in browser stores refuse data
access; that sentence needs updating with this change's History row.
