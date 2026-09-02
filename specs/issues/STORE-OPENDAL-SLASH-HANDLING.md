---
id: STORE-OPENDAL-SLASH-HANDLING
kind: issue
title: OpenDAL store mishandles keys containing slashes
status: closed
priority: P0
complexity: M
area: [store/backends]
design: opendal-path-mapping
created: 2026-08-08
github:
---
## Problem

`liquers-store/src/opendal_store.rs:335` carries
`// FIXME: This currently does not work due to some bug with handling '/'`. Directory creation is
also stubbed at `:110`, `:119`, `:357` and `:374` (`//TODO: create_dir`).

## Impact

Keys that contain a `/` — which is to say most real keys — are not reliably addressable through an
OpenDAL-backed store. This is a correctness bug against real backends, not a limitation.

## Expected behaviour

Path normalization is applied in one place, with a round-trip property test: any `Key` encoded to a
backend path and decoded returns the original. WP-5 proposes a dedicated `path_map.rs` and strict
rewrites in the store tests.

## Discovery

Migration triage, 2026-08-08. Source: `todo20260219.md` #6, work package WP-5. Verified against HEAD: the FIXME is still at `opendal_store.rs:335`. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.

## Update, 2026-08-29 — reproduction narrows the problem

Reproduced at HEAD while preparing [`design/opendal-path-mapping/`](../design/opendal-path-mapping/).
The headline claim above does **not** hold on the filesystem backend: `sub/deeper/foo.txt` is
correct through `set`, `get_bytes`, `contains`, `is_dir`, `listdir`, `listdir_keys`,
`listdir_keys_deep`, `keys` and `removedir`. The `FIXME` at `opendal_store.rs:335` is stale — the
line it guards produces correct children when re-enabled — and the two live `//TODO: create_dir`
markers are satisfied by `make_sub_dirs`.

What reproduction did find is three defects the headline hides: `key_prefix()` returns `Key::new()`
instead of the configured prefix; directory keys are unaddressable on backends with no directory
objects (memory, and object stores generally) even though `listdir` sees them; and the path mapping
is spread across four methods with no round-trip guarantee. The design folder's Phase 1 records the
evidence and Phase 2 the proposed fix, both awaiting approval. Status left `accepted`: the issue is
real, its statement is being corrected rather than withdrawn.

## Update, 2026-09-02 — the headline is true after all, and one defect is data loss

A second reproduction, probing two sibling directories whose names share a prefix (`sub/` and
`subway/`) rather than one key in isolation, found two defects the 2026-08-29 pass missed. Both are
genuine slash handling, which is what the issue said in the first place:

1. **`removedir` deletes sibling directories.** `opendal_store.rs:408` calls
   `op.remove_all(key_to_path(key))` with no trailing slash, which OpenDAL treats as a *prefix*
   delete. Reproduced on the filesystem backend: `removedir("sub")` destroyed `subway/`. Reachable
   remotely — `liquers-axum` serves `DELETE /api/store/removedir/{*key}`. Both other async stores
   scope the delete correctly, so this is a divergence from an established contract.
2. **`listdir_keys_deep` returns keys from sibling directories** (`:481`), same missing slash on
   `list_with(path).recursive(true)`. It propagates through `keys()` and the store router.

Verified fix in both cases: pass `"sub/"`. With the slash, a recursive list of `sub/` returns only
`sub/…`, and `remove_all("sub/")` leaves `subway/b.txt` in place, on memory and filesystem alike.

**Priority raised P1 -> P0** on the first of these: data loss, per `DOCS_STRUCTURE_GUIDE.md` §4.4.

The same pass also **disproved the 2026-08-29 update's claim** that the two live `//TODO: create_dir`
markers are satisfied by `make_sub_dirs`. They are not: `make_sub_dirs` calls `create_dir` without a
trailing slash, which OpenDAL rejects unconditionally, and the error is discarded by `let _ignore`.
The function has never created a directory on any backend; nested writes work on the filesystem
because OpenDAL's `Fs` service creates parents on write.

Six defects are now in scope, recorded with evidence in
[`design/opendal-path-mapping/`](../design/opendal-path-mapping/) Phase 1, with the solution in
Phase 2. Two adjacent P3 issues in the same file are folded into the same change:
`OPENDAL-LOCALFS-TEST-SILENT-ON-WRONG-VALUE-TYPE` and `STORE-OPENDAL-WITHOUT-ASYNC-STORE-BROKEN`.

## Resolution, 2026-09-02

Fixed in [`design/opendal-path-mapping/`](../design/opendal-path-mapping/), which corrected this
issue's statement twice before fixing it. All six defects are resolved:

1. **`removedir` deleted sibling directories** (data loss, the reason for the P0 raise) — every
   call site that names a directory now goes through `PathMap::directory`, which supplies the
   trailing `/` OpenDAL requires. `sibling01`, `sibling02`.
2. **`listdir_keys_deep` leaked sibling keys** — same fix. `sibling03`.
3. **`key_prefix()` returned the root key** — returns the configured prefix, matching the file
   stores. `prefix01`, `router01`, and the assertion `store_factory.rs`'s `opendal03` could not
   previously make.
4. **Directory keys were unaddressable on backends with no directory objects** — `is_dir` falls
   back to a bounded listing, `contains` and `get_metadata` follow, and `AsyncStore` now carries the
   shared semantics (`CORE-DIRECTORY-INDEX-NOT-SHARED`). `dir01`-`dir04`, and
   `test_opendal_subdir`'s commented-out assertions are live.
5. **Path mapping was spread across four methods with no round-trip guarantee** — one `PathMap`
   type with data, metadata and directory forms and a `DecodedPath` decoder, with a round-trip
   corpus. `pathmap01`-`pathmap06`.
6. **`make_sub_dirs` was a no-op behind two `//TODO: create_dir` markers** — deleted. It called
   `create_dir` without a trailing slash, which OpenDAL rejects unconditionally, and discarded the
   error. All four of the issue's `//TODO` citations are gone: the other two were inside the
   commented-out synchronous store, deleted with it.

The behavioural contract these defects violated is written down in
[`reference/STORE_SEMANTICS.md`](../reference/STORE_SEMANTICS.md).
