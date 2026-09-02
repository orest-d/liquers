---
id: MEMORYSTORE-MAKEDIR-SUCCEEDS-WITHOUT-CREATING-A-DIRECTORY
kind: issue
title: MemoryStore::makedir succeeds without creating a directory
status: rejected
priority: P0
complexity: S
area: [core/store]
design: 
created: 2026-09-02
github:
---
## Problem

The synchronous `MemoryStore::makedir` in `liquers-core/src/store.rs` validates that its key is
absolute and returns `Ok(())` without recording the directory. A subsequent `is_dir(key)` is false
unless a stored descendant happens to make the directory exist implicitly.

The issue is rejected since the synchronous stores including `MemoryStore` are obsolete and will be removed.

## Impact

Callers receive success for a documented operation that did nothing. This is P0 under
`DOCS_STRUCTURE_GUIDE.md` because a documented feature does not work; creating a child key is an
imperfect workaround because it changes the requested store contents.

## Expected behaviour

`MemoryStore::makedir(key)` should record an explicit directory so `contains`, `is_dir`, and parent
listing agree until `removedir` removes it. The asynchronous implementation already uses the shared
`DirectoryIndex` to provide this behavior.

## Discovery

Found while checking synchronous `MemoryStore` parity for
`CORE-ASYNC-MEMORY-STORE-IS-SUPPORTED-IGNORES-PREFIX`. The similarly named asynchronous defect is
closed; this issue is distinct because the remaining no-op is in the synchronous implementation.
