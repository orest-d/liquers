---
id: CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING
kind: issue
title: AsyncMemoryStore::makedir succeeds without creating a directory
status: closed
priority: P0
complexity: S
area: [core/store]
design: opendal-path-mapping
created: 2026-09-02
github:
---
## Problem

`AsyncMemoryStore::makedir` (`liquers-core/src/store.rs:888`) is a no-op that reports success:

```rust
async fn makedir(&self, key: &Key) -> Result<(), Error> {
    let key = key.as_absolute()?;
    Ok(())
}
```

It validates the key and then discards it. Nothing is recorded, so immediately afterwards
`is_dir(key)` is `false`, `contains(key)` is `false` and `listdir(parent)` does not show the name.
The caller is told the directory was created and no directory exists.

The cause is structural rather than an oversight: `AsyncMemoryStore`'s `dir_index` is *derived* from
stored keys, and a derived index has no way to represent a directory with no children.
`LocalStorageStore` (`liquers-web/src/store/local_storage.rs:98`) hit the same wall and grew a
separate `explicit_dirs` set beside its derived map.

## Impact

`makedir` is a documented operation, not an internal detail: `PUT /api/store/makedir/{*key}` is
specified in `reference/WEB_API_SPECIFICATION.md` §4.1.10, with a GET variant. Against a
memory-backed store the endpoint returns success and changes nothing, and a subsequent
`GET /api/store/is_dir/...` contradicts it.

`priority: P0` follows the vocabulary in `DOCS_STRUCTURE_GUIDE.md` §4.4 — "a documented feature that
does not work" — rather than from the size of the consequence, which is small: the memory store is
used for tests, for scratch space and as a router layer, so an empty directory that fails to persist
rarely matters in practice. `AsyncFileStore::makedir` (`:1244`) creates the directory properly, so a
router mixing the two answers the same call two different ways.

## Expected behaviour

`makedir(key)` records the directory so that `is_dir(key)` is `true`, `contains(key)` is `true` and
`listdir(key.parent())` includes its name, until the directory is removed. An explicitly created
directory is distinct from one derived from children: removing the last child of a derived directory
retires it, while an explicitly created one persists until `removedir`.

`CORE-DIRECTORY-INDEX-NOT-SHARED` puts exactly that distinction in `liquers-core` as
`DirectoryIndex`'s `explicit` set, so the fix here is for `AsyncMemoryStore::makedir` to call
`insert_directory`.

## Discovery

Found on 2026-09-02 writing the Phase 3 test plan of
[`design/opendal-path-mapping/`](../design/opendal-path-mapping/): a characterization test for
`AsyncMemoryStore`'s directory behaviour, written to pin that behaviour before extracting the index,
had to assert that `makedir` does nothing in order to pass.

## Resolution, 2026-09-02

`AsyncMemoryStore::makedir` calls `DirectoryIndex::insert_directory`, and `removedir` calls
`remove_directory`. An explicitly created directory now exists without children and outlives losing
them, which is what `makedir` means. `memdir04` asserts it — the test was written asserting the
opposite, because that was the behaviour it had to characterize before the index moved.

Fixed as a commit separate from the extraction, so the extraction stayed provably
behaviour-preserving and the one behaviour change is visible on its own.
