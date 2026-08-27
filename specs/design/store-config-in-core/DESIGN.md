---
id: STORE-CONFIG-IN-CORE
kind: design
title: Store configuration and factories in liquers-core
workflow: liquers-project
status: draft
phase: architecture
area: [core/store, store/config, store/backends, web, docs]
gh_pr: []
issues: [STORE-CONFIG-IN-CORE]
affects_docs: [reference/STORE_CONFIG_FSD.md, reference/api/DOC_01_ARCHITECTURE_REFERENCE.md, guides/LANGUAGE-INTEGRATION_GUIDE.md, guides/STORE_FACTORY_GUIDE.md]
created: 2026-08-27
superseded_by:
---
# Store Configuration and Factories in `liquers-core` — Design Tracking

**Created:** 2026-08-27

## Phase Status

- [x] Phase 1: High-Level Design (approved)
- [x] Phase 2: Solution & Architecture (awaiting approval)
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

**Three pieces that do not exist today.** Factory *chaining* into a composite factory; a *core
factory* for the stores core already implements (`memory`, and `filesystem` off wasm); and a
*parametrisable* factory assembled from a map of store-type names to creation functions rather than
a trait impl. `liquers-store` supplies an OpenDAL factory and a ready-made core-then-OpenDAL chain.

**Phase 1 gate decisions (user, second round).** Chaining is **first-wins**, with core registered
first, then `liquers-store`, then `liquers-lib`, then the integration — so the core definition of a
store type is stable and no downstream crate can redefine it. The overlap warning and its
`eprintln!` are **not implemented**; `store_types()` stays on the trait, so a factory still reports
what it claims and a caller can detect overlap if it wants to. The map-based parametrisable factory
is **confirmed**. `liquers-store`'s `opendal` feature is **kept** — non-OpenDAL backends in that
crate are expected, so an OpenDAL-free configuration keeps its purpose even though the wasm reason
in its manifest comment no longer applies.

**The browser's `http` override changes mechanism.** Today `WebStoreFactory` beats the built-in
OpenDAL `http` *because* factories are consulted before built-ins;
`design/liquers-web-store/phase2-architecture.md` argues explicitly that consulting factories second
would make that impossible. Under first-wins with core first, a later factory can never override an
earlier one — the override survives only because `liquers-web` drops `liquers-store` and the OpenDAL
factory claiming `http` is never in its chain. Same outcome, different mechanism, so that rationale
is superseded rather than relocated and the new rule must be documented where a reader finds it.

**Phase 1 gate decisions (user, third round).** The builder gets **no built-in fallback** — every
store it creates comes from a factory it was given — and each crate instead offers a **default
factory** as a convenience (core's is the core factory; `liquers-store`'s is core's chained with
OpenDAL's). An **unclaimed `store_type` is an error that lists the store types the chain supports**,
enumerated from the factories themselves, so the message is accurate for the build in hand.
**Overriding is a chain the caller composes**, putting their factory first: first-wins fixes the
*default* ordering, not the only possible one. And a factory **describes, per store type, the
configuration arguments it accepts** — which is what makes the supported-types error possible and
lets the configuration format be documented from the code that implements it.

The argument-description requirement is the piece with the most design freedom left.
`command_metadata.rs`'s `ArgumentInfo` is the nearest precedent but is shaped for positional command
parameters (`multiple`, `injected`, `gui_info`, `CommandParameterValue`) while store configuration is
a `HashMap<String, serde_json::Value>` of named keys. Phase 2 chooses reuse, subset or a
store-specific type, and decides how many optional-vs-required/default/enum fields are worth the
cost to every factory implementation.

Phase 1 correction to the issue: its verification item 3 was unachievable under the data-only
boundary (`liquers-web` also uses `StoreRouterBuilder` and implements `StoreFactory`). Under the
widened boundary it is achievable and strengthens to "no `liquers-store` dependency at all". The
issue's "what moves and what does not" table is superseded and is corrected at Phase 5, along with
`complexity: L`.

Documentation intent changed with the scope: Phase 1's earlier `neither` on a guide no longer holds.
"How do I add a store type" and "how do I override a built-in one" become repeatable tasks with a
real answer, so a new `specs/guides/STORE_FACTORY_GUIDE.md` is provisionally committed, with
`WebStoreFactory` as the worked example.

Open for Phase 2: how rich the per-store-type argument description is, and whether the resulting
store-type registry should be exportable the way `specs/command_registry.yaml` is; whether a factory
can explain a type it knows of but cannot build (the `opendal`-off and wasm-`filesystem` messages
worth not losing); whether `with_factory` survives alongside chaining, given that with no built-in
fallback `StoreRouterBuilder::new(config)` alone can build nothing; whether `expand_env_vars`'s bare
`std::env::var` moves verbatim, is `#[cfg]`-gated or takes a closure; the re-export shape; `toml`
feature forwarding; and whether the §3 `area` vocabulary needs `core/store` widened now that
`store/config` names files that will not exist.

Noted for filing rather than absorbing: with per-type argument descriptions in hand, a chain could
validate a `StoreRouterConfig` — unknown type, unknown key, missing required key — without
constructing a single store. Attractive, and beyond this design's scope.

## Phase 2 notes

**No blocker found** in the known-issue preflight. `WEB-NATIVE-IO-TIER2` (P3) is the one non-blocker
with a real design constraint, honoured rather than deferred: no `Send`/`Sync` bound on the trait or
on the map factory's closures, so a Promise-based IndexedDB store stays expressible.

**Reuse found rather than invented.** `command_metadata::ArgumentType` covers store argument types
and is reused instead of a parallel enum; `Error::not_supported` replaces `general_error` for an
unclaimed store type; `AsyncMemoryStore` / `AsyncFileStore` are already core types, so the core
factory constructs nothing new.

**`StoreFactory::store_types()` changes return type** (`Vec<String>` -> `Vec<StoreTypeInfo>`).
Breaking, and taken deliberately: two in-tree implementors, both edited anyway, and a parallel
`store_type_info()` with a default impl would leave two sources of truth for what a factory claims.

**`StoreTypeAvailability`** preserves what `create_store`'s single `match` provides today and what
`LANGUAGE-INTEGRATION_GUIDE.md` makes conformance requirement `STORE13`: a type that is real but
ungated-off in this build must be refused with the feature or target responsible, never as "unknown".

**Two findings outside the Phase 1 boundary, surfaced at the gate rather than absorbed:**

1. `from_yaml` / `from_json` / `from_toml` / `expand_env_vars` use `Error::new(ErrorType::ParseError,
   ...)`, which `CLAUDE.md` forbids, and no typed constructor fits (`key_parse_error` and
   `query_parse_error` both require a `Position`). Proposed: add `Error::parse_error(String)` to
   `liquers-core/src/error.rs` rather than move a known violation into the crate that enforces the
   rule most strictly.
2. `scripts/check-build-matrix.sh` has **no `liquers-core` rows at all**, and core is about to gain
   its first optional feature (`toml`) and target-conditional store availability. Four rows proposed.

**The documentation finding that matters most.** `LANGUAGE-INTEGRATION_GUIDE.md` §"Taking only part
of the store support crate" enumerates three resolutions to exactly this problem, recommends option 3
(optional backend feature) and explicitly rejects option 2 (move the types into `liquers-core`) as
"widens core for one consumer's benefit". This design does option 2. The rejection was written when
`liquers-web` was the only consumer and no longer holds once core itself must embed a store
description; the section is rewritten to record the reversal and its reason, while keeping option 3's
three cost lessons, which remain true of the surviving `opendal` feature. Conformance item `STORE12`
("a factory that overrides a shared type name resolves to the integration's implementation") also
needs restating: after this change `liquers-web` has nothing to override.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
