---
id: WEB-NATIVE-IO-TIER2
kind: issue
title: No browser-native store or command backend
status: draft
priority: P3
complexity: L
area: [web]
design: liquers-web-store
created: 2026-08-08
github:
---
## Problem

The conditional-`Send` groundwork from `async-wasm-refactor` permits a `BrowserEnvironment` with an
IndexedDB/`fetch` `AsyncStore` and a JavaScript-closure command backend — the core already does not
preclude `!Send` closures. None of it is implemented.

## Impact

A browser deployment cannot persist to browser-native storage or fetch over the network through the
store abstraction; it is limited to what the host page passes in.

## Expected behaviour

A tier-2 `BrowserEnvironment` providing an IndexedDB-backed `AsyncStore` and a `fetch`-backed
read-only store, plus a command backend accepting JavaScript closures.

## Partially delivered, 2026-08-09

`specs/design/liquers-web-store/` shipped the store half in a different shape than this issue
anticipated, and the issue stays **open** for what it did not cover.

Delivered: a `localStorage`-backed `AsyncStore` (full contract, directories included), a
`fetch`-backed read-only store, a `JsStore` adapting a page object, and declarative composition
through `liquers-store`'s configuration and a new `StoreFactory` seam.

Not delivered, and still what this issue asks for:

- **IndexedDB.** `localStorage` is synchronous, string-only and capped at a few megabytes; anything
  larger than a few documents needs IndexedDB, which is Promise-based and a genuinely different
  store.
- **A command backend accepting JavaScript closures.** Delivered separately by
  `specs/design/liquers-web/` (`registerCommand`), so this half is arguably done — confirm before
  closing.
- **A `BrowserEnvironment` type.** Not built and, as it turned out, not needed:
  `DefaultEnvironment` was already sufficient once a store could be configured on it.

One thing to know before building on this: `-R/` queries still do not evaluate in a browser, for a
reason unrelated to stores — `CORE-IMMEDIATE-MANAGER-KEYED-RECURSION` (P1).

## Discovery

Migration triage, 2026-08-08. Source: the *async-wasm-refactor follow-ups* section of `specs/archive/2026-08-08-issues.md`. Verified against HEAD — see
`specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.
