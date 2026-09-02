# Phase 1 — High-level design

For [`issues/STORE-OPENDAL-SLASH-HANDLING.md`](../../issues/STORE-OPENDAL-SLASH-HANDLING.md)
(issue, complexity M, `status: accepted`; priority raised to **P0** on 2026-09-02 — see
"Second reproduction" below).

> **Revision history of this document.** Written 2026-08-29 from a first reproduction, which
> concluded that the issue's headline claim was not reproducible and that the remaining defects
> were three. A **second reproduction on 2026-09-02** found two further defects the first pass
> missed, one of them destructive, and disproved one of the first pass's own claims. Both passes
> are recorded below, because the correction is part of the evidence.

## The problem as filed

The issue states: *"Keys that contain a `/` — which is to say most real keys — are not reliably
addressable through an OpenDAL-backed store."* It cites the
`// FIXME: This currently does not work due to some bug with handling '/'` at
`liquers-store/src/opendal_store.rs:340` and four `//TODO: create_dir` markers.

## First reproduction, 2026-08-29 — the headline is not the whole story

Reproduced against `HEAD` with a scratch integration test over `AsyncOpenDALStore` (deleted
afterwards; results below are raw output).

**Filesystem backend** (`opendal::services::Fs`, key `sub/deeper/foo.txt`):

```
set            = Ok
keys           = ["", "sub", "sub/deeper", "sub/deeper/foo.txt"]
listdir(root)  = ["sub"]        listdir(sub)   = ["deeper"]    listdir(deep) = ["foo.txt"]
listdir_keys(deep) = ["sub/deeper/foo.txt"]
is_dir(sub) = true    is_dir(key) = false    contains(key) = true
get_bytes(key) = "hello"        get_metadata(sub) = Ok
```

Every one of those is correct: **an ordinary read/write of a nested key is not broken.**

The FIXME is stale. Temporarily re-enabling the line it guards —
`metadata.children = self.listdir_asset_info(key).await.unwrap_or_default()` — produced correct
children at every level. What is still true is the comment's *second* sentence: `get_metadata` on a
directory calls `listdir_asset_info`, which calls `get_asset_info` per child, which calls
`get_metadata` per child directory — a full recursive walk of the subtree for one directory read.

## Second reproduction, 2026-09-02 — two further defects, one destructive

The first pass probed one key in isolation. The second probed **two sibling directories whose names
share a prefix** (`sub/` and `subway/`), which is where a path-versus-prefix confusion becomes
visible. Raw output, memory and filesystem backends:

```
set("sub/a.txt"), set("subway/b.txt")

MEMORY   listdir(sub)            = ["a.txt"]                       correct
         listdir_keys_deep(sub)  = ["sub/a.txt", "subway/b.txt"]   WRONG — leaks a sibling
FS       listdir_keys_deep(sub)  = ["sub", "sub/a.txt",
                                    "subway", "subway/b.txt"]      WRONG — leaks a sibling
FS       removedir(sub)          = Ok
         subway/ still on disk   = false                           WRONG — deleted a sibling
```

Both come from the same root cause as everything else in this issue: **a directory path needs a
trailing `/` and does not always get one.** `op.list_with(path).recursive(true)` and
`op.remove_all(path)` treat a path with no trailing slash as a *prefix*, so `"sub"` matches
`"subway/…"` too. Verified directly against the operator:

```
list recursive "sub"   = ["sub/a.txt", "sub/deep/c.txt", "subway/b.txt"]
list recursive "sub/"  = ["sub/a.txt", "sub/deep/c.txt"]            <- the fix
remove_all("sub/")     -> subway/b.txt survives = true              <- the fix
```

The first pass also recorded that `make_sub_dirs` satisfies the two live `//TODO: create_dir`
markers. **That is wrong, and this document is the correction.** `make_sub_dirs` (`:277`) calls
`op.create_dir(path)` with no trailing slash, and OpenDAL rejects that unconditionally
(`operator.rs:460`, *"the path trying to create should end with `/`"*). The error is swallowed by
`let _ignore` (`:281`), so the function has never created a directory on any backend:

```
memory create_dir("sub")  = Err(NotADirectory … should end with `/`)
fs     create_dir("sub")  = Err(NotADirectory … should end with `/`)   'sub' on disk = false
fs     create_dir("sub/") = Ok                                          'sub' on disk = true
```

Nested writes work on the filesystem because OpenDAL's `Fs` service creates parent directories on
write, not because `make_sub_dirs` does anything.

## What is actually wrong

Six defects. The first is data loss and is why the issue's priority is raised to P0.

1. **`removedir` deletes sibling directories** (`:408`). `op.remove_all(self.key_to_path(key)?)`
   with no trailing slash is a prefix delete: `removedir("data")` also destroys `database/` and
   `data_archive/`. Reproduced on the filesystem backend above. Reachable remotely —
   `liquers-axum` exposes `DELETE /api/store/removedir/{*key}`, and a GET route for it can be
   opted in (`liquers-axum/src/store/builder.rs:86`, `:98`). Both other async stores scope the
   delete correctly (`AsyncMemoryStore` by `Key` prefix, `store.rs:790`; `AsyncFileStore` by
   `remove_dir_all`, `:1171`), so this is a divergence from an established contract, not an
   undefined area.

2. **`listdir_keys_deep` returns keys from sibling directories** (`:481`). Same missing trailing
   slash, on `list_with(path).recursive(true)`. It propagates: `keys()` (`:434`) is
   `listdir_keys_deep(key_prefix())`, so a prefixed store enumerates its siblings, and
   `AsyncStoreRouter::listdir_keys_deep` aggregates the result.

3. **`key_prefix()` returns the wrong value** (`:296`). `AsyncOpenDALStore` stores `prefix: Key`
   (`:222-223`) and `is_supported` uses it (`:514-520`), but `key_prefix()` returns `Key::new()`.
   `AsyncFileStore` and `FileStore` return `self.prefix` (`store.rs:1022`, `:1310`). Confirmed:
   a store built with `prefix: data` reports `key_prefix() == ""`, names itself
   `" OpenDAL Store"`, and `keys()` includes the root key `""`, which is outside its own prefix.
   Consequences: `AsyncStoreRouter::is_dir` (`store.rs:2053`) consults **only** `key_prefix()`, so
   such a store answers `is_dir` for every key in the router, including keys belonging to a store
   listed after it; `listdir` aggregation (`:2080-2097`) has the same problem. Ordinary `get`/`set`
   routing is unaffected, because `find_store` (`:1921`) also requires `is_supported`.

4. **Directory keys are invisible on backends without directory objects.** `is_dir` (`:427`) asks
   the backend to `stat` the path. On the memory backend — and on object stores generally, which is
   most of the advertised types (`s3`, `gcs`, `azblob`, the SQL backends, …) — that path does not
   exist:

   ```
   set("sub/deeper/foo.txt") = Ok      get_bytes = "hello"       listdir(sub) = ["deeper"]
   is_dir(sub)       = Err(KeyReadError: NotFound … memory doesn't have this path)
   contains(sub)     = false
   get_metadata(sub) = Err(KeyNotFound)
   ```

   The *listing* sees the directory and the *addressing* does not. Note also that `is_dir` returns
   `Err` where every other store returns `Ok(false)` for an absent key (`AsyncFileStore`
   `store.rs:1199`, `AsyncMemoryStore` `:822`, and the trait default `:448`).
   `test_opendal_subdir` (`:663`) documents the gap with commented-out assertions and the note
   "memory backend does not support directories explicitly, so not everything works as it should".

5. **Path mapping is spread out and has no round-trip guarantee.** `key_to_path` (`:238`),
   `key_to_path_metadata` (`:248`), `path_to_key` (`:241`), plus ad-hoc trailing-slash arithmetic
   in `listdir` (`:452`) and `makedir` (`:499`) — and its *absence* in `removedir`,
   `listdir_keys_deep` and `make_sub_dirs`, which is defects 1, 2 and 6. Nothing asserts
   `path_to_key(key_to_path(k)) == k`. The issue's own **Expected behaviour** asks for exactly
   this: *"Path normalization is applied in one place, with a round-trip property test."*

6. **`make_sub_dirs` is a no-op, and two markers claim otherwise.** Evidence above. Two further
   defects sit in the same lines: `prefix_of_size(i).unwrap()` at `:279` and `:488` is `unwrap()`
   in library code, which `CLAUDE.md` forbids; and the file emits two compiler warnings at `HEAD`
   (unused `Store` import `:8`, unnecessary `mut` `:339`).

## Expected behaviour and acceptance criteria

1. **No operation on a key ever reaches a sibling key.** `removedir("sub")` leaves `subway/`
   untouched; `listdir_keys_deep("sub")` and `keys()` return nothing from `subway/`. Asserted on
   both the memory and filesystem backends, which behave differently enough to catch a fix that
   only works on one.
2. One place maps a `Key` to a backend path and back — data, metadata and **directory** forms —
   with a property test over a corpus of keys (multi-segment, dots in names, unicode) asserting
   `path_to_key(key_to_path(k)) == k`, and that a metadata path decodes to its data key. A key
   whose *filename* ends in the metadata suffix cannot round-trip and must not be asked to: its
   data path is byte-identical to another key's metadata path, and `is_supported` already refuses
   it. The corpus covers it by asserting **refusal**, which makes the exclusion explicit rather
   than accidental.
3. `key_prefix()` returns the configured prefix, matching `AsyncFileStore`, with a test that a
   prefixed store enumerates and routes only within its prefix.
4. A directory key whose children exist is addressable on a backend with no directory objects:
   `is_dir`, `contains`, `get_metadata` and `get_asset_info` agree with `listdir`. Verified on the
   memory backend, so `test_opendal_subdir`'s assertions can be uncommented rather than
   apologised for. `is_dir` on an absent key returns `Ok(false)`, as every other store does.
5. Every `//TODO: create_dir` and the stale `FIXME` is resolved by code or replaced with what is
   true; `make_sub_dirs` is fixed or deleted, not left dead behind a comment claiming otherwise.
   No `unwrap()` remains in this file outside tests, and the file compiles without warnings.
6. No behaviour change to the filesystem paths that already work: the first reproduction becomes a
   regression test.

## Affected users, workflows and systems

`store/backends`. Reached by: `AsyncStoreRouter` (routing, `listdir` and `is_dir` across stores),
`-R/` and `-R-dir/` queries against any OpenDAL-backed store, `liquers-axum`'s store endpoints
(including the destructive `removedir` route), and `liquers-lib/examples/ui_query_console_app.rs`.
`liquers-web` no longer depends on `liquers-store` at all (see
[`design/store-factories-in-core/`](../store-factories-in-core/)), so the browser build is
unaffected. Query, Commands and Assets are untouched.

## Scope and non-goals

In scope: the six defects above, in `liquers-store/src/opendal_store.rs` and its colocated tests.

Folded in, both because they live in the same file and the same test module is being rewritten:

- `OPENDAL-LOCALFS-TEST-SILENT-ON-WRONG-VALUE-TYPE` (P3, S) — `test_opendal_localfs` (`:705`)
  `eprintln!`s where it should assert, so it would not catch a regression this change might cause.
- `STORE-OPENDAL-WITHOUT-ASYNC-STORE-BROKEN` (P3, S) — `opendal` without `async_store` does not
  compile, because `store_factory.rs` imports a type gated on the other feature.

Not in scope:

- making `get_metadata` on a directory populate `children` (the expensive recursive walk) — the
  FIXME is deleted, not honoured; whether directory metadata should carry children at all is a
  separate question about `AsyncStore`'s default (`store.rs:396-403`);
- `STORE-OPENDAL-LIST-OPTION-MISPARSED` (P2) — `store_factory.rs`, and it has its own design
  folder ([`design/opendal-list-option-config/`](../opendal-list-option-config/));
- `STORE-OPENDAL-ARGUMENTS-NOT-DERIVED` (P3) — belongs to `store-factories-in-core`;
- `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` — the key-absoluteness rule is already enforced here, by
  `key_to_path`;
- a shared behavioural conformance suite for `AsyncStore` implementations. Every defect here is a
  divergence from what the two `liquers-core` stores already do, and one suite run against all
  three would have caught four of the six. That is an `L`-complexity change to `liquers-core` and
  is filed separately rather than folded in;
- deleting the commented-out synchronous `OpenDALStore` block (`:16-218`, 200 lines, 27% of the
  file). See Q3.

## Compatibility constraints

- **Defect 1's fix changes destructive behaviour, in the safe direction.** A caller that today
  relies on `removedir("sub")` also clearing `subway/` would break. There is no such caller in
  this repository, and the behaviour is not documented anywhere; it is data loss, not a feature.
- **Defect 3's fix changes routing.** An `AsyncStoreRouter` that today lets a prefixed OpenDAL
  store answer `is_dir` and contribute to `listdir` for every key will stop doing so. That is the
  correction, but it can change what a multi-store configuration reports. Nothing in this
  repository configures a prefixed OpenDAL store.
- **The on-disk layout must not change**: paths written today must still be read. This is the
  property the round-trip test pins down.
- `removedir`'s doc comment says *"Files are not removed recursively"*, which is false for all
  three async stores. Correct the comment; do not change the behaviour to match it.

## Known questions and assumptions

- **Q1** — is the directory-key gap (defect 4) in scope, or a separate issue? It is where the real
  design choice is. **Answered at the 2026-09-02 gate: in scope.**
- **Q2** — the routing behaviour change in defect 3: fix here, or split out?
- **Q3** — the commented-out synchronous `OpenDALStore` block: delete as part of this work, or
  leave?
- Assumption: synthesizing directory existence from what is stored is the intended contract for
  `is_dir`/`contains`, not an accident. Both `liquers-core` memory stores do it; `AsyncFileStore`
  asks the filesystem. The contract is not written down anywhere — see the documentation note.

## Documentation assessment

`specs/README.md` §Stores describes this issue and links this design; both need the corrected
statement, since the "not reliably addressable" headline turns out to be **true after all**, for a
different reason than the one the issue gave.

Potentially substantive, for Phase 5: `AsyncStore`'s directory and deletion contract — what
`is_dir`, `contains` and `removedir` mean when the backend has no directories — is undocumented,
and four of the six defects are divergences from an unwritten rule.
`specs/reference/STORE_CONFIG_FSD.md` describes configuration, not semantics. Writing that contract
down is a new section in a reference document, and is now recommended rather than merely proposed.
