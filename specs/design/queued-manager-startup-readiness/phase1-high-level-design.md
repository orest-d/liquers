# Phase 1: High-Level Design - Environment Builder

## Feature Name

Environment Builder (resolves `QUEUED-MANAGER-STARTUP-READINESS`)

## Purpose

An `Environment` owns its `AssetManager`, and the manager needs an `EnvRef` back to that
environment — a construction cycle. Today it is broken by building the manager unattached and
back-filling `set_envref` from `init_with_envref`, *after* `EnvRef` already exists and is already
shareable. Replace that with a builder that owns the whole cycle inside one fallible, awaited
`build()`, so a partially initialized environment is never observable and the manager and
environment variants become a runtime choice rather than four near-duplicate structs.

## Core Interactions

### Query System
None. No change to parsing, planning or `Key` encoding.

### Store System
The builder becomes the place where the async store is selected, replacing today's
`with_async_store(&mut self)` setter. No store implementation changes.

### Command System
The builder owns the configure-then-freeze boundary: commands are registered into the builder, and
`build()` freezes the registry. This is the same boundary `POST-INIT-COMMAND-REGISTRATION` (P3)
wants to relax, so the design must not foreclose it. Startup's `load_command_versions` reads the
frozen registry, and `build()` awaits it — which is what closes the readiness hole.

### Asset System
Central. `build()` constructs the manager with the envref already available, installs it, awaits
`AssetManager::start`, and only then hands back an `EnvRef`. `DefaultAssetManager` (queued) and
`ImmediateAssetManager` (inline) become selectable rather than baked into the environment type;
`liquers-lib` already fakes this selection with a `SelectedAssetManager` cfg alias.

### Value Types
None. The builder is where a caller-supplied `TypeRegistry` is passed instead of
`new_with_type_registry`.

### Web/API
`liquers-web` and `liquers-axum` construct environments and call `to_ref`. Both must migrate, and
`liquers-web`'s wasm paths need a `build()` that works without a Tokio runtime.

### UI
None directly; `liquers-lib`'s egui/webui environments are built through the same path.

## Crate Placement

**liquers-core** — new builder module plus `src/context.rs` (the four built-in environments,
`Environment::to_ref`, `init_with_envref`, `EnvRef::new`) and `src/assets.rs` (`AssetManager`
lifecycle: `set_envref`, `start`). **liquers-lib** — `src/environment.rs` (`DefaultEnvironment`,
`SelectedAssetManager`). **liquers-web**, **liquers-axum** — migrate construction sites.
`liquers-py`'s `init_with_envref` is `todo!()` and stays out of scope.

## Documentation Intent

**Reference:** Extend, do not create.
`specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` owns the initialization sequence and
already carries both defects this closes as gap rows — P1 "Manager startup completion is not
observable" and P0 "`EnvRef::new` creates an evaluation-unsafe uninitialized reference". It must
describe the builder as the construction path and retire both rows.
`specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` must restate the manager lifecycle
primitives under the new ownership.

**Guide:** Create `specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md`. Building and configuring an
environment — choosing a manager, registering commands, attaching a store and recipe provider,
getting a ready `EnvRef` — is exactly the repeatable "what is the typical workflow for X?" task a
guide exists for, and it is currently reconstructed by copying from tests. Phase 1 previously said
"extend the language-integration guide"; the builder makes this a workflow in its own right.

**Other documents to create:** None.

**Specific documents to update:** `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`,
`specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`,
`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` (what a host-language environment owes the contract),
`CLAUDE.md` (§Adding a Value Type points at `new_with_type_registry`), `specs/README.md`,
`specs/index.csv`, `specs/issues/QUEUED-MANAGER-STARTUP-READINESS.md` and
`specs/issues/ENVIRONMENT-MANAGER-REFERENCE-CYCLE.md` at Phase 5.

Audience: framework maintainers and language integrators. Afterwards they should be able to build a
correctly initialized environment from the guide alone, and to tell from the reference what an
`EnvRef` guarantees.

## Open Questions

1. **One environment or four? — Phase 2 research task.** The user's position: the environment must
   be *configurable*; consolidating today's four structs is welcome but not required, and internal
   multiplicity is acceptable where it buys optimization. Two requirements constrain the answer and
   are not negotiable:
   - the caller must be able to specify the **`Value` type**;
   - the caller must be able to **implement their own `Environment`**, to carry custom global
     services.
   The second is the sharp one: the builder cannot own a fixed concrete environment type. It must be
   generic over `E: Environment`, driving a hook `E` exposes, so a user-defined environment reaches
   the same guarantees as the built-in ones. Phase 2 researches whether consolidation is worth it
   under that constraint.
2. **Manager construction shape.** `Arc::new_cyclic` (manager built with a `Weak` back-reference, no
   `OnceLock` at all, manager well-formed at birth, "envref not set" panic path gone) versus a
   `FnOnce(EnvRef<E>) -> Arc<M>` factory the builder invokes after wrapping (smaller diff, keeps the
   back-fill but hidden inside `build()`). Decide on construction-shape grounds alone — see
   question 3 for why this is *not* also the leak fix.
   Constraint to verify in Phase 2: `Arc::new_cyclic`'s closure cannot upgrade the `Weak`, and
   `DefaultAssetManager::with_capacity` spawns the job queue and expiration monitor from inside the
   constructor. Those tasks must not reach for the environment before `new_cyclic` returns.
3. **Is the cycle fix in scope? — reassessed, and larger than filed.** There are **two** cycles, not
   one. Besides the manager's `set_envref` back-reference, `AssetData<E>` holds a strong
   `envref: EnvRef<E>` and the manager's `assets` / `query_assets` maps hold those assets — so every
   cached asset closes a second cycle independent of the first. Weakening only the manager's
   back-reference does not stop the leak. Sizing: 78 `get_envref()` sites (68 in `assets.rs`) plus 16
   `ImmediateAssetManager::envref()` sites, and the cost turns on whether the accessor keeps
   returning `EnvRef<E>` (panicking at teardown, in background tasks) or starts returning
   `Option`/`Result`. Recommendation: keep `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` out of this
   project's committed scope and let the builder merely not make it worse.
4. **~~What happens to `to_ref` and `EnvRef::new`?~~ Decided.** `EnvRef::new` is deprecated — it has
   exactly one in-tree caller, `to_ref` itself, so this is free. `to_ref` is withdrawn from the
   public surface. Note it cannot literally be made *private*: it is a defaulted method on the public
   `Environment` trait, and a public trait has no private methods. Phase 2 picks the shape that
   delivers the intent — remove it from the trait so the builder is the only path (preferred), or
   `#[deprecated]` + `#[doc(hidden)]` with the body delegating to the builder. Migration size: 336
   `.to_ref()` call sites, overwhelmingly in tests and examples (125 in `assets.rs`, 29 in
   `interpreter.rs`, the rest across the integration suites), so mechanical but not small.
5. **~~Async or sync `build()`?~~ Decided: sync (option A).** `start()` is async only because
   `DependencyManager::register_version` awaits `scc::HashMap::entry_async`. At build time that map
   is empty and uncontended, every command key inserts `Vacant`, so `version_changed` is always
   `false` and `expire_dependents` can never fire; no store is touched (`load_from_records` is
   reached from asset recovery and `track_asset`, never from `start()`). scc offers `entry_sync`,
   already used at `assets.rs:5166`. So the async is incidental to the map API, not to the work, and
   the readiness guarantee rests on `build()` being the only way to obtain an `EnvRef` — not on
   asyncness. Phase 2 makes startup sync and gives `AssetManager` a sync startup operation.
   Two consequences to carry forward: (a) this forecloses genuinely async manager startup — a
   manager restoring a persisted dependency graph from the store would need a breaking change or the
   deferred async sibling; (b) sync does **not** mean runtime-free for the queued environment, since
   `DefaultAssetManager::with_capacity` calls `tokio::spawn` for the job queue and expiration
   monitor. Runtime-free construction is real only for the inline/wasm manager.
6. **Command registration after `build()`.** The builder makes freezing explicit. Does that close
   `POST-INIT-COMMAND-REGISTRATION` (P3), or should `build()` leave a re-runnable startup barrier so
   late registration stays reachable?
7. **Complexity.** The issue is recorded `complexity: M`; an environment builder is L. Confirm the
   reclassification, since L/XL is what mandates this design folder.

## References

- `specs/issues/QUEUED-MANAGER-STARTUP-READINESS.md` — the issue (P1, `core/assets`)
- `specs/issues/ENVIRONMENT-MANAGER-REFERENCE-CYCLE.md` — filed during this phase (P2)
- `specs/issues/POST-INIT-COMMAND-REGISTRATION.md` — adjacent P3, constrains question 6
- `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` §gap table, rows P0 and P1
- `liquers-store/src/store_builder.rs` — `StoreRouterBuilder`, the in-tree builder precedent
- `liquers-core/src/context.rs` (four environments, `to_ref`, `init_with_envref`, `EnvRef::new`),
  `liquers-core/src/assets.rs` (`load_command_versions`, `DefaultAssetManager`,
  `ImmediateAssetManager`, `register_plan_dependencies`), `liquers-lib/src/environment.rs`
