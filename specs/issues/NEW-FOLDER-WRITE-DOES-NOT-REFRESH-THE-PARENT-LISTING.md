---
id: NEW-FOLDER-WRITE-DOES-NOT-REFRESH-THE-PARENT-LISTING
kind: issue
title: A write that creates a folder does not refresh the listing of the folder above it
status: closed
priority: P3
complexity: S
area: [core/assets]
design: new-folder-write-ancestor-listing-refresh
created: 2026-10-17
github:
---
## Problem

Every write or removal through the asset manager calls `refresh_listing_version(&key.parent())`
(`liquers-core/src/assets.rs`): it re-reads the listing of the folder the key is in, and expires
what depends on it if it moved. When the write creates that folder (writing `a/new/x.md` where
`a/new` did not exist), the listing of `a` changes too, since it gains `new`, but nothing refreshes
it. The mirror case is a removal that empties a folder on a store where an empty folder disappears.

So a dependent of `-R-dir/a` (a plan's `GetAssetDirectory`, or `Context::record_listing`) is not
expired, and serves a listing without the new folder until something else in `a` changes.

## Problem Example

Evaluate `index.txt = -R-dir/data/-/index_files`, then `set_binary("data/new/x.txt", ...)`,
which creates `data/new`. Today `index.txt` stays `Ready` without `data/new`. It should be expired,
with root `-R-dir/data`.

## Impact

Small but silent. `ns-search/catalog` over `a`, recursive, misses every file written into a new
subfolder of `a` (`specs/design/search-read-dependencies/`, test
`search_catalog_follows_added_and_removed_files` covers only existing subfolders). An existing
subfolder is fine: its own listing is recorded and refreshed.

## Expected behaviour

A write refreshes each ancestor whose membership it changed. Recommended: after refreshing
`key.parent()`, continue to the next ancestor only while the folder just refreshed is new to its
parent (absent from the parent's registered listing, or the parent's listing version moved), so an
ordinary write still costs one refresh. Removal does the same while a folder disappears.

## Discovery

Implementing `design/search-read-dependencies` (2026-10-17): a catalog of `specs/` did not show
`specs/design/y/DESIGN.md` written into the new folder `specs/design/y`, while files written into
`specs/design/x` appeared.

## Resolution

2026-10-18, by `design/new-folder-write-ancestor-listing-refresh/`.
`AssetManager::refresh_listing_version` (`liquers-core/src/assets.rs`) now walks from the written
key's parent towards the root, through the private helpers `refresh_one_listing` and
`has_registered_ancestor_listing`. It climbs past a registered listing only when its old or new
listing is empty, and past an unregistered one only when some higher listing is registered, so an
ordinary write still reads one listing and a write under no registered listing reads none. The
issue's recommended rule ("the parent's listing version moved") was refined this way because an
ordinary new file moves its folder's version too. Tests in
`liquers-core/tests/dependency_audit_integration.rs`:
`writing_into_a_new_folder_expires_the_parent_index`,
`writing_into_a_new_deep_chain_expires_the_index`, `emptying_a_folder_expires_the_parent_index`,
`ordinary_write_reads_one_listing`, `write_without_registered_listings_reads_none`.
