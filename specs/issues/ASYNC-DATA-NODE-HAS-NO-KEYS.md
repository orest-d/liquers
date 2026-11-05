---
id: ASYNC-DATA-NODE-HAS-NO-KEYS
kind: issue
title: An async data node cannot list the field names of a map
status: draft
priority: P3
complexity: S
area: [core/value]
design:
created: 2026-11-04
github:
---

## Problem

`DataNode` has `keys()`, but `AsyncDataNode` (`liquers-core/src/data_node.rs`) has only `kind`,
`scalar`, `get`, `items`, `to_json`, `len` and `index`. An async map, such as the folder node a
template sees for `-R-dir/data` (`liquers-lib/src/template/folder.rs` `FolderNode`), cannot say
which names it holds except by writing itself out whole with `to_json`.

**Example.** A Python script holds a view of a folder (`env.data_node(env.evaluate_state("-R-dir/data"))`,
design `liquers-py-data-nodes`) and wants its entry names. `keys()` raises `NotSupportedError`,
and there is no `keys_async` to await. Expected: an awaitable list of names.

## Impact

Bindings cannot list an async map's fields; a template cannot iterate the names of a folder.
Workaround: list the folder with its own query (`-R-dir/data`) and read the listing.

## Expected behaviour

`AsyncDataNode::keys(&self) -> Result<Vec<String>, Error>` (async, defaulting to an empty list),
implemented by `FolderNode` and the state wrapper in `template/host.rs`; bindings then offer
`keys_async`.

## Discovery

Found on 2026-11-04 while designing `liquers-py-data-nodes` Phase 2 (its views have no
`keys_async` because of this).
