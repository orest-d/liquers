---
id: WEB-NATIVE-IO-TIER2
kind: issue
title: No browser-native store or command backend
status: draft
priority: P3
complexity: L
area: [web]
design: 
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

## Discovery

Migration triage, 2026-08-08. Source: the *async-wasm-refactor follow-ups* section of `specs/archive/2026-08-08-issues.md`. Verified against HEAD — see
`specs/DOCS_MIGRATION_PLAN.md` §4.0c.
