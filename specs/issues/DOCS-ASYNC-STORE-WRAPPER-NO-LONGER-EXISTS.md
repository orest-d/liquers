---
id: DOCS-ASYNC-STORE-WRAPPER-NO-LONGER-EXISTS
kind: issue
title: Three documents teach AsyncStoreWrapper, which no longer exists in the code
status: draft
priority: P2
complexity: S
area: [docs, core/store]
design:
created: 2026-09-02
github:
---
## Problem

`AsyncStoreWrapper` — the adapter from the synchronous `Store` to `AsyncStore` — has been removed
from `liquers-core`. `grep -rn AsyncStoreWrapper --include='*.rs'` matches nothing outside
`specs/`. Three current documents still name it:

| Document | What it says |
|---|---|
| `CLAUDE.md` §"Async Patterns" | "Sync wrappers (`AsyncStoreWrapper`) only for Python compatibility" |
| `CLAUDE.md` §Testing | "Memory stores for testing: `MemoryStore::new(&Key::new())`, wrapped via `AsyncStoreWrapper`" |
| `specs/guides/UNITTEST_GUIDE.md` | a worked example importing `AsyncStoreWrapper` and wrapping a `MemoryStore` |
| `specs/reference/STORE_CONFIG_FSD.md:399` | "Currently memory store can be implemented via AsyncStoreWrapper." |

Design folders under `specs/design/` also cite it, but those are historical records and are
correct as of their date; only `CLAUDE.md`, the guide and the reference must be true at `HEAD`.

## Impact

`UNITTEST_GUIDE.md`'s setup snippet does not compile, and `CLAUDE.md` is the first thing a coding
agent reads — both direct a test author to a type that is not there, for a trait
(`CORE-SYNC-STORE-TRAIT-OBSOLETE`) nothing can use. The replacement is `AsyncMemoryStore::new`,
which needs no wrapper.

## Expected behaviour

Point all four passages at `AsyncMemoryStore`. If `CORE-SYNC-STORE-TRAIT-OBSOLETE` is taken first,
this falls out of it; if not, it is a small independent fix and should not wait.

## Discovery

Found on 2026-09-02 while scoping `design/store-conformance-suite/` and checking whether the
synchronous store had a live bridge to the async API. It does not.
