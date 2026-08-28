# Phase 1 — High-level design

For [`issues/STORE-OPENDAL-SLASH-HANDLING.md`](../../issues/STORE-OPENDAL-SLASH-HANDLING.md)
(issue, P1, complexity M, `status: accepted`).

## The problem as filed, and what reproduction found

The issue states: *"Keys that contain a `/` — which is to say most real keys — are not reliably
addressable through an OpenDAL-backed store."* It cites the `// FIXME: This currently does not work
due to some bug with handling '/'` at `liquers-store/src/opendal_store.rs:335` and four
`//TODO: create_dir` markers.

Reproduced against `HEAD` with a scratch integration test over
`AsyncOpenDALStore` (deleted afterwards; results below are the raw output):

**Filesystem backend** (`opendal::services::Fs`, key `sub/deeper/foo.txt`):

```
set            = Ok
keys           = ["", "sub", "sub/deeper", "sub/deeper/foo.txt"]
listdir(root)  = ["sub"]        listdir(sub)   = ["deeper"]    listdir(deep) = ["foo.txt"]
listdir_keys(deep) = ["sub/deeper/foo.txt"]
listdir_keys_deep(root) = ["", "sub", "sub/deeper", "sub/deeper/foo.txt"]
is_dir(sub) = true    is_dir(key) = false    contains(key) = true
get_bytes(key) = "hello"        get_metadata(sub) = Ok        removedir(sub) = Ok
```

Every one of those is correct. **The headline claim is not reproducible on the filesystem backend.**

Two of the four `//TODO: create_dir` markers (`:110`, `:119`) are inside the commented-out
synchronous `OpenDALStore` block and are dead text. The remaining two (`:357`, `:374`) sit above
calls to `make_sub_dirs`, which does create the directories — the markers are stale.

The FIXME is stale too. Temporarily re-enabling the line it guards —
`metadata.children = self.listdir_asset_info(key).await.unwrap_or_default()` — produced correct
children at every level:

```
get_metadata("")           children = [("sub", dir), ("top.txt", file)]
get_metadata("sub")        children = [("deeper", dir), ("a.txt", file)]
get_metadata("sub/deeper") children = [("b.txt", file)]
```

What is still true is the comment's *second* sentence: `get_metadata` on a directory calls
`listdir_asset_info`, which calls `get_asset_info` per child, which calls `get_metadata` per child
directory — a full recursive walk of the subtree for one directory read.

## What is actually wrong

Three defects, none of which is "slashes do not work":

1. **`key_prefix()` returns the wrong value.** `AsyncOpenDALStore` stores `prefix: Key`
   (`opendal_store.rs:221-224`) and `is_supported` uses it (`:514-520`), but
   `fn key_prefix(&self) -> Key { Key::new() }` (`:296`) ignores it. `AsyncFileStore` and
   `FileStore` return `self.prefix` (`store.rs:1035`, `:1323`). Consequences: `AsyncStore::keys`
   enumerates from the backend root rather than the prefix (`store.rs:457` uses
   `self.key_prefix()`); `AsyncStoreRouter` routes and lists by `store.key_prefix()`
   (`store.rs:1711`, `:1843`, `:1846`); `store_name()` prints an empty prefix. Every OpenDAL store
   configured with a `prefix` through `StoreRouterConfig` (`store_builder.rs:200`) is affected.

2. **Directory keys are invisible on backends without directory objects.** `make_sub_dirs`
   (`opendal_store.rs:277`) discards `create_dir` failures (`let _ignore`), and `is_dir` then asks
   the backend to `stat` the path. On the memory backend — and on object stores generally, which is
   most of `OPENDAL_STORE_TYPES` (`config.rs:275`: `s3`, `gcs`, `azblob`, `redis`, the SQL
   backends, …) — that path does not exist:

   ```
   set("sub/foo.txt")  = Ok        get_bytes("sub/foo.txt") = "hello"
   listdir("sub")      = ["foo.txt"]        listdir_keys("sub") = ["sub/foo.txt"]
   is_dir("sub")       = Err(KeyReadError: NotFound … memory doesn't have this path)
   contains("sub")     = false
   get_metadata("sub") = Err(KeyNotFound)   get_asset_info("sub") = Err(KeyNotFound)
   ```

   So the *listing* sees the directory and the *addressing* does not. `test_opendal_subdir`
   (`opendal_store.rs:661`) documents this with commented-out assertions and the note "memory
   backend does not support directories explicitly, so not everything works as it should".
   `AsyncMemoryStore` in `liquers-core` shows the intended semantics: it synthesizes `is_dir` from
   the stored keys (`store.rs:1619-1633`) and `contains` falls back to `is_dir` (`store.rs:1611-1618`).

3. **Path mapping is spread out and has no round-trip guarantee.** `key_to_path` (`:238`),
   `key_to_path_metadata` (`:248`), `path_to_key` (`:241`), plus ad-hoc trailing-slash handling in
   `listdir` (`:445`) and `makedir` (`:498`). `path_to_key` is lossy — it applies
   `trim_matches('/')` then `trim_end_matches(METADATA)` — and nothing asserts
   `path_to_key(key_to_path(k)) == k`. The issue's own **Expected behaviour** asks for exactly
   this: *"Path normalization is applied in one place, with a round-trip property test."*

## Expected behaviour and acceptance criteria

1. One place maps a `Key` to a backend path and back, with a property test over a generated corpus
   of keys — including multi-segment keys, keys with dots, unicode names, and names ending in the
   metadata suffix — asserting `path_to_key(key_to_path(k)) == k` and that a metadata path decodes
   to the data key.
2. `AsyncOpenDALStore::key_prefix()` returns the configured prefix, matching `AsyncFileStore`, with
   a test that a prefixed store enumerates and routes only within its prefix.
3. A directory key whose children exist is addressable on a backend with no directory objects:
   `is_dir`, `contains`, `get_metadata` and `get_asset_info` agree with `listdir`. Verified on the
   memory backend, so the assertions in `test_opendal_subdir` can be uncommented rather than
   apologised for.
4. The stale `FIXME` and the two live `//TODO: create_dir` markers are removed or replaced with
   what is actually true. The two dead ones in the commented-out sync block go with it or stay
   untouched — a decision, not an oversight.
5. No behaviour change on the filesystem backend, which already works: the reproduction above is
   turned into a regression test.

## Affected users, workflows and systems

`store/backends` only. Reached by: `AsyncStoreRouter` (routing and `listdir` across stores),
`-R/` and `-R-dir/` queries against any OpenDAL-backed store, `liquers-axum`'s store endpoints, and
`liquers-lib/examples/ui_query_console_app.rs:92`. `liquers-web` depends on `liquers-store` with
the `opendal` feature **off** (`liquers-store/Cargo.toml`), so the browser build is not affected.
Query, Commands and Assets are untouched.

## Scope and non-goals

In scope: the three defects above, in `liquers-store/src/opendal_store.rs` and its tests.

Not in scope:

- making `get_metadata` on a directory populate `children` (the expensive recursive walk) — the
  FIXME is deleted, not honoured; whether directory metadata should carry children at all is a
  separate question about `AsyncStore`'s default (`store.rs:399-403`);
- `OPENDAL-LOCALFS-TEST-SILENT-ON-WRONG-VALUE-TYPE` (P3), an unrelated weakness in
  `test_opendal_localfs`;
- `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` (the key-absoluteness rule is already enforced here, by
  `key_to_path`);
- adding a `path_map.rs` *module* if the mapping fits in one impl block — WP-5 proposed the file,
  but the deliverable is "one place", not "one file".

## Compatibility constraints

Defect 1's fix **changes routing behaviour**: an `AsyncStoreRouter` that today sends every key to a
prefixed OpenDAL store (because its `key_prefix()` claims the root) will stop doing so. That is the
correction, but it can change which store a key resolves to in an existing multi-store
configuration. This is the one part of the change that is not obviously safe, and it needs to be
called out rather than slipped in.

Defect 3's fix must not change the on-disk layout: paths written today must still be read.

## Known questions and assumptions

- **Q1** — is the directory-key gap (defect 2) in scope for this issue, or a separate one? It is
  the closest thing to the issue's headline claim, and it is where the real design choice is.
- **Q2** — the routing behaviour change in defect 1. Fix, or fix and document, or split out?
- Assumption: `AsyncMemoryStore`'s synthesize-from-keys semantics is the intended contract for
  `is_dir`/`contains`, not an accident. `AsyncFileStore` asks the filesystem, so the two built-in
  stores already differ; the contract is not written down anywhere.

## Documentation assessment

`specs/README.md` §Stores currently repeats the disproven claim — *"`STORE-OPENDAL-SLASH-HANDLING`
is P1 and blunt: keys containing `/` are not reliably addressable through an OpenDAL backend, which
is most real keys"* — and links the issue as `planned`. Small in-scope maintenance: correct that
sentence and move the link to this design at stage `designing`.

Potentially substantive, for Phase 5: `AsyncStore`'s directory contract (what `is_dir` and
`contains` mean when the backend has no directories) is undocumented, and Q1 turns on it.
`specs/reference/STORE_CONFIG_FSD.md` describes configuration, not semantics. Writing that contract
down would be a new section in a reference document — a Phase 5 proposal, not in-scope work.
