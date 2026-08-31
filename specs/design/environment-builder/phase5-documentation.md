# Phase 5: Documentation - Environment Builder

## Completion Preconditions

- [x] Implementation is finished and validated — all twelve Phase 4 steps
- [x] All user comments are answered or incorporated — the two gate decisions of 2026-08-31
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with implemented and tested behavior
- [x] Documentation is included in the implementation PR

## Implementation Summary

`QUEUED-MANAGER-STARTUP-READINESS` (P1) is closed. `Environment::to_ref` used to install the asset
manager's back-reference, spawn `AssetManager::start` as a detached task, and return — so a caller
could evaluate against a manager whose command versions were not registered yet. The symptom was
silent: `register_plan_dependencies` skips any dependency whose version the manager does not know,
so a plan evaluated in that window registered **no** dependency edges and nothing ever invalidated
the assets built from it.

The fix is structural rather than a check. `Environment::try_to_ref` owns one readiness sequence —
refresh command metadata versions, create the `EnvRef`, then call `init_with_envref`, which
constructs the manager with that reference, installs it, and starts it. No reference escapes the
function before startup completes. `EnvironmentBuilder::build` **delegates** to that sequence rather
than reimplementing it, so both construction paths carry one guarantee with one implementation.

`AssetManager::start` became synchronous and fallible. Its only reason to be async was
`scc::HashMap::entry_async`; the work is uncontended in-memory map writes and touches no store, so
`DependencyManager::register_version_sync` (on `entry_sync`) replaces it. That is what let `to_ref`
keep its exact signature — all 348 call sites — while becoming correct.

Conformance with the approved design is complete, with four deliberate departures recorded below.
Delivered beyond the readiness fix, both by maintainer decision at the Phase 3 gate:

- **One generic environment.** `GenericEnvironment<V, P, K>` replaces four near-duplicate structs;
  the four names survive as type aliases, so no call site moved. `context.rs` 2113 → 1813 lines,
  and `liquers-lib`'s environment 204 → 94.
- **One configuration document.** `EnvironmentConfig` in `liquers-core` embeds `StoreRouterConfig`
  verbatim, so a single YAML/JSON/TOML file configures the environment and its store together.

### Scale

| | |
|---|---|
| Steps executed | 12 of 12 |
| Files changed | 32 (+2108 / −879) |
| New modules | `environment_builder.rs` (555 lines), `environment_config.rs` (279) |
| New tests | 12 integration, 8 builder unit, 6 config unit, 5 parametric |
| Suites green | `liquers-core` 22 suites, `liquers-lib` 18, all 16 feature configurations |

## Deviations from the approved design

Four, each found during implementation and each recorded rather than quietly applied.

**1. `register_version_sync` returns `bool`, not `ExpiredDependents`.** Phase 2's signature is
unimplementable: `DependencyManager::expire_dependents` reaches `scc` through `get_async` and
`iter_async`, so a synchronous registration cannot compute the dependents. Detection and
application are split instead — `register_version_sync -> bool`,
`AssetManager::refresh_command_versions -> Result<Vec<DependencyKey>, Error>`, and a provided
`async refresh_command_versions_and_expire` that cascades what the sync half reports. This resolved
Phase 2 open question 1 and is strictly better than the sketch, because the async boundary is now
where the async work actually is. **Read Phase 2 as amended by this.**

**2. Steps 3 and 4 were split.** Phase 2 treated the ownership change and the contract change as
one edit. Step 3 moved the manager slot to the environment using the *existing* `set_envref` and
detached start — self-contained, compiling, and running the entire existing suite against the new
ownership before any trait changed. It caught nothing, which is the outcome that makes the split
worth recording: the risk was real and the cost of derisking it was one commit.

**3. `installed_manager` introduces a panic where two were removed.** The design said the
environment-side `OnceLock` read would use `debug_assert!` plus the installed value. It cannot:
`get_asset_manager` returns `Arc<Manager>` and an unset slot has no value to return. Some deferred
slot is unavoidable — the manager needs an `EnvRef` and the environment owns the manager — so
exactly one unset-state read exists either way, and moving it from the manager to the environment
relocates rather than removes it. Net count is unchanged (two `expect`s deleted, one `panic!`
added) and reachability strictly improves: the removed ones guarded a state `to_ref` did *not*
preclude, while the added one guards a state `try_to_ref` structurally precludes. Worth knowing
that the "no unset state at all" claim in Phase 1 question 2 is true of the *manager* only.

**4. `liquers-py` came into scope.** Phase 2 left it out. Once `init_with_envref` became fallible
and carried the readiness contract, its `todo!()` was a panic on a supported path; it now returns an
explicit `Err`. The crate's environment is still a stub.

Also, two things the plan expected to cost more than they did. `liquers-web` needed **no code
change** — only a stale comment — and the axum examples compile unchanged, both because the gate
decision kept `to_ref` and the environment setters. And `with_default_recipe_provider` /
`with_trivial_recipe_provider` were kept on `GenericEnvironment` rather than dropped, because 22
call sites use them; they are now thin wrappers over `with_recipe_provider_choice`, so the choice is
spelled one way everywhere.

## Documentation Delivered

### New Reference Documents

- [`specs/reference/ENVIRONMENT_CONFIG.md`](../../reference/ENVIRONMENT_CONFIG.md) — the
  configuration document field by field, its deferred failures, the two deliberate omissions
  (manager kind, store factories), and the `recipes`-absent asymmetry.

### New Guide Documents

- [`specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md`](../../guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md)
  — building an environment, choosing an execution model, configuring from a document, library
  defaults, the readiness guarantee, when `to_ref` is right, and implementing a custom
  `Environment`.

### Existing Documents Reviewed or Updated

Authoritative `affects_docs`: `DOC_04_ENVIRONMENT_CONTEXT_EVALUATION`,
`DOC_03_ASSETS_EXECUTION_LIFECYCLE`, `ENVIRONMENT_CONSTRUCTION_GUIDE`, `ENVIRONMENT_CONFIG`,
`LANGUAGE-INTEGRATION_GUIDE`, `PAYLOAD_GUIDE`, `STORE_CONFIG_FSD`.

| Document | Change | `reviewed:` |
|---|---|---|
| `DOC_04_ENVIRONMENT_CONTEXT_EVALUATION` | Initialization sequence replaced with `try_to_ref`'s; builder documented as recommended; `init_with_envref`'s contract, synchronous fallible startup, `GenericEnvironment` and the kind. **Retired the P0 `EnvRef::new` and P1 unobservable-startup gap rows.** | 2026-08-31 |
| `DOC_03_ASSETS_EXECUTION_LIFECYCLE` | New §Manager lifecycle: constructors take the `EnvRef`, `set_envref` gone, `start` sync/fallible, `is_started`, `refresh_command_versions` and its async companion, lazy startup removed. | 2026-08-31 |
| `LANGUAGE-INTEGRATION_GUIDE` | §VALUE shows `with_type_registry`; records that an integration defining its own `Environment` carries the readiness obligation. | 2026-08-31 |
| `PAYLOAD_GUIDE` | Records that the payload environments are aliases now. Payload semantics unchanged and not otherwise re-reviewed. | 2026-08-31 |
| `STORE_CONFIG_FSD` | Links `EnvironmentConfig`, which embeds it. Format unchanged. | 2026-08-31 |
| `ASSET_LIFECYCLE` | **Dropped from `affects_docs`.** It is already labelled as predating the inline manager and is scheduled for regeneration (`DOC_03` gap row P1); adding a row would imply a review this work did not do. |

`CLAUDE.md` §Adding a Value Type now points at `EnvironmentBuilder::with_type_registry`.

### Links and Capability Map

`specs/README.md`: a "Build and configure an environment" row in the task table; **Environment
construction and manager readiness** under Assets; **Configuring an environment and its store from
one document** under Stores. Generated blocks and `specs/index.csv` regenerated.

## Issues Filed

- `WEB-STORE-CONFIG-NOT-APPLIED-THROUGH-ENVIRONMENT-CONFIG` (P3) — `liquers-web` hand-rolls the
  configuration-apply path `EnvironmentConfig` now owns (`apply_store`, `STORE_CONFIG`,
  `STORE_OBJECTS`). Migrating it is what makes the configuration layer pay off for the JavaScript
  target, and was deliberately left out: that rebuild path is the crate's most delicate code.

Nothing else was deferred. `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` (P2) and
`POST-INIT-COMMAND-REGISTRATION` (P3) remain open by prior decision; both files record where this
work leaves them.

## Important Learning

**Reversing a Phase 2 finding made the design smaller.** Finding A1 had removed `to_ref` from the
trait, arguing that a defaulted body cannot construct a builder for an arbitrary implementor. True,
and beside the point: the body needs the *sequence*, not a builder, and the one step that varies was
already abstracted behind `init_with_envref`. Restoring it meant `build()` delegates instead of
duplicating, so one guarantee has one implementation — and the metadata-version refresh invariant is
inherited structurally rather than remembered. Generalizable: when a trait method "cannot be
generic", check whether the varying part is already behind a neighbouring hook before moving the
method to a concrete type.

**A construction path that bypasses another must be re-checked whenever the bypassed one changes.**
`refresh-command-metadata-versions` added a `refresh_metadata_versions()` call at the head of
`to_ref` while this design waited between phases. Phase 2's `build()` sequence predated it and had
no equivalent, so implementing the design as written would have silently reopened
`MACRO-LEAVES-STALE-METADATA-VERSION` for every environment built through the builder. Nothing in
the workflow catches that automatically; the review that found it was a deliberate re-read against
`HEAD`.

**The defect had no error to notice, which is why it needed a construction-time guarantee.**
`register_plan_dependencies` reads `if let Some(ver) = get_version(&plan_dep.key)`. A missing
version is a skipped iteration, not a failure. Any fix shaped as a check would have had to know
where to look; making the unready state unreachable does not. This is preserved as a differential
test pair — the edge forming after `build()`, and the same call registering nothing for a key with
no version.

**Deferred state cannot be eliminated, only placed.** The manager needs an `EnvRef`, the environment
owns the manager. `Arc::new_cyclic` cannot help while the back-reference is strong, since its
closure yields a non-upgradable `Weak`. So exactly one side carries an unset state and one read has
to handle it. Choosing the *environment* side is still right — `get_envref` is called at 78 sites in
manager hot paths and is now a plain field read — but the tradeoff is a relocation, not a removal,
and Phase 1 recorded it as the latter.

**The gate decision to keep `to_ref` paid for itself twice.** `liquers-web`, the hardest call site
in the tree, needed no code change at all; the axum examples compiled unchanged; and a
hand-written `Environment` — the case the builder deliberately does not serve — gets the same
readiness guarantee, which test T14 pins.

## Conformance and Remaining Work

| Scope | State |
|---|---|
| Requested: an observable readiness boundary (`QUEUED-MANAGER-STARTUP-READINESS`) | **Delivered.** All six issue verification items covered by tests. |
| Approved Phase 2/3: consolidation, builder, re-runnable barrier, `to_ref` kept | **Delivered**, with the four deviations above. |
| Gate decision D1: `to_ref` stays, phased out only where cheap | **Delivered.** No deprecation on `to_ref`, constructors stay `pub`, 348 call sites untouched; one axum example migrated as the flagship. |
| Gate decision D2: one configuration document | **Delivered.** `EnvironmentConfig` in `liquers-core`. |
| Not in scope, unchanged | The two `Arc` cycles; post-construction command registration. |
| Deferred, filed | `liquers-web`'s hand-rolled configuration path. |

Nothing remains outstanding for this design.

## Validation

| Check | Result |
|---|---|
| `cargo test -p liquers-core --lib --tests` | 22 suites green, including the new `environment_builder` suite |
| `cargo test -p liquers-lib --lib --tests` | 18 suites green; 17 polars tests via the extension trait |
| `bash scripts/check-build-matrix.sh` | **All 16 configurations OK**, including the wasm32 rows |
| `cargo check -p liquers-web --target wasm32-unknown-unknown` | Clean, with no code change to that crate |
| `cargo check -p liquers-axum --examples` | Clean |
| `cargo check -p liquers-py --lib` | Clean |
| `python3 scripts/docs_index.py --check` | 210 documents, 0 errors |
| `liquers-validate -- 'world/greet'` | Ok; the only query in the new guide |

The removed 50 ms sleep in `dependency_manager_integration.rs` is itself a validation result: that
assertion is now deterministic.

No rebase or merge conflict has changed this work since it was written; if one does, the
affected material needs re-reviewing per the workflow.
