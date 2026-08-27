---
id: QUEUED-MANAGER-STARTUP-READINESS
kind: design
title: Environment builder
workflow: liquers-project
status: draft
phase: architecture
area: [core/assets, core/context]
gh_pr: []
issues: [QUEUED-MANAGER-STARTUP-READINESS, ENVIRONMENT-MANAGER-REFERENCE-CYCLE, CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC]
affects_docs: [DOC_04_ENVIRONMENT_CONTEXT_EVALUATION, DOC_03_ASSETS_EXECUTION_LIFECYCLE, ENVIRONMENT_CONSTRUCTION_GUIDE, LANGUAGE-INTEGRATION_GUIDE, PAYLOAD_GUIDE, ASSET_LIFECYCLE]
created: 2026-08-27
superseded_by:
---
# queued-manager-startup-readiness Design Tracking

**Created:** 2026-08-27

## Phase Status

- [x] Phase 1: High-Level Design (approved)
- [x] Phase 2: Solution & Architecture (awaiting approval)
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Resolves issue `QUEUED-MANAGER-STARTUP-READINESS` (P1; complexity reclassified M -> L).

Race confirmed empirically: immediately after `to_ref()` the dependency manager holds no command
versions, and `register_plan_dependencies` therefore silently registers zero edges for a plan
evaluated in that window.

Root cause is the Environment/AssetManager construction cycle, broken today by back-filling
`set_envref` after `EnvRef` is already shareable. Scope was widened at the user's direction from a
readiness barrier to an environment builder that owns that cycle.

Phase 1 decision (user, option A): `build()` and asset-manager startup are **sync**. The async in
today's `start()` comes only from `scc`'s `entry_async`; startup does uncontended vacant inserts into
an in-memory map and never touches the store.

Cleanup to fold into Phase 2 rather than file separately: `DefaultAssetManager::with_capacity` has an
unconditional `eprintln!("Spawned job queue")` that fires on every manager construction
(`liquers-core/src/assets.rs`). Stray debug output in code this design rewrites.

Also filed during Phase 1: `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` (P2) — the manager's back-reference
is a strong `Arc`, so every environment leaks (`Arc::strong_count(&envref.0) == 2` after `to_ref`).
Reassessed: there are **two** cycles. `AssetData<E>` also holds a strong `EnvRef<E>`, and the
manager's `assets` / `query_assets` maps hold those assets, so every cached asset closes a second
cycle. Weakening only the manager's back-reference does not fix the leak; recommendation is to keep
that issue outside this project's committed scope.

Phase 1 decisions (user): `EnvRef::new` is deprecated (one in-tree caller); `to_ref` **stays public**,
so its 336 call sites need no migration — but its body must be reimplemented over the builder path so
it is fully ready on return. Sync startup makes that possible without changing its signature.

Manager construction: factory, not `Arc::new_cyclic`. Keeping the back-reference strong rules
`new_cyclic` out (its closure yields a non-upgradable `Weak`), and `Weak::upgrade` costs more than
`Arc::clone` — a CAS loop rather than a relaxed `fetch_add`, across 78 `get_envref()` sites. The
deferred slot moves from the manager to the environment, so the manager has no unset state at all.

Recipe provider raises the same "component needs the environment" problem in a third shape:
`AsyncRecipeProvider` takes `envref` as an argument on every method. Three components, three
different solutions, none chosen deliberately — the builder is where that becomes a decision.

Command registration: the startup barrier must be **re-runnable**, not one-shot. Dynamic command
registration and command-metadata modification are long-term goals, and re-registering a changed
`metadata_version` already triggers `expire_dependents`, i.e. the cascade that invalidates dependent
assets. `ImmediateAssetManager`'s `tokio::sync::OnceCell` would foreclose that.

Reference cycle deferred by decision, not oversight: one environment per realm for the process
lifetime means no practical cost; soft reboot is the case that would surface it. Environment consolidation is a Phase 2 research task, constrained by two firm
requirements: the caller specifies the `Value` type, and a caller can implement their own
`Environment` for custom global services.

Revised: the builder does **not** need to support externally defined environments — a user with a
custom environment may replicate the construction. So the builder may own concrete environment types
instead of being generic over `E: Environment`, which is what makes consolidation tractable. Custom
global services are expected to arrive later via a separate route.

Future direction the design must not preclude (not in scope): a single YAML-serializable
`EnvironmentConfiguration` covering manager, commands, recipe provider and store.
`liquers-store`'s `StoreRouterConfig` / `StoreRouterBuilder` is the working precedent. Layering
constraint: `StoreRouterConfig` lives in `liquers-store`, which depends on `liquers-core`, so a
core-side configuration type cannot embed it — this decides where the builder can live. Also note
`E::Payload` is per-execution, not a global service bag; a "global payload" would be a distinct
environment-lifetime thing.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
