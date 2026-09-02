---
id: WEB-JS-STORE-HAS-NO-DIRECTORY-METADATA
kind: issue
title: JsStore::get_metadata delegates a directory key to get, which throws
status: draft
priority: P2
complexity: S
area: [web, core/store]
design: store-conformance-suite
created: 2026-09-02
github:
---
## Problem

`JsStore::get_metadata` falls back to `get(key)` when the delegate provides no `getMetadata`. For a
**directory** key that is wrong: a directory has no data, so the delegate throws and the adapter
reports `KeyReadError`.

```
ERROR dir04 KeyReadError: Error: not found: …/d0     (via get_metadata → get)
ERROR dir07 KeyReadError: Error: not found: …/d0
```

`STORE_SEMANTICS.md` §2 requires a directory's metadata to be `default_metadata(key, true)` — a
record with `is_dir == true` carrying its key. `get_asset_info` is built on `get_metadata`, so a
store that cannot produce directory metadata cannot answer `-R-dir/` queries at all.

## Impact

A page-defined store that answers `isDir` and `listdir` correctly — as the documented protocol
invites — still cannot have its directories inspected. The failure surfaces as a read error naming
a key the caller believes is a directory, which reads as a bug in the caller.

## Expected behaviour

`get_metadata` consults `is_dir` first and returns `default_metadata(key, true)` for a directory,
delegating to `getMetadata`/`get` only for a data key. That is what `AsyncMemoryStore`,
`AsyncFileStore` and `AsyncOpenDALStore` all do; `JsStore` is the outlier.

## Discovery

Found on 2026-09-02 by `C10` of the conformance suite (Phase 4 step 12 of
`design/store-conformance-suite/`). Recorded as an allowed failure on that suite so it cannot be
forgotten and cannot outlive its fix.
