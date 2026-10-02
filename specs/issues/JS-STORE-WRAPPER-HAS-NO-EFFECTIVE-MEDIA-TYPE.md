---
id: JS-STORE-WRAPPER-HAS-NO-EFFECTIVE-MEDIA-TYPE
kind: issue
title: A page can read a key's declared metadata but not its effective media type
status: draft
priority: P3
complexity: S
area: [web]
design:
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
