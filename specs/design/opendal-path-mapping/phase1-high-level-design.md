# Phase 1: High-Level Design — OpenDAL path mapping

For [`issues/STORE-OPENDAL-SLASH-HANDLING.md`](../../issues/STORE-OPENDAL-SLASH-HANDLING.md)
(issue, **P0**, complexity M, `status: accepted`).

> **Revision history.** Written 2026-08-29 from a first reproduction, which concluded that the
> issue's headline claim was not reproducible and that the remaining defects were three. A **second
> reproduction on 2026-09-02** found two further defects, one of them destructive, and disproved one
> of the first pass's own claims. Restructured the same day to the `liquers-project` contract when
> the workflow was adopted at the gate. Both reproductions are recorded below, because the
> correction is part of the evidence.

## Feature Name

OpenDAL path mapping — one `Key`-to-backend-path mapping, with a round-trip property and a
directory form.

## Purpose

`AsyncOpenDALStore` builds backend paths in six places and gets the trailing slash right in only
two. Because OpenDAL treats a path without a trailing `/` as a *prefix*, `removedir("sub")` deletes
`subway/` and `listdir_keys_deep("sub")` lists it — data loss and wrong results on every backend.
This work puts the mapping in one place, fixes the five defects that follow from its absence, and
pins the result down with a round-trip property test and a sibling-safety test.

## Core Interactions

### Query System
None. No query, parse or plan behaviour changes. `-R/` and `-R-dir/` queries against an
OpenDAL-backed store return correct results afterwards where they returned sibling data before.

### Store System
The whole change. `AsyncOpenDALStore` (`liquers-store/src/opendal_store.rs`): path mapping,
`key_prefix`, `is_dir`, `contains`, `get_metadata`, `removedir`, `listdir`, `listdir_keys_deep`,
`makedir`. `AsyncStoreRouter` (`liquers-core`) is not edited but changes behaviour, because it
routes and aggregates on `key_prefix()`.

### Command System
None. No command is added, removed or changed; no namespace is touched.

### Asset System
Indirectly: `AsyncStore::get_asset_info` delegates to `get_metadata`, so a directory key that is
unaddressable today becomes addressable as an asset on backends with no directory objects.

### Value Types
None. No `ExtValue` variant, no `TypeInfo`.

### Web/API
No route changes, but `liquers-axum`'s `DELETE /api/store/removedir/{*key}` stops deleting sibling
directories, and its store listing endpoints stop reporting keys from them.

### UI
None. `liquers-web` no longer depends on `liquers-store` at all
(see [`design/store-factories-in-core/`](../store-factories-in-core/)).

## Crate Placement

**`liquers-store`** — `src/opendal_store.rs` for the implementation and its colocated tests, plus
one `#[cfg]` line in `src/store_factory.rs`. This is where the OpenDAL backend lives, and the
dependency flow is respected: nothing moves toward `liquers-core`, which is read but not edited.

The mapping stays a private type inside `opendal_store.rs` rather than a new `path_map.rs` module:
the deliverable the issue asks for is "one place", not "one file", and a module would add public
surface for a single caller.

## Documentation Intent

**Reference:** *Extend* `specs/reference/STORE_CONFIG_FSD.md` — no. That document specifies
configuration, and this is semantics. **Create** a new reference section documenting `AsyncStore`'s
directory and deletion contract: what `is_dir`, `contains` and `removedir` mean when the backend has
no directory objects, and the rule that no operation on a key may reach a sibling key. Four of the
six defects are divergences from an unwritten rule, so writing the rule down is the durable fix.
Exact path decided in Phase 2 (a new `specs/reference/STORE_SEMANTICS.md`, or a section in an
existing store reference).

**Guide:** Neither. There is no repeatable task a developer performs here; nobody "uses" a path
mapping. Reconsider if Phase 3 finds that configuring a *prefixed* store needs explaining — the
prefix convention (the prefix is part of the path under the backend root) is currently folklore.

**Other documents to create:** `specs/issues/STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE.md`
— filed 2026-09-02, since the suite itself is out of scope (see Non-goals).

**Specific documents to update:** `specs/README.md` §Stores (done, 2026-09-02: the disproven
statement is corrected and the P0 raise reflected); the issue file
`STORE-OPENDAL-SLASH-HANDLING.md` (evidence update done; `status` at Phase 5); the two folded-in
issue files, closed at Phase 5; `specs/index.csv`, regenerated.

**Audience and outcome:** a future maintainer adding a fifth `AsyncStore` implementation should be
able to read the contract and satisfy it without reading this design folder or reverse-engineering
`AsyncMemoryStore`.

## Open Questions

1. **Q1 — is the directory-key gap (defect 4) in scope, or a separate issue?**
   **Answered at the 2026-09-02 gate: in scope.**
2. **Q2 — the `key_prefix()` fix changes router behaviour. Fix here or split out?**
   **Answered: fix here, in its own commit, with a router test.**
3. **Q3 — delete the commented-out synchronous `OpenDALStore` block?**
   **Answered: delete it in this change.** It is 200 lines that cannot compile and it holds two of
   the four `//TODO: create_dir` markers the issue cites, so leaving it would close the issue with
   two citations untouched.
4. **Q4 — is the P1 → P0 raise right?** **Answered: keep P0.**
5. Still open, for Phase 2 to settle: the exact path and shape of the new reference section
   (question 1 under Documentation Intent).
6. Still open, for Phase 3: whether the sibling-safety property can be asserted against a *remote*
   object store offline, or whether `memory` and `fs` are the whole verifiable surface.

## References

- [`issues/STORE-OPENDAL-SLASH-HANDLING.md`](../../issues/STORE-OPENDAL-SLASH-HANDLING.md)
- [`issues/STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE.md`](../../issues/STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE.md)
- [`design/store-factories-in-core/`](../store-factories-in-core/) — merged; `store_builder.rs` is
  gone and every reference here is re-resolved to `store_factory.rs`
- [`archive/2026-08-08-docs-migration-plan.md`](../../archive/2026-08-08-docs-migration-plan.md)
  §4.0c — where the issue came from (work package WP-5)
- OpenDAL 0.55 `Operator::create_dir` (`types/operator/operator.rs:457`) — the trailing-slash rule

---

# Appendix A — Reproduction evidence

## The problem as filed

*"Keys that contain a `/` — which is to say most real keys — are not reliably addressable through
an OpenDAL-backed store."* The issue cites the
`// FIXME: This currently does not work due to some bug with handling '/'` at
`liquers-store/src/opendal_store.rs:340` and four `//TODO: create_dir` markers.

## First reproduction, 2026-08-29 — the headline is not the whole story

Scratch integration test over `AsyncOpenDALStore`, deleted afterwards; raw output.
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
`"subway/…"` too. Verified directly against the operator, on both backends:

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

---

# Appendix B — What is actually wrong

Six defects. The first is data loss and is why the issue is P0.

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

6. **`make_sub_dirs` is a no-op, and two markers claim otherwise.** Evidence in Appendix A. Two
   further defects sit in the same lines: `prefix_of_size(i).unwrap()` at `:279` and `:488` is
   `unwrap()` in library code, which `CLAUDE.md` forbids; and the file emits two compiler warnings
   at `HEAD` (unused `Store` import `:8`, unnecessary `mut` `:339`).

---

# Appendix C — Acceptance criteria, scope and constraints

## Acceptance criteria

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
   true; `make_sub_dirs` is deleted, not left dead behind a comment claiming otherwise; the
   commented-out synchronous block goes with them (Q3). No `unwrap()` remains in this file outside
   tests, and the file compiles without warnings.
6. No behaviour change to the filesystem paths that already work: the first reproduction becomes a
   regression test.

## Affected users, workflows and systems

`store/backends`. Reached by: `AsyncStoreRouter` (routing, `listdir` and `is_dir` across stores),
`-R/` and `-R-dir/` queries against any OpenDAL-backed store, `liquers-axum`'s store endpoints
(including the destructive `removedir` route), and `liquers-lib/examples/ui_query_console_app.rs`.
Query, Commands and Assets are untouched.

## Non-goals

- making `get_metadata` on a directory populate `children` (the expensive recursive walk) — the
  FIXME is deleted, not honoured; whether directory metadata should carry children at all is a
  separate question about `AsyncStore`'s default (`store.rs:396-403`);
- `STORE-OPENDAL-LIST-OPTION-MISPARSED` (P2) — `store_factory.rs`, and it has its own design
  folder ([`design/opendal-list-option-config/`](../opendal-list-option-config/));
- `STORE-OPENDAL-ARGUMENTS-NOT-DERIVED` (P3) — belongs to `store-factories-in-core`;
- `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` — the key-absoluteness rule is already enforced here, by
  `key_to_path`;
- building the shared `AsyncStore` behavioural conformance suite. Every defect here is a divergence
  from what the two `liquers-core` stores already do, and one suite run against all four
  implementations would have caught four of the six. That is an `L`-complexity change to
  `liquers-core` and is filed as
  `STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE` rather than folded into a P0 fix. Writing
  the *contract* it would encode is in scope, under Documentation Intent.

Folded in, both because they live in the same file and the same test module is being rewritten:
`OPENDAL-LOCALFS-TEST-SILENT-ON-WRONG-VALUE-TYPE` (P3, S) and
`STORE-OPENDAL-WITHOUT-ASYNC-STORE-BROKEN` (P3, S).

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
- Assumption: synthesizing directory existence from what is stored is the intended contract for
  `is_dir`/`contains`, not an accident. Both `liquers-core` memory stores do it; `AsyncFileStore`
  asks the filesystem. The contract is not written down anywhere — hence the reference section
  under Documentation Intent.
