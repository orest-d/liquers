---
id: STORE-OPENDAL-FS-CONCURRENT-WRITES-TO-ONE-KEY-FAIL
kind: issue
title: Concurrent writes to one key of an OpenDAL fs store can fail with a write error
status: draft
priority: P2
complexity: M
area: [store/backends]
created: 2026-11-01
github:
---
## Problem

Ten concurrent `set` calls to the same key of an OpenDAL `fs` store (`STORE06`, "last write wins,
not a torn value") sometimes fail instead of one of them winning:

```
KeyWriteError: Key 'd/race' write error by store  OpenDAL Store: Unexpected (permanent) at ,
context: { expect: 531, actual: 532 } => writer got too much data (OpenDAL Write Error)
```

About one run in thirty, on Linux, through `liquers-py`'s
`test_STORE06_concurrent_update_policy[fs]` (ten `set_async` calls gathered on one event loop).
The sizes are those of the metadata sidecar: `AsyncOpenDALStore::set`
(`liquers-store/src/opendal_store.rs`) writes the sidecar, then the data, then the sidecar again,
each with a plain `op.write` to a fixed path. Two writers of the same path interleave, and OpenDAL's
`fs` writer checks the length it was promised against what reached the file.

## Impact

`STORE06` does not hold for the `fs` type: a concurrent writer gets an error instead of last-write
wins. Readers are not affected beyond the usual absence of a precondition
(`STORE-WRITE-HAS-NO-PRECONDITION`). The memory and `filesystem` stores pass the same test.

## Expected behaviour

Concurrent writes to one key either all succeed with one value winning, or the store documents
that it refuses concurrent writers. Candidates: write through a temporary path and rename
(OpenDAL's `atomic_write_dir`, already an `fs` option), or serialize writes per key inside the
store. A Rust conformance case for concurrent same-key writes would catch it for every backend.

## Discovery

Found by the `liquers-py` store conformance suite (`specs/design/liquers-py-store/`, Phase 3,
`test_STORE_conformance.py`), where the `fs` parameter of `STORE06` is marked `xfail` with this
issue as the reason until it is fixed.
