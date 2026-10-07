---
id: JS-STORE-WRAPPER-HAS-NO-EFFECTIVE-MEDIA-TYPE
kind: issue
title: A page can read a key's declared metadata but not its effective media type
status: closed
priority: P3
complexity: S
area: [web]
design: js-store-effective-media-type
created: 2026-09-30
github:
---

## Problem

`LiquersStore.getMetadata` (`liquers-web/src/store/wrapper.rs`) returns the raw
`MetadataRecord` JSON. Since the metadata level model, `media_type` in that record holds only a
*declared* override; the effective media type is derived from the data format
(`MetadataRecord::get_media_type`). A page therefore sees `media_type: null` for `input.csv` and has
no call that returns `text/csv`.

## Impact

A page that wants to set a `Content-Type`, pick a viewer or label a download must re-derive
the media type from `data_format` itself, duplicating a table Liquers already owns.

## Expected behaviour

A read that returns the effective values — a `getAssetInfo(key)` on `LiquersStore`, whose
`AssetInfo.media_type` is the effective string — or an `effectiveMediaType(key)` helper. Do not write
the derived value into the raw record: that would make every filename look like an override.

## Discovery

Found 2026-09-29 while resolving `HTTP-STORE-METADATA-DROPS-THE-EXTENSION-MEDIA-TYPE` in
`design/store-conformance-backlog/` (Phase 2 area F): the store was right and the e2e test was
stale, but the test showed what a page cannot ask for.

## Resolution (2026-10-07)

Fixed by `design/js-store-effective-media-type/`. `LiquersStore.getAssetInfo(key)`
(`liquers-web/src/store/wrapper.rs`) resolves to the key's `AssetInfo` as a plain object, whose
`media_type` is the effective one (`text/csv` for `input.csv`), plus `is_dir` and the other asset
info fields. `getMetadata` is unchanged and still returns the raw record. Tested in
`liquers-web/tests/store_wrapper_STORE.rs`, declared through the generated stubs (checked by
`check-stubs.sh` STUBS02), and documented in `liquers-web/README.md`.

The effective type derives from the data format, and a memory store does not seed that from the key
when the written metadata has no filename. That is filed as
`STORES-DISAGREE-ON-SEEDING-THE-DATA-FORMAT-FROM-THE-KEY`.
