---
id: STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE
kind: issue
title: No shared behavioural conformance suite for AsyncStore implementations
status: draft
priority: P1
complexity: L
area: [core/store, store/backends]
design:
created: 2026-09-02
github:
---
## Problem

`AsyncStore` has four in-tree implementations — `AsyncMemoryStore`, `AsyncFileStore`,
`AsyncStoreRouter` (`liquers-core/src/store.rs`) and `AsyncOpenDALStore`
(`liquers-store/src/opendal_store.rs`) — and each is tested only by tests written against itself.
The one cross-implementation suite that exists, the `keyabs` family, checks a single rule: that
every store refuses a relative key.

Nothing checks that the implementations *agree*. The trait's doc comments are the whole
specification for questions every backend has to answer:

- Is `is_dir(k)` on an absent key `Ok(false)` or an error? (`AsyncFileStore` and `AsyncMemoryStore`
  say `Ok(false)`; `AsyncOpenDALStore` returns `Err`.)
- Does `contains(k)` fall back to `is_dir`? (Two of three do.)
- Is a directory key with children addressable on a backend that has no directory objects?
- Is `removedir` recursive, and is it scoped to the directory or to the path prefix?
  (The doc comment says non-recursive; all three implementations are recursive.)
- Is removing an absent directory an error or a no-op?

## Impact

Four of the six defects found in `STORE-OPENDAL-SLASH-HANDLING` are divergences from what the two
`liquers-core` stores already do, including one that destroyed sibling directories. A single suite
run against every implementation would have caught them at the commit that introduced them, and
would catch the next backend's version of the same mistakes. The gap is what makes each new
`AsyncStore` a fresh opportunity to re-invent semantics.

## Expected behaviour

A shared, parameterized test suite — one set of behavioural assertions applied to every
`AsyncStore` implementation, in the shape the `keyabs` family already uses for the absoluteness
rule — plus a written contract in `specs/reference/` for the questions above, so the suite has a
specification to encode rather than being a description of whichever store was written first.

Sibling safety belongs in it: for stores holding both `sub/` and `subway/`, no operation on `sub`
may read, list or delete anything under `subway/`.

## Discovery

Found on 2026-09-02 while designing `STORE-OPENDAL-SLASH-HANDLING` in
[`design/opendal-path-mapping/`](../design/opendal-path-mapping/), whose Phase 2 rejects building
the suite inside a P0 correctness fix and files it here instead.
