# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | unit | Router{mem: `mem`, files: `files`}: `get_metadata(&Key::new())` is a dir record, children names = {`mem`, `files`} |
| T2 | unit | Router{memory at `a/b`}: `get_metadata(a)` is a dir with child `b` |
| T3 | unit | Router: `get_metadata(zzz)` (no member, not a dir) is `KeyNotFound` |

Setup: `AsyncStoreRouter::new()` + `add_store(Box::new(AsyncMemoryStore::new(&parse_key("mem")?)))`
etc. Write one key into each member first so member listings succeed. Names:
`router_root_metadata_lists_members`, `router_intermediate_directory_metadata`,
`router_unowned_key_metadata_not_found`.
