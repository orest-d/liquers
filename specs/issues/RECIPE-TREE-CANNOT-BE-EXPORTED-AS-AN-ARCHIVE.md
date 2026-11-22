---
id: RECIPE-TREE-CANNOT-BE-EXPORTED-AS-AN-ARCHIVE
kind: feature
title: A recipe tree cannot be exported with its source data as one archive, or imported elsewhere
status: draft
priority: P3
complexity: L
area: [core/store, core/assets]
design:
created: 2026-11-22
github:
---
## Problem

A recipe tree (design `recipe-tree`) is relocatable inside one store: its references are relative,
so a copy of the folder to another key means the same thing. Nothing moves it *between* stores or
installations. There is no store-level export of a folder's data together with its metadata
(status `Source` / `Override`, media type, the asset's recorded facts), and no import that writes
them back, so the `Source` and `Override` assets a tree depends on cannot travel with its
`recipes.yaml`.

## Impact

A tree cannot be shared as a file (attached, downloaded, versioned outside the store). The
workaround is copying the store's files by hand, which loses or mis-states the metadata of
overridden assets and depends on the store's on-disk layout.

## Expected behaviour

An export of a tree folder to one archive (a zip) holding `recipes.yaml` with its `tree:`
declaration, the data and metadata of every `Source` and `Override` node, and optionally the
generated assets; and an import into any folder of any store. The import refuses or reports a
reference that the tree's containment check would refuse at the target (a `shared` folder that does
not exist there). It needs store-level export and import of data with its metadata, which other
uses (backup, moving a folder between stores) would share.

## Discovery

Split out of `RECIPES-CANNOT-DESCRIBE-A-SELF-CONTAINED-ASSET-TREE` (expected behaviour item 7) when
the `recipe-tree` design started on 2026-11-22: it is store work rather than recipe work, and the
tree design is complete without it.
