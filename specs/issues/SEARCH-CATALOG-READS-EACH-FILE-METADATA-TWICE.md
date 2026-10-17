---
id: SEARCH-CATALOG-READS-EACH-FILE-METADATA-TWICE
kind: issue
title: ns-search/catalog reads each file's metadata twice, once for its info and once for its version
status: draft
priority: P3
complexity: S
area: [lib/commands, core/assets]
design: 
created: 2026-10-17
github:
---
## Problem

Since `design/search-read-dependencies/`, `catalog` records every file it lists at its version.
`catalog::infos` (`liquers-lib/src/search/catalog.rs`) calls `AssetManager::get_asset_info` and then
`AssetManager::version` for each file. Both read the same metadata: on a file store, `contains` plus
a metadata read each, so the store is read twice per entry. `AssetInfo` has no version, so the
catalog cannot take it from the first read.

## Impact

Cost only. Measured on `AsyncFileStore`, 5 000 files, debug build, cold: a catalog takes 1.7 s
before the design, 2.5 s after it, and 1.9 s with the version read removed. The version read is
about four fifths of the added cost. On a memory store the difference is small.

## Expected behaviour

One metadata read per entry. Options: an `AssetManager` method returning the info and the version
together (a new `pub` item in `liquers-core`), or a `version` field in `AssetInfo` (a serialized
format change, also visible over HTTP and in the bindings). The first is smaller.

## Discovery

Measuring `design/search-read-dependencies/` (Phase 4 step 6, 2026-10-17): the file-store catalog
came out at about 1.5x the pre-change time, the limit the design set; disabling the version read
brought it to 1.1x.
