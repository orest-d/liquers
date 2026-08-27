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

1. **One environment or four?** Does the builder produce a single environment generic over manager
   and payload — the "select between environment versions and asset manager versions" goal — or does
   it keep constructing today's four structs? The former removes real duplication and the
   `SelectedAssetManager` cfg alias; it is also the larger change.
2. **Manager construction shape.** `Arc::new_cyclic` (manager built with a `Weak` back-reference, no
   `OnceLock` at all) versus a `FnOnce(EnvRef<E>) -> Arc<M>` factory the builder invokes after
   wrapping. The first also fixes `ENVIRONMENT-MANAGER-REFERENCE-CYCLE`; the second is a smaller
   diff but keeps the back-fill, only hidden inside `build()`.
3. **Is the cycle fix in scope?** Filed as `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` (P2). It is cheap
   here and expensive later — fold it in, or keep this project to readiness only?
4. **What happens to `to_ref` and `EnvRef::new`?** Deprecate-and-keep, or make private? Every
   existing test, example, `liquers-web` entry point and `liquers-axum` setup calls `to_ref`; a hard
   break is a large mechanical migration.
5. **Async `build()` on the spawn-free path.** `build()` must be async to await `start()`, but
   `ImmediateEnvironment` is meant to be constructible without a Tokio runtime. Is an async `build()`
   acceptable everywhere (it is still just a future under wasm), or is a sync `build()` plus an
   explicit readiness await also needed?
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
