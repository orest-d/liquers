---
id: WEB-NATIVE-IO-TIER2
kind: issue
title: Browser storage has no IndexedDB backend
status: accepted
priority: P3
complexity: L
area: [web]
design: liquers-web-store
created: 2026-08-08
github:
---
## Problem

The browser bindings provide `localStorage`, `fetch`, and host-object store adapters, but no
IndexedDB-backed `AsyncStore`. The existing `localStorage` backend is synchronous, string-only, and
subject to small browser quotas, so it is not suitable for larger persisted assets.

## Impact

Browser deployments cannot use the store abstraction for larger, asynchronous, transactional
browser-native persistence. They can still use the delivered `localStorage`, `fetch`, and `JsStore`
backends.

## Expected behaviour

An IndexedDB-backed `AsyncStore` with the same observable store contract as the existing browser
backends, including directory operations and binary round trips.

## Partially delivered, 2026-08-09

`specs/design/liquers-web-store/` shipped the store half in a different shape than this issue
anticipated, and the issue stays **open** for what it did not cover.

Delivered: a `localStorage`-backed `AsyncStore` (full contract, directories included), a
`fetch`-backed read-only store, a `JsStore` adapting a page object, and declarative composition
through `liquers-store`'s configuration and a new `StoreFactory` seam.

Not delivered, and now the sole scope of this issue, is **IndexedDB**. It is Promise-based and a
genuinely different store from `localStorage`.

The other originally proposed capabilities no longer belong to this issue: JavaScript closure
commands were delivered by `specs/design/liquers-web/` (`registerCommand`), and a dedicated
`BrowserEnvironment` proved unnecessary because `DefaultEnvironment` can be configured with the
browser store adapters. Browser `-R/` evaluation was subsequently unblocked by the fixes recorded
in `CORE-IMMEDIATE-MANAGER-KEYED-RECURSION` and `IMMEDIATE-MANAGER-NO-FAST-TRACK`.

## Discovery

Migration triage, 2026-08-08. Source: the *async-wasm-refactor follow-ups* section of `specs/archive/2026-08-08-issues.md`. Verified against HEAD — see
`specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.
