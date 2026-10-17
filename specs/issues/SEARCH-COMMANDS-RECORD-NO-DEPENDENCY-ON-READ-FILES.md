---
id: SEARCH-COMMANDS-RECORD-NO-DEPENDENCY-ON-READ-FILES
kind: issue
title: ns-search results do not depend on the files they read, so they go stale
status: closed
priority: P2
complexity: M
area: [lib/commands, core/assets]
design: search-read-dependencies
created: 2026-10-17
github:
---

## Problem

The `ns-search` commands read the store directly. `catalog::walk` lists folders through
`assets.listdir` and `store.is_dir` (`liquers-lib/src/search/catalog.rs`, around lines 59–85),
`read_text` takes the bytes with `get_async_store().get(key)` (`liquers-lib/src/search/mod.rs`,
around line 159), and `info_of` uses `get_asset_info` (around lines 172–177). None of these records
a dependency, and no search command is `volatile`. A cached `catalog`, `field_extract` or `search`
asset therefore stays `Ready` after a file under its folder is edited, added or removed, and every
asset built on it is served stale.

## Impact

Context packs and indexes built from search results (`design/agent-memory-mvp`, the
`liquers-template` problem example) do not update when an issue file changes, which is the point of
rendering them as assets. Found while planning `design/liquers-mustache` Phase 3: the recompute test
had to use a plain `-R/` key instead of the search query. Workaround: evaluate with a fresh
environment, or remove the cached asset.

## Expected behaviour

A search result depends on what it read: each file read through the asset layer (or its version
recorded through the dependency API), and each listed folder recorded so that an added or removed
file invalidates the listing. Where that is too costly, the commands are declared `volatile` or
given an expiration, and the documentation says so.

## Resolution (2026-10-17)

Fixed by `design/search-read-dependencies/` (PR #141). Every folder a search command walks is
recorded as `-R-dir/<folder>` through the new `Context::record_listing`, and every file listed or
read as `-R/<key>`; in core, a non-keyed dependency's run-time records are inherited by its
dependent, so chains (`…/catalog-t-t/search-x`) and recipe keys built on a search expire too.
Evidence: `liquers-lib/tests/search_commands.rs` (`search_catalog_*`, `search_document_commands_*`,
`search_over_a_cut_catalog_*`, `search_root_catalog_*`, `search_keyed_recipe_*`) and
`liquers-core/tests/cache_strategy.rs` (`cut_predecessor_runtime_dependency_reaches_dependent_*`).
A file written into a new subfolder is still missed:
`NEW-FOLDER-WRITE-DOES-NOT-REFRESH-THE-PARENT-LISTING`.
