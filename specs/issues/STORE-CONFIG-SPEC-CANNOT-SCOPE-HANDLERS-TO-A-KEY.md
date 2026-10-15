---
id: STORE-CONFIG-SPEC-CANNOT-SCOPE-HANDLERS-TO-A-KEY
kind: feature
title: A store_config spec cannot limit its handlers to one key, folder or pattern
status: draft
priority: P3
complexity: M
area: [core/store, store/config, core/plan]
design:
created: 2026-10-15
github:
---
## Problem

A mount's `metadata` handler chains apply to every key of the mount. The natural syntax for scoping
them, a resource segment such as `-R-key/docs/notes/-store_config/front_matter-title`, parses, but
the `store_config` compiler refuses `GetResource`, `GetResourceMetadata`, `GetResourceDirectory`
and `UseKeyValue` steps with a configuration error (`liquers-core/src/store/realm.rs`,
`StoreConfigFunctions::compile`). `design/store-adapters/phase2-architecture.md` §4 analysed the
form and deferred it.

## Impact

A mount mixing document types cannot run `front_matter` only on Markdown files; the workaround is
a separate mount per kind of content, or a handler that ignores keys it does not recognise.

## Expected behaviour

A resource segment compiles into a guard over `FactsIn.key` (already carried for this purpose).
Open points from the analysis: exact key or prefix match; how several guarded chains combine
(first match or all in order); whether a pattern (`*.md`) is needed rather than a key.

## Discovery

Deferred by `design/store-adapters/` (Phase 2, "`-R-key/…` — analysed, not supported now").
