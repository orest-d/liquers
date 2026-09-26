---
id: DIRECTORY-KEY-CANNOT-BE-EVALUATED-AS-A-RESOURCE
kind: issue
title: A query on a directory resource fails with "No recipe found" instead of giving a Directory state
status: draft
priority: P2
complexity: M
area: [core/assets]
design: record-streams
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
