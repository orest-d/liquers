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

### Recipe Provider
The same "component needs the environment" problem appears here in a **third** shape. The codebase
currently solves it three different ways, none of them chosen deliberately:

| Component | How it gets the environment | Consequence |
|---|---|---|
| `AssetManager` | `OnceLock<EnvRef<E>>` back-filled after `EnvRef` exists | the readiness hole; cycle 1 |
| `AssetData` / `AssetRef` | strong `EnvRef<E>` field, set at construction | correct, but cycle 2 |
| `AsyncRecipeProvider` | `envref: EnvRef<E>` passed as an argument to **every** method | no cycle and no readiness hole, but verbose, and a provider that needs the environment at construction time cannot have it |

The builder is the place to make this a deliberate, documented choice. In particular it can offer the
recipe provider the same factory treatment as the manager — constructed with the envref in hand —
which would let a provider hold what it needs instead of receiving it per call. Whether to change
the `AsyncRecipeProvider` signatures is a Phase 2 question; not changing them is a valid answer, but
the three-way inconsistency should be recorded rather than inherited.

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

## Future Direction (aware of, not in scope)

The ambition is a **single configuration point** that sets up an environment — manager selection,
commands, recipe provider, and the store — plausibly a YAML-serializable `EnvironmentConfiguration`.
Not solved here; the builder must simply not preclude it. Three facts shape it:

- **The pattern already exists.** `StoreRouterConfig` is serde-derived with
  `from_yaml` / `from_json` / `from_toml` and `${VAR_NAME}` expansion, consumed by
  `StoreRouterBuilder` plus registered factories. An `EnvironmentConfiguration` embedding it is the
  natural shape, and `StoreRouterBuilder` is the precedent the environment builder should mirror.
- **~~Layering blocks the obvious version.~~ Lifted 2026-08-31.** This bullet originally read: a
  config type in `liquers-core` cannot embed `StoreRouterConfig`, because that type lived in
  `liquers-store`, which depends on core. `STORE-CONFIG-IN-CORE` closed that gap —
  `liquers-core/src/store_config.rs` and `liquers-core/src/store_factory.rs` now hold
  `StoreRouterConfig`, `StoreConfig`, `expand_env_vars`, the `StoreFactory` trait, factory chaining
  and `StoreRouterBuilder`, and `liquers-web` dropped its `liquers-store` dependency entirely. A
  core-side `EnvironmentConfig` is therefore constructible in one crate. `RECIPE-PROVIDER-BY-NAME`
  closed alongside it, so `RecipeProviderChoice` makes the recipe section expressible as data too.
  What this *changes* for the builder is recorded as Phase 2 open question 4: whether
  `with_async_store(Arc<dyn AsyncStore>)` remains the only store entry point, or the builder also
  accepts a `StoreRouterConfig` plus a factory. It does not change anything already committed.
- **"Global payload" is not today's `Payload`.** `E::Payload` / `PayloadType` is a *per-execution*
  value reaching commands through `Context::get_payload_clone` and `InjectedFromContext`. A global
  service bag would be a distinct, environment-lifetime thing that could plausibly reuse the same
  injection machinery with a global rather than per-execution source. The two must not be conflated.

The `ENVIRONMENT_CONSTRUCTION_GUIDE.md` planned above should be written so a config-driven setup can
be added as a section later without restructuring it.

## Open Questions

1. **One environment or four? — Phase 2 research task.** The environment must be *configurable*;
   consolidating today's four structs is welcome but not required, and internal multiplicity is fine
   where it buys optimization. One firm requirement: the caller must be able to specify the **`Value`
   type**.
   The builder does **not** have to support externally defined `Environment` implementations — a user
   with a custom environment may replicate the construction. That relaxation matters: the builder may
   own concrete environment types rather than being generic over `E: Environment`, which is what makes
   consolidation tractable at all. Custom global services are expected to arrive later by a different
   route (see *Future direction* below), not by user-implemented environments.
   Phase 2 researches whether consolidation pays, and must leave the door open for that later route.
2. **~~Manager construction shape.~~ Decided: factory, and move the `OnceLock` to the environment.**
   `Arc::new_cyclic` is off the table once the back-reference stays strong (question 3): its closure
   hands out a `Weak` that cannot be upgraded inside the closure, so it only works if the manager
   keeps a `Weak`. And `Weak::upgrade` genuinely does cost more than `Arc::clone` — a compare-exchange
   loop against the strong count instead of a single relaxed `fetch_add`, plus an `Option` to branch
   on, across 78 `get_envref()` sites. Same order of magnitude, but strictly more, and paid for a leak
   the project is not committing to fix.
   So the builder keeps the two-phase back-fill and hides it inside `build()`. One improvement over
   today: move the deferred slot from the *manager* to the *environment*. Build the environment with
   an empty `OnceLock<Arc<Manager>>`, wrap it in an `EnvRef`, construct the manager with a plain
   strong `EnvRef` field, install it, start it. The manager then has no unset state and no
   `"Environment not set"` panic path at all. The environment-side slot is written by `build()`
   before any `EnvRef` is observable, so its own unset state is unreachable rather than merely
   unlikely — which is the whole point of the builder.
3. **~~Is the cycle fix in scope?~~ Decided: no — filed and deferred.** There are two cycles, not
   one: besides the manager's back-reference, `AssetData<E>` holds a strong `EnvRef<E>` and the
   manager's `assets` / `query_assets` maps hold those assets, so every cached asset closes a second
   one. Rationale for deferring (user): a typical system holds one environment, or at most one per
   realm, alive for the whole process lifetime, so the leak has no practical cost. A soft reboot that
   rebuilds the environment is the case where it would surface. Tracked as
   `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` (P2). This project keeps the strong back-reference and
   simply does not make the situation worse.
4. **~~What happens to `to_ref` and `EnvRef::new`?~~ Decided: `to_ref` stays public; `EnvRef::new` is
   deprecated.** `EnvRef::new` has exactly one in-tree caller — `to_ref` itself — so deprecating it
   costs nothing. `to_ref` keeps its public signature, and the 336 in-tree call sites keep working
   with no migration.
   The consequence is a requirement, not a free pass: if `to_ref` stays public it stays a door into
   the same readiness hole, so its body must be **reimplemented over the builder path** — construct,
   install, start — and be fully ready on return. Sync startup (question 5) is what makes that
   possible: `fn to_ref(self) -> EnvRef<Self>` can do the whole sequence without changing its
   signature. So `to_ref` becomes a correct shorthand for "build with defaults", and the builder
   becomes the configuration surface for everything else. Phase 2 must confirm no path reaches an
   `EnvRef` except through those two.
   **Refinement to evaluate in Phase 2:** hide `to_ref` from users without touching it, by making the
   built-in environments' *constructors* `pub(crate)` and dropping their public `Default` impls. Since
   `to_ref(self)` consumes an owned environment, a caller who cannot construct one cannot call it, and
   the builder becomes the only source. The types themselves must stay **public and nameable** —
   `register_command!` needs a `CommandEnvironment` type alias, and users write
   `EnvRef<SimpleEnvironment<Value>>` and `Context<E>` in their own signatures — so this is
   constructor visibility, not type visibility.
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
6. **~~Command registration after `build()`?~~ Decided: the barrier must be re-runnable.**
   `POST-INIT-COMMAND-REGISTRATION` (P3) is not blocked by this work and is not closed by it. Its real
   constraint is that registration needs `&mut CommandRegistry` while `to_ref` consumes the
   environment and `Arc::get_mut` never sees a count of 1; its recommended fix is interior mutability
   *inside* `CommandRegistry` (`RefCell` on wasm / `RwLock` on native via the existing
   `MaybeSend`/`MaybeSync` split), which is additive and leaves `get_command_executor(&self) ->
   &CommandRegistry` unchanged. Today `liquers-web` copes by rebuilding the environment and replaying
   declarations, at the cost of the asset cache. Moving the deferred slot to the environment
   (question 2) does not change that: the strong count is still never 1, so this is neither a
   regression nor an improvement.
   What this project *must* get right is the long-term goal of dynamic command registration **and
   command-metadata modification**. Startup snapshots each command's `metadata_version` /
   `impl_version` into the `DependencyManager`; when metadata changes later those versions must be
   re-registered, and `register_version` already does the right thing — a changed version triggers
   `expire_dependents`, which is exactly the cascade that invalidates dependent assets. So the
   machinery for dynamic metadata already exists and only needs re-running.
   Therefore: the startup operation must be **idempotent and re-runnable**, not one-shot. Note
   `ImmediateAssetManager::ensure_started` currently uses `tokio::sync::OnceCell`, i.e. strictly once,
   which would foreclose this. Phase 2 separates the *readiness* flag (has startup happened at least
   once — the guarantee this project delivers) from a re-runnable version-refresh path.
7. **~~Complexity.~~ Decided: M -> L**, applied to the issue front matter and `specs/index.csv`.
   L/XL is what mandates this design folder, so the classification and the artifact now agree.

## References

- `specs/issues/QUEUED-MANAGER-STARTUP-READINESS.md` — the issue (P1, `core/assets`)
- `specs/issues/ENVIRONMENT-MANAGER-REFERENCE-CYCLE.md` — filed during this phase (P2)
- `specs/issues/POST-INIT-COMMAND-REGISTRATION.md` — adjacent P3, constrains question 6
- `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` §gap table, rows P0 and P1
- `liquers-store/src/store_builder.rs` — `StoreRouterBuilder`, the in-tree builder precedent
- `liquers-core/src/context.rs` (four environments, `to_ref`, `init_with_envref`, `EnvRef::new`),
  `liquers-core/src/assets.rs` (`load_command_versions`, `DefaultAssetManager`,
  `ImmediateAssetManager`, `register_plan_dependencies`), `liquers-lib/src/environment.rs`
