---
id: CORE-SYNC-STORE-TRAIT-OBSOLETE
kind: issue
title: The synchronous Store trait is obsolete and should be removed
status: draft
priority: P2
complexity: M
area: [core/store, py, docs]
design:
created: 2026-09-02
github:
---
## Problem

`liquers-core::store::Store` is the synchronous sibling of `AsyncStore`. It is obsolete:

- **No `Environment` can hold one.** The `Environment` trait exposes `get_async_store` and nothing
  else; `SimpleEnvironment` stores an `Arc<dyn AsyncStore>`. A synchronous store cannot reach the
  interpreter, the asset manager, or a query.
- **Its bridge to the async world has already been deleted.** `AsyncStoreWrapper` — the adapter
  that made a `Store` usable — no longer exists anywhere in the tree. Nothing converts a `Store`
  into anything the system will accept, which is what makes the remaining implementations
  unreachable rather than merely unused. (Three documents still teach it:
  `DOCS-ASYNC-STORE-WRAPPER-NO-LONGER-EXISTS`.)
- **It carries its own copy of the contract, and drifts.** `Store` and `AsyncStore` declare the
  same ~20 methods with the same doc comments and independently maintained defaults. Every rule in
  `specs/reference/STORE_SEMANTICS.md` would have to be stated and enforced twice for a trait
  nothing can use.

What remains: the trait (`store.rs:66`), `NoStore` (`:566`), `FileStore` (`:1241`), `MemoryStore`
(`:1453`) and `StoreRouter` (`:1655`) in `liquers-core`, and in `liquers-py` a `Arc<dyn Store>`
field on its own `Context`, a `with_store` setter, and the 192-line `PyStore` wrapper. Roughly 800
lines in core plus a `liquers-py` API change.

## Impact

Dead weight that reads as live API. A store author following `CLAUDE.md` or `UNITTEST_GUIDE.md`
can implement the wrong trait and discover only at wiring time that nothing accepts it. It also
doubles the surface any store contract work has to cover: `STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE`
scopes itself to `AsyncStore` for exactly this reason.

## Expected behaviour

Remove `Store` and its implementations from `liquers-core`, and the corresponding surface from
`liquers-py`. `liquers-py`'s public API changes, so this is not a silent cleanup — check
`specs/design/python-wrapper/` for what it promises.

**Keep the door open.** A synchronous store may be wanted again, to support *realms* that evaluate
queries synchronously. That is a design, not a restoration: it would need a synchronous evaluation
path, not just the trait back.
`specs/reference/STORE_SEMANTICS.md` therefore states its rules in trait-neutral terms where they
are the same for both, and records that only the asynchronous case must be satisfied today — so a
future synchronous store inherits the contract instead of re-deriving it.

## Discovery

Recorded on 2026-09-02 while scoping `design/store-conformance-suite/`, when deciding whether the
conformance suite should cover the synchronous trait. It should not; the trait should go.
