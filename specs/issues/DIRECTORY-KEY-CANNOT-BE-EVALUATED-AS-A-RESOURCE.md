---
id: DIRECTORY-KEY-CANNOT-BE-EVALUATED-AS-A-RESOURCE
kind: issue
title: A query on a directory resource fails with "No recipe found" instead of giving a Directory state
status: closed
priority: P3
complexity: M
area: [core/assets]
design: key-without-recipe-answer
created: 2026-09-26
github:
---
# A query on a directory resource fails with "No recipe found" instead of giving a Directory state

## Problem

The stores already describe a directory. `get_metadata` on a key that `is_dir` builds its metadata
with `Status::Directory` (`liquers-core/src/store.rs`, `default_metadata(key, true)`), and
`Status::Directory` is a known terminal state with `ReadExposure::MetadataOnly`
(`metadata.rs`). But the asset manager's `get(key)` path never asks the store whether a key is a
directory. It finds no data for the key and goes to the recipe provider, which fails:

```
-R/data/-/ns-rec/file_records   →   "No recipe found for key data"
```

The only `is_dir` calls in `assets.rs` are in the listing functions (`listdir_keys_deep`).

## Why it matters

`record-streams`' primary scenario lists a directory as a table:
`-R/data/-/ns-rec/file_records/files.csv` (`design/record-streams/phase3-examples.md`, Example 1).
The command itself works, because it reads only the state's metadata key. The query that is
supposed to reach it cannot. So the scenario's test calls `file_records` directly, with a
hand-built state, rather than through `evaluate`. Any other command meant to act on a folder has
the same problem.

## Expected behaviour

When a key has no data and no recipe, `get(key)` asks `store.is_dir(key)`. For a directory it
returns an asset in `Status::Directory` carrying the store's directory metadata. The state has no
value, and its metadata names the key, which is what `file_records` reads. Directory status is
terminal and records no value, so it should not be cached as data or written back.

## Discovery

Found 2026-09-26, `record-streams` Phase 4 Step 5.5, while testing `ns-rec/file_records` end to
end.

## Update 2026-09-27 — the scenario works through the `sdir` header

The files-to-records scenario now runs through the query API as
`-R-sdir/data/-/ns-rec/file_records/files.csv` (the `sdir` header already existed and yields the
store's listing). What was missing was the key, and the interpreter now carries it across that
header's predecessor boundary: `fetched_key` in `liquers-core/src/interpreter.rs`, tested by
`fetched_key_honours_the_resource_header` and, end to end, by `records_end_to_end.rs`
`file_records_lists_a_store_directory_through_a_query`. The reference and guide give that query.

What this record asks for is still open: a **plain** `-R/data/-/…` still fails ("Key not found"),
because `get(key)` never asks the store whether the key is a directory. Priority lowered to P3
because a working spelling exists.

## Reproduction at HEAD (bulk design, 2026-10-20)

Still reproduces at 18732dd. Found during `design/record-streams/` (complete), which this issue no
longer links as its design. `-R/data/…` plans `GetAsset(data)`; the fast track sees `contains` true
and `get` → `KeyNotFound` (a directory has no data object), declines as "stored without bytes", and
the evaluation fails at the recipe provider ("No recipe found for key data", as the test comment in
`liquers-lib/src/records/commands.rs` records). The same holds under a router for a key above its
mounts (`AsyncStoreRouter::get` answers `KeyNotFound`, `is_dir` answers true). Designed in
`design/key-without-recipe-answer/` (same branch as `MISSING-RESOURCE-REPORTED-AS-GENERAL-ERROR`).

## Resolution

2026-10-25, `design/key-without-recipe-answer/`. `-R/<dir>` with no recipe is a `Directory` state (no job, value none, the key and `is_dir` in its metadata), evicted on the next request so the directory is re-read; a recipe for a directory key fails with `AssetNotSupported` (Orest, 2026-10-25). `-R/files/-/ns-rec/file_records` now lists the directory. Tests: `key_without_recipe_tests::directory_key_gives_a_directory_state`, `immediate_manager_directory_key_gives_a_directory_state`, `directory_key_with_recipe_is_asset_not_supported`; `liquers-lib/tests/records_end_to_end.rs` `file_records_lists_a_store_directory_through_a_query`.
