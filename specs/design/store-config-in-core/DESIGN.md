---
id: STORE-CONFIG-IN-CORE
kind: design
title: Store configuration and factories in liquers-core
workflow: liquers-project
status: draft
phase: high-level
area: [core/store, store/config, store/backends, web, docs]
gh_pr: []
issues: [STORE-CONFIG-IN-CORE]
affects_docs: [reference/STORE_CONFIG_FSD.md, reference/api/DOC_01_ARCHITECTURE_REFERENCE.md, guides/LANGUAGE-INTEGRATION_GUIDE.md]
created: 2026-08-27
superseded_by:
---
# Store Configuration and Factories in `liquers-core` — Design Tracking

**Created:** 2026-08-27

## Phase Status

- [x] Phase 1: High-Level Design (awaiting approval)
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Resolves feature `STORE-CONFIG-IN-CORE` (P0 by maintainer decision), one of three recorded
prerequisites for the document-driven setup path in `design/environment-builder`.

**Scope widened at the user's direction after the first Phase 1 draft; complexity M -> L.** The
issue as filed proposed moving *pure data only* and explicitly left `StoreFactory` and
`StoreRouterBuilder` in `liquers-store`. That boundary is rejected: `liquers-web` needs the builder
and the factory trait as much as the config types, so under the data-only boundary its
`liquers-store` dependency survives and the stated goal is not met. The committed target is that
**`liquers-web` depends on `liquers-store` not at all**, which requires the config types, the
`StoreFactory` trait, factory chaining and `StoreRouterBuilder` all to land in `liquers-core`.
`liquers-store` is reduced to the OpenDAL backend crate plus compatibility re-exports.

**Three pieces that do not exist today.** Factory *chaining* into a composite factory, last-wins,
with an `eprintln!` warning when chained factories claim overlapping `store_type` strings; a *core
factory* for the stores core already implements (`memory`, and `filesystem` off wasm); and a
*parametrisable* factory assembled from named creation functions rather than a trait impl.
`liquers-store` supplies an OpenDAL factory and a ready-made core-then-OpenDAL chain.

**Precedence inverts.** `StoreRouterBuilder::with_factory` is documented today as first-wins ("a
later factory cannot shadow an earlier one", `store_builder.rs`). Chaining is last-wins. One in-tree
caller, registering one factory, so nothing breaks — but the contract reverses and
`design/liquers-web-store/phase2-architecture.md` asserts the old rule.

**No in-tree chain overlaps.** Core claims `memory`/`filesystem`; OpenDAL claims `fs`, `s3`, `http`,
...; `WebStoreFactory` claims `localstorage`, `js`, `http`, `https`. Since `liquers-web` stops
chaining the OpenDAL factory, both real chains are clean — the new warning has no in-tree trigger
and needs a deliberate test.

**`liquers-store`'s `opendal` feature may lose its reason to exist.** Its manifest comment says it is
optional "so that a wasm32 consumer can depend on this crate for its configuration and builder
alone" — exactly what this change removes. Phase 2 decides whether the feature stays.

Phase 1 correction to the issue: its verification item 3 was unachievable under the data-only
boundary (`liquers-web` also uses `StoreRouterBuilder` and implements `StoreFactory`). Under the
widened boundary it is achievable and strengthens to "no `liquers-store` dependency at all". The
issue's "what moves and what does not" table is superseded and is corrected at Phase 5, along with
`complexity: L`.

Documentation intent changed with the scope: Phase 1's earlier `neither` on a guide no longer holds.
"How do I add a store type" and "how do I override a built-in one" become repeatable tasks with a
real answer, so a new `specs/guides/STORE_FACTORY_GUIDE.md` is provisionally committed, with
`WebStoreFactory` as the worked example.

Open for Phase 2: the exact reading of "parametrisable store creation function/method"; whether the
builder keeps an implicit built-in fallback; whether `with_factory` survives alongside chaining;
where the overlap warning fires; that `eprintln!` is silent on wasm, which is where an override is
most likely; that `create_store`'s centralized unknown-vs-unavailable error messages degrade when
split across factories; the re-export shape; `toml` feature forwarding; and whether the §3 `area`
vocabulary needs `core/store` widened now that `store/config` names files that will not exist.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
