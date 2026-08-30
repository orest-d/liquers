---
id: ENVIRONMENT-BUILDER
kind: design
title: Environment builder
workflow: liquers-project
phase: examples
area: [core/assets, core/context]
gh_pr: [44]
issues: [QUEUED-MANAGER-STARTUP-READINESS, ENVIRONMENT-MANAGER-REFERENCE-CYCLE, CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC, STORE-CONFIG-IN-CORE, COMMAND-DECLARATION-FORMAT, RECIPE-PROVIDER-BY-NAME]
affects_docs: [DOC_04_ENVIRONMENT_CONTEXT_EVALUATION, DOC_03_ASSETS_EXECUTION_LIFECYCLE, ENVIRONMENT_CONSTRUCTION_GUIDE, LANGUAGE-INTEGRATION_GUIDE, PAYLOAD_GUIDE, ASSET_LIFECYCLE]
created: 2026-08-27
superseded_by:
---
# Environment Builder Design Tracking

**Created:** 2026-08-27

## Phase Status

- [x] Phase 1: High-Level Design (approved)
- [x] Phase 2: Solution & Architecture (approved)
- [x] Phase 3: Examples & Testing (awaiting approval)
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

Follow-up from `refresh-command-metadata-versions`: the builder design must preserve the invariant
that command metadata versions are refreshed after registration mutation and before command versions
are loaded into the dependency manager. If the eventual builder delegates through the refreshed
`to_ref` path, no separate builder operation is needed; if it bypasses `to_ref`, `build()` must call
the same `CommandMetadataRegistry::refresh_metadata_versions` lifecycle operation before manager
startup.

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

## Preparatory work for document-driven setup

The JavaScript (and later Python) target is a two-document setup: one configuring the environment,
one declaring commands. Phase 3 §Scenario 4 sketches the first. Filed as prerequisites, none of them
blocking this design:

All three are **P0 by maintainer decision** (2026-08-27) — hard prerequisites, not severity. See the
priority note in each file, and the §4.4 caveat below.

| Issue | Priority | Why it comes first |
|---|---|---|
| `STORE-CONFIG-IN-CORE` | P0 | A core-side configuration type cannot embed `StoreRouterConfig` while it lives in `liquers-store`. No new core dependency; `liquers-web` already takes `liquers-store` with backends off just to reach these types. |
| `COMMAND-DECLARATION-FORMAT` | P0 | Document #2 has no home. `JsCommandSpec` hand-parses a `JsValue` field by field; Python would rewrite it. Split declarative half (serde) from implementation (resolved by name). |
| `RECIPE-PROVIDER-BY-NAME` | P0 | The one `EnvironmentConfig` field that cannot be expressed as data today. |

**Open against `DOCS_STRUCTURE_GUIDE.md` §4.4.** That table defines P1 as "something blocking planned
work" and reserves P0 for incorrect results, data loss, a panic on a supported path, or a documented
feature that does not work. These three are prerequisites, not defects, so the guide as written puts
them at P1. The P0 marking is deliberate and recorded, but §4.4 and these files now disagree: either
the guide gains a clause for hard prerequisites, or these settle back to P1. Worth resolving before
the vocabulary drifts — `CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC` is a genuine §4.4 P0 candidate (a
live panic) currently sitting at P1 below them.

Recommended priority change, **not applied** pending confirmation: `POST-INIT-COMMAND-REGISTRATION`
P3 → P2. For a document-driven host, registering commands after the environment is built is the
normal path, not the exception, and the current workaround rebuilds the environment and discards
the asset cache.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

### Preparatory issues designed separately

Three of the issues listed above now have their own design folders, prepared under
[`guides/autonomous_issue_fixing.md`](../../guides/autonomous_issue_fixing.md). They keep the same
five phase names but not this design's persistent-artifact or approval contract, and none of them
changes this design's phase documents, front-matter or workflow marker. All three are **awaiting
the approval gate — nothing is implemented.**

| Issue | Design |
|---|---|
| `RECIPE-PROVIDER-BY-NAME` | [`design/recipe-provider-selection/`](../recipe-provider-selection/) |
| `COMMAND-DECLARATION-FORMAT` | [`design/command-declaration/`](../command-declaration/) |
| `CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC` | [`design/payload-env-recipe-provider-fallback/`](../payload-env-recipe-provider-fallback/) |

`STORE-CONFIG-IN-CORE` also has its own folder now,
[`design/store-factories-in-core/`](../store-factories-in-core/), but under the full
`workflow: liquers-project` contract rather than the three above. Its scope was widened at the
maintainer's direction beyond the issue as filed — the `StoreFactory` trait and `StoreRouterBuilder`
move into `liquers-core` alongside the configuration types, so `liquers-web` drops `liquers-store`
entirely — and its complexity is reclassified M -> L. The layering constraint recorded above ("a
core-side configuration type cannot embed `StoreRouterConfig`") is what that design lifts.

Note for whoever owns this design: if the third is fixed directly, the
`SimpleEnvironmentWithPayload` row of [Phase 2](./phase2-architecture.md) §"The recipe-provider
default is per-crate" becomes stale, and that design's Phase 1 corrects a claim in the issue file
about the struct's doc comment.
