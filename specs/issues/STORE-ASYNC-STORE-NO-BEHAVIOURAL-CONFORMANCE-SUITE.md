---
id: STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE
kind: issue
title: AsyncStore has no written behavioural contract and no suite holding implementations to one
status: accepted
priority: P1
complexity: L
area: [core/store, store/backends, web, docs]
design:
created: 2026-09-02
github:
---
## Problem

`AsyncStore` has five in-tree implementations — `AsyncMemoryStore`, `AsyncFileStore`,
`AsyncStoreRouter` (`liquers-core/src/store.rs`), `AsyncOpenDALStore`
(`liquers-store/src/opendal_store.rs`), and `liquers-web`'s `FetchStore`, `LocalStorageStore` and
`JsStore` — plus whatever a language integration supplies. Each is tested only against itself. The
one cross-implementation suite, the `keyabs` family, checks a single rule: that every store refuses
a relative key.

Nothing checks that the implementations **agree**, and the trait's doc comments are the whole
specification. They do not agree. Enumerated at `HEAD` on 2026-09-02, with evidence:

| # | Question | Divergence | Tracked as |
|---|---|---|---|
| 1 | `is_dir` on an absent key | `Ok(false)` in the trait default (`:448`), `AsyncFileStore` (`:1199`) and `AsyncMemoryStore` (`:822`); **`Err`** in `AsyncOpenDALStore` (`:427`) | `STORE-OPENDAL-SLASH-HANDLING` defect 4 |
| 2 | Is a directory key with children addressable at all? | yes in every store with an index or a real filesystem; **no** in `AsyncOpenDALStore` on a backend with no directory objects — `listdir` sees it and `is_dir`/`contains`/`get_metadata` deny it | `STORE-OPENDAL-SLASH-HANDLING` defect 4 |
| 3 | Does `contains` fall back to `is_dir`? | yes in `AsyncMemoryStore` (`:810`) and `LocalStorageStore`; effectively yes in `AsyncFileStore` (a directory path exists); **no** in the trait default (`:442`) and in `AsyncOpenDALStore` | `CORE-DIRECTORY-INDEX-NOT-SHARED` |
| 4 | Is `removedir` scoped to the directory or to the path prefix? | directory in `AsyncMemoryStore` (`:790`) and `AsyncFileStore` (`:1171`); **path prefix** in `AsyncOpenDALStore` (`:408`) — so `removedir("data")` destroys `database/` | `STORE-OPENDAL-SLASH-HANDLING` defect 1 (P0, data loss) |
| 5 | Is `removedir` recursive? | **the doc comment says no; all three async implementations say yes** | this issue |
| 6 | `removedir` on a directory that does not exist | `Ok(())` in `AsyncFileStore` and `AsyncOpenDALStore`; **`Err(key_not_supported)`** in the trait default (`:436`) | this issue |
| 7 | Does `makedir` create anything? | `create_dir_all` in `AsyncFileStore` (`:1244`); **a silent no-op** in `AsyncMemoryStore` (`:888`); `Err(key_not_supported)` in the trait default (`:518`) | `CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING` (P0) |
| 8 | Does `is_supported` consult the store's prefix? | yes in `AsyncFileStore` (`:1252`), `FileStore` (`:1490`), `AsyncOpenDALStore` (`:514`); **no** in `AsyncMemoryStore` (`:893`), with a code comment recording the omission | `CORE-ASYNC-MEMORY-STORE-IS-SUPPORTED-IGNORES-PREFIX` (P1) |
| 9 | Does `key_prefix()` report the configured prefix? | yes everywhere except `AsyncOpenDALStore` (`:296`), which returns `Key::new()` | `STORE-OPENDAL-SLASH-HANDLING` defect 3 |
| 10 | What does `keys()` return? | data keys only in `AsyncMemoryStore` (`:831`); data keys **plus directories plus the root** in the trait default (`:454`), `AsyncFileStore` and `AsyncOpenDALStore` | `CORE-STORE-KEYS-MEANS-TWO-DIFFERENT-THINGS` (P2) |
| 11 | How is directory structure derived on a flat backend? | four private mechanisms, no two alike, and one store with none | `CORE-DIRECTORY-INDEX-NOT-SHARED` (P1) |

Rows 5 and 6 have no other home and are this issue's own; every other row is tracked by a named
issue, which is the point: **each was found separately, by someone tripping over it, rather than by
anything checking.**

## Impact

`AsyncStoreRouter` mixes implementations in one namespace, so a single deployment answers the same
question two ways depending on which store a key lands in. Every new `AsyncStore` — including one
written outside this repository through a language integration — is a fresh opportunity to pick
differently, and nothing will say so.

The cost is measurable rather than hypothetical: of the eleven rows above, one is a P0 data-loss bug
that survived in `main` long enough to be filed, disproven, and re-found; two more are P0 or P1 bugs
found only because a design happened to look. A suite run against every implementation would have
caught rows 1, 2, 3, 4, 7, 8, 9 and 10 at the commit that introduced them.

## Expected behaviour

Two deliverables, in this order — the specification first, because a suite written without one just
freezes whichever store was consulted:

1. **A written contract** in `specs/reference/`, answering every row above and the questions they
   imply: the three sources of directory truth (`stat`, a bounded listing, an index) and which
   backend shape uses which; that no operation on a key may reach a sibling key; that an explicitly
   created empty directory is distinct from a derived one; what `keys()` enumerates; whether
   `is_supported` is about the prefix, the key's shape, or both; the atomicity guarantees
   (`removedir` is not atomic on any backend).
   `design/opendal-path-mapping/` creates `specs/reference/STORE_SEMANTICS.md` at its Phase 5,
   covering the rows it touches; this issue owns completing it.

2. **A shared, parameterized suite** applied to every implementation, in the shape the `keyabs`
   family already uses for the absoluteness rule. Sibling safety belongs in it: for a store holding
   both `sub/` and `subway/`, no operation on `sub` may read, list or delete anything under
   `subway/`. So does the `is_dir`/`contains`/`listdir` agreement, and the `keys()` decision from
   row 10.

The `liquers-web` stores make this an `L`: the suite has to run under `wasm32` as well as natively,
so it cannot simply be a `#[tokio::test]` module.

## Discovery

Opened on 2026-09-02 while designing `STORE-OPENDAL-SLASH-HANDLING` in
[`design/opendal-path-mapping/`](../design/opendal-path-mapping/), whose Phase 2 rejects building
the suite inside a P0 correctness fix. Expanded the same day, at the Phase 4 gate, into the full
enumeration above after the reviewer asked that disagreeing store contract implementations be
recorded rather than mentioned.
