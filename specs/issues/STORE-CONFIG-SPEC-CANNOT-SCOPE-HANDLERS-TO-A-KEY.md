---
id: STORE-CONFIG-SPEC-CANNOT-SCOPE-HANDLERS-TO-A-KEY
kind: feature
title: A store_config spec cannot limit its handlers to one key, folder or pattern
status: draft
priority: P3
complexity: M
area: [core/store, store/config, core/plan]
design: store-config-key-scoped-handlers
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

## Problem Example

```yaml
- type: filesystem
  prefix: kb
  config: { path: ./kb }
  metadata:
    on_validate: -R-key/kb/notes/-/require-title
    on_read: media_type-fill/file_facts/-R-key/kb/notes/-/front_matter
```

Today: the build fails — `StoreConfigFunctions::compile_plan` (`liquers-core/src/store/realm.rs`)
refuses the `UseKeyValue` step: "Store configuration 'on_validate: -R-key/kb/notes/-store_config/
require-title': contains a resource segment, which is not supported in store configuration yet".
Should be: the store builds; `set` of `kb/notes/a.md` without a title is refused, `set` of
`kb/data/x.csv` without one succeeds, and only files under `kb/notes` get `front_matter`.
(The issue's own example, `front_matter-title`, does not plan: `front_matter`'s first parameter is
`mode`; `front_matter-overwrite-title` is the spelling.)

## Impact

A mount mixing document types cannot run `front_matter` only on Markdown files; the workaround is
a separate mount per kind of content, or a handler that ignores keys it does not recognise.

## Expected behaviour

A resource segment compiles into a guard over `FactsIn.key` (already carried for this purpose).
Open points from the analysis: exact key or prefix match; how several guarded chains combine
(first match or all in order); whether a pattern (`*.md`) is needed rather than a key.

## Discovery

Deferred by `design/store-adapters/` (Phase 2, "`-R-key/…` — analysed, not supported now").

## Note (2026-10-21)

The example spelling `front_matter-title` above does not plan: `front_matter`'s first parameter is
`mode`, so the scoped form is `front_matter-overwrite-title`. Designed in
`design/store-config-key-scoped-handlers/`.
