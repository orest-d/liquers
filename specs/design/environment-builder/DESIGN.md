---
id: ENVIRONMENT-BUILDER
kind: design
title: Environment builder
workflow: liquers-project
phase: documentation
area: [core/assets, core/context]
gh_pr: [44]
issues: [QUEUED-MANAGER-STARTUP-READINESS, ENVIRONMENT-MANAGER-REFERENCE-CYCLE, CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC, STORE-CONFIG-IN-CORE, COMMAND-DECLARATION-FORMAT, RECIPE-PROVIDER-BY-NAME, WEB-STORE-CONFIG-NOT-APPLIED-THROUGH-ENVIRONMENT-CONFIG]
affects_docs: [DOC_04_ENVIRONMENT_CONTEXT_EVALUATION, DOC_03_ASSETS_EXECUTION_LIFECYCLE, ENVIRONMENT_CONSTRUCTION_GUIDE, ENVIRONMENT_CONFIG, LANGUAGE-INTEGRATION_GUIDE, PAYLOAD_GUIDE, ASSET_LIFECYCLE, STORE_CONFIG_FSD]
created: 2026-08-27
superseded_by:
---
# Environment Builder Design Tracking

**Created:** 2026-08-27

## Phase Status

- [x] Phase 1: High-Level Design (approved)
- [x] Phase 2: Solution & Architecture (approved)
- [x] Phase 3: Examples & Testing (approved 2026-08-31)
- [x] Phase 4: Implementation Plan (approved 2026-08-31)
- [x] Implementation: all twelve steps complete and green
- [x] Phase 5: Documentation (awaiting approval)

## Notes

Resolves issue `QUEUED-MANAGER-STARTUP-READINESS` (P1; complexity reclassified M -> L). Since the
2026-08-31 gate decisions it also delivers the single-configuration-point goal (`EnvironmentConfig`),
which Phase 1 had recorded as future direction.

Filed during the 2026-08-31 review, not fixed here:
`WEB-STORE-CONFIG-NOT-APPLIED-THROUGH-ENVIRONMENT-CONFIG` (P3) — `liquers-web` hand-rolls the
configuration-apply path that `EnvironmentConfig` will own; migrating it is deliberately left out of
this project.

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
startup. **Satisfied by the first branch as of the 2026-08-31 gate decisions:** `build()` delegates
to `Environment::try_to_ref`, whose provided body carries the refresh, so no builder-side operation
exists to forget. The 2026-08-31 review found the design had taken the *second* branch and omitted
the call — see §Gate decisions, D1.

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
`StoreRouterConfig` / `StoreRouterBuilder` is the working precedent. **The layering constraint
recorded here originally — that `StoreRouterConfig` lived in `liquers-store` and so could not be
embedded by a core-side configuration type — was lifted on 2026-08-31** when
`design/store-factories-in-core/` merged: `store_config.rs` and `store_factory.rs` are now
`liquers-core` modules. A core-side `EnvironmentConfig` is therefore possible, which changes where
the builder and its future configuration type can live; see §Prerequisite review below. Also note
`E::Payload` is per-execution, not a global service bag; a "global payload" would be a distinct
environment-lifetime thing.

## Preparatory work for document-driven setup

The JavaScript (and later Python) target is a two-document setup: one configuring the environment,
one declaring commands. Phase 3 §Scenario 4 sketches the first. Filed as prerequisites, none of them
blocking this design:

All three were **P0 by maintainer decision** (2026-08-27) — hard prerequisites, not severity. See the
priority note in each file, and the §4.4 caveat below. **All three are now `closed`** — see
§Prerequisite review.

| Issue | Priority | Why it came first | State (2026-08-31) |
|---|---|---|---|
| `STORE-CONFIG-IN-CORE` | P0 | A core-side configuration type cannot embed `StoreRouterConfig` while it lives in `liquers-store`. | **closed.** `liquers-core/src/store_config.rs` and `store_factory.rs` exist; `liquers-web` no longer depends on `liquers-store` at all. |
| `COMMAND-DECLARATION-FORMAT` | P0 | Document #2 has no home. `JsCommandSpec` hand-parses a `JsValue` field by field; Python would rewrite it. Split declarative half (serde) from implementation (resolved by name). | **closed.** `liquers-core/src/command_declaration.rs`; `liquers-web`'s `JsCommandSpec` now builds on `CommandDeclaration`. |
| `RECIPE-PROVIDER-BY-NAME` | P0 | The one `EnvironmentConfig` field that cannot be expressed as data today. | **closed.** `RecipeProviderChoice` in `liquers-core/src/recipes.rs`, with `provider()` / `boxed_provider()` / `FromStr` / `Display`. |

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

Four of the issues listed above were designed and implemented in their own folders. **All four have
merged; none of them changed this design's phase documents, front-matter or workflow marker.**

| Issue | Design | Merged as |
|---|---|---|
| `RECIPE-PROVIDER-BY-NAME` | [`design/recipe-provider-selection/`](../recipe-provider-selection/) | PR 48 |
| `COMMAND-DECLARATION-FORMAT` | [`design/command-declaration/`](../command-declaration/) | PR 50 |
| `CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC` | [`design/payload-env-recipe-provider-fallback/`](../payload-env-recipe-provider-fallback/) | PR 51 |
| `STORE-CONFIG-IN-CORE` | [`design/store-factories-in-core/`](../store-factories-in-core/) | PR 46 |

The first three were prepared under
[`guides/autonomous_issue_fixing.md`](../../guides/autonomous_issue_fixing.md) — the same five phase
names, but not this design's persistent-artifact or approval contract — except
`command-declaration`, which was converted to `workflow: liquers-project` mid-flight.
`store-factories-in-core` ran the full `liquers-project` contract from the start; its scope was
widened at the maintainer's direction beyond the issue as filed (the `StoreFactory` trait and
`StoreRouterBuilder` moved into `liquers-core` alongside the configuration types, so `liquers-web`
drops `liquers-store` entirely), and its complexity was reclassified M -> L.

## Prerequisite review (2026-08-31)

Phases 1-3 were written on 2026-08-27, before any prerequisite had merged. This review re-read them
against `HEAD` and amended what the merges invalidated. **Nothing of the architecture changed** —
the readiness fix, the consolidation into `GenericEnvironment`, the sync fallible `build()` and the
re-runnable barrier all stand. What changed is the surrounding facts the documents cited.

| Merged work | What it invalidated | Where amended |
|---|---|---|
| `store-factories-in-core` (PR 46) | The layering constraint. `StoreRouterConfig`, `StoreFactory`, `StoreRouterBuilder` are `liquers-core` modules now, so a core-side `EnvironmentConfig` is possible and `liquers-web` no longer depends on `liquers-store`. | Phase 1 §Future Direction; Phase 2 §Integration Points and new open question 4; Phase 3 §Scenario 4 |
| `recipe-provider-selection` (PR 48) | "The one `EnvironmentConfig` field that cannot be expressed as data." `RecipeProviderChoice` exists in `liquers-core/src/recipes.rs`. | Phase 2 §Recipe Provider and §`EnvironmentBuilder` inherent API; Phase 3 §Scenario 4 |
| `command-declaration` (PR 50) | Nothing structural — the builder does not touch declaration parsing — but `liquers-web`'s `JsCommandSpec` now builds on `CommandDeclaration`, so the replay path the builder migration must preserve has a different internal shape. | Phase 3 §Scenario 2a note |
| `payload-env-recipe-provider-fallback` (PR 51) | `SimpleEnvironmentWithPayload::get_recipe_provider` no longer panics; it falls back to `TrivialRecipeProvider` and logs to stderr. The builder therefore *preserves* a fix rather than delivering one. | Phase 2 preflight row and §The recipe-provider default is per-crate; Phase 3 T9 and corner-case table |
| `refresh-command-metadata-versions` | `Environment::to_ref` now calls `refresh_metadata_versions()` before `EnvRef::new`. Phase 2's `build()` sequence predates it and omitted the step. | Phase 2 §`EnvironmentBuilder` inherent API, step 0 |

**§4.4 priority dispute is moot in practice.** The three P0-by-decision prerequisites are closed, so
nothing now sits at a priority the guide's table does not support. The guide is unchanged and gains
no hard-prerequisite clause; the recommendation is to leave §4.4 as written and to treat future
prerequisites as P1 unless they independently meet a P0 criterion.

**`POST-INIT-COMMAND-REGISTRATION` P3 → P2 remains unapplied**, still pending confirmation.

## Gate decisions (2026-08-31)

Two maintainer decisions taken at the Phase 3 approval gate. Both are applied through Phases 1-3;
neither has been implemented.

**D1 — `to_ref` stays.** The builder is the ergonomic, recommended way to construct an environment,
but ad-hoc user-created environments may still need `to_ref` or an equivalent mechanism. Phase it out
where that makes sense and is cheap; it can stay.

*Applied as:* `Environment::to_ref` keeps its trait method and signature, gains a fallible sibling
`try_to_ref`, and carries **no** deprecation. `init_with_envref` is kept — sync and fallible now,
with a strengthened contract: on return the manager is constructed, installed and started. The
`pub(crate)`-constructors refinement is withdrawn. `EnvRef::new` keeps its deprecation, since it is
the door that genuinely produces an unready reference.

*Consequence the decision did not ask for, and the reason it is a good one:*
`EnvironmentBuilder::build()` now **delegates** to `try_to_ref` rather than reimplementing the
readiness sequence beside it. One guarantee, one implementation, and the metadata-version refresh
invariant is inherited structurally instead of having to be remembered. This reverses Phase 2's
finding A1, which had moved `to_ref` to a deprecated inherent method on the ground that a defaulted
trait body cannot construct a builder — true, but the body needs the *sequence*, and the varying
step was already behind `init_with_envref`.

*Resolves:* Phase 3 open question 5 (was blocking Phase 4) and Phase 2 open question 3.

**D2 — one configuration document.** The store router configuration is part of the environment
configuration. The goal is to configure both the environment and its store from a single file or
JSON structure.

*Applied as:* `EnvironmentConfig` in `liquers-core` — `store: StoreRouterConfig`,
`recipes: RecipeProviderChoice`, `assets: AssetManagerOptions`, with the same `from_yaml` /
`from_json` / `from_toml` / `expand_env_vars` surface `StoreRouterConfig` already has. The builder
gains `with_store_config`, `with_store_config_unexpanded` and `with_config`;
`with_async_store` stays for a caller who has already built a store. The manager *kind* and the
store *factories* stay out of the document, because neither can be selected by a string: the kind
would need two different concrete types behind a non-object-safe trait, and which backends exist is
a build fact.

*Scope:* this moves Phase 1's *Future Direction* into scope and grows the project beyond its P1
readiness fix. Deliberate, and the smallest version of that growth — every type it names already
exists in `liquers-core`, and `StoreRouterBuilder::build` is synchronous, so `build()` stays sync.
Phase 4 must sequence it as the **final, separable step**, after the readiness fix is green.

*Resolves:* Phase 2 open question 4.


Note for whoever owns this design: `payload-env-recipe-provider-fallback`'s Phase 1 corrects a claim
in `CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC.md` about the struct's doc comment.
