---
id: STORE-CONFIG-IN-CORE
kind: design
title: Store configuration types in liquers-core
workflow: liquers-project
status: draft
phase: high-level
area: [core/store, store/config, web, docs]
gh_pr: []
issues: [STORE-CONFIG-IN-CORE]
affects_docs: [reference/STORE_CONFIG_FSD.md, reference/api/DOC_01_ARCHITECTURE_REFERENCE.md, guides/LANGUAGE-INTEGRATION_GUIDE.md]
created: 2026-08-27
superseded_by:
---
# Store Configuration Types in `liquers-core` — Design Tracking

**Created:** 2026-08-27

## Phase Status

- [x] Phase 1: High-Level Design (awaiting approval)
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Resolves feature `STORE-CONFIG-IN-CORE` (P0 by maintainer decision, complexity M), one of three
recorded prerequisites for the document-driven setup path in `design/environment-builder`.

**The move is down the dependency chain, not across it.** `StoreRouterConfig` / `StoreConfig` /
`expand_env_vars` are pure serde data and already call into `liquers-core`
(`parse_key`, `Error`), so relocating them to `liquers-core/src/store_config.rs` removes an edge
rather than adding one. Backends, `StoreRouterBuilder`, `StoreFactory` and the OpenDAL type tables
stay in `liquers-store`; core learns the *shape* of a store description without learning that S3
exists.

**Phase 1 correction to the issue.** Its verification item 3 — "`liquers-web` builds without
depending on `liquers-store` for configuration" — is not achievable and is not the benefit.
`liquers-web` also uses `StoreRouterBuilder` and implements `StoreFactory`, both of which stay. The
gain is layering: `liquers-core` can define a type that embeds `StoreRouterConfig`. Phase 2 restates
the verification list.

**No new non-optional core dependency.** `serde`, `serde_derive`, `serde_json` and `serde_yaml` are
already non-optional in `liquers-core`; only `toml` is new and carries across as the same optional
feature `liquers-store` already gates `from_toml` behind.

Open for Phase 2: whether `expand_env_vars`'s bare `std::env::var` belongs in core unqualified on
wasm; the re-export shape and whether it is deprecated; how `liquers-store/toml` forwards; and
whether the §3 `area` vocabulary needs `core/store` widened.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
