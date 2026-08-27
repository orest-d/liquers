# Phase 1: High-Level Design - Asset Manager Startup Readiness

## Feature Name

Asset Manager Startup Readiness (resolves `QUEUED-MANAGER-STARTUP-READINESS`)

## Purpose

`Environment::to_ref` returns an `EnvRef` while `AssetManager::start` may still be running in a
detached task, so the first evaluations can observe an empty dependency manager. Because
`register_plan_dependencies` skips any dependency whose version is not yet registered, edges lost in
that window are lost silently and permanently: the affected assets never expire when a command
changes. This project gives startup one observable, fallible, idempotent completion boundary that
every evaluation entry point respects.

## Core Interactions

### Query System
No change to parsing, planning or `Key` encoding. Command-metadata and command-implementation
dependency keys emitted by `Plan` (`plan.rs`, `find_dependencies`) are the state that must be ready
before a plan's dependencies are registered.

### Store System
None. Startup reads only the command metadata registry; no store is opened.

### Command System
No new commands and no namespace change. `CommandMetadataRegistry` is the *input* to startup:
`load_command_versions` turns each command's `metadata_version` / `impl_version` into
`DependencyManager` versions. Interacts with `POST-INIT-COMMAND-REGISTRATION` (P3), which wants
commands registered after `to_ref` — a re-runnable barrier should not foreclose that.

### Asset System
Central. Adds a readiness operation to the `AssetManager` contract, awaited by the public evaluation
entry points (`get_asset`, `get`, `apply`, `apply_immediately`, keyed mutation). Affects
`DefaultAssetManager` (queued, eager, spawned) and `ImmediateAssetManager` (inline, already lazy and
idempotent via `ensure_started`); the goal is one shared guarantee, not one shared execution model.

### Value Types
None.

### Web/API
No new endpoints. `liquers-axum` and `liquers-web` construct environments through `to_ref`, so any
signature change there is a breaking change for them — a constraint on the Phase 2 choice, not a
feature.

### UI
None.

## Crate Placement

**liquers-core** — `src/assets.rs` (`AssetManager` contract and both managers), `src/context.rs`
(`Environment::init_with_envref`, the four built-in environments). **liquers-lib** —
`src/environment.rs` (`DefaultEnvironment`, whose native branch spawns `start`). No change expected
in `liquers-store`, `liquers-axum`, `liquers-web` or `liquers-py` beyond compiling against the
contract; `liquers-py`'s `init_with_envref` is `todo!()` and stays out of scope.

## Documentation Intent

**Reference:** Extend, do not create. `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`
owns the initialization sequence, already records this defect as a P1 gap row, and must state the new
guarantee and retire that row. `specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` owns the
manager lifecycle primitives and must describe the readiness operation alongside `set_envref` and
`start`. A new reference would split one lifecycle across three documents.

**Guide:** Extend `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`. An integrator implementing
`Environment` for a host language needs to know what its `init_with_envref` must guarantee; that is a
repeatable task, and the guide already covers this seam. No new guide — there is no workflow here
beyond "implement the hook correctly".

**Other documents to create:** None. The change is a contract tightening, not a new capability.

**Specific documents to update:** `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`
(initialization sequence, gap table), `specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`
(manager lifecycle primitives), `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` (integrator obligation),
`specs/README.md` (design folder link), `specs/index.csv`, and
`specs/issues/QUEUED-MANAGER-STARTUP-READINESS.md` (status at Phase 5).

Audience: framework maintainers and language integrators. After this project they should be able to
tell, without reading the design folder, when an `EnvRef` is safe to evaluate against and what a
custom `Environment` or `AssetManager` owes that contract.

## Open Questions

1. Which fix direction? A barrier awaited by evaluation entry points (issue direction 2) keeps
   `to_ref` synchronous and infallible and so does not break `liquers-web`, `liquers-axum` or the
   examples; an async/fallible `to_ref` (direction 1) is a stronger guarantee at a much wider blast
   radius. Phase 2 decides, with a bias toward direction 2.
2. Where does a startup *failure* surface? `load_command_versions` and both `start` implementations
   are infallible today (`async fn start(&self)`), so "propagate startup failure" implies making the
   contract fallible. Is that in scope, or is it a follow-up once a startup step can actually fail?
3. Is the barrier re-runnable after later command registration, or strictly once
   (`OnceCell`)? This decides whether `POST-INIT-COMMAND-REGISTRATION` stays solvable.
4. Which entry points must await it? Every `AssetManager` method, or only those that read
   startup-dependent state — and is the resulting per-call cost acceptable on the hot path?
5. Does the queued manager keep its eager spawned `start` as a warm-up alongside the barrier, or
   drop it in favour of purely lazy startup as `ImmediateAssetManager` does?

## References

- `specs/issues/QUEUED-MANAGER-STARTUP-READINESS.md` — the issue (P1, complexity M, `core/assets`)
- `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` §gap table, row P1 "Manager startup
  completion is not observable for queued environments"
- `specs/issues/POST-INIT-COMMAND-REGISTRATION.md` — adjacent P3 constraining question 3
- `specs/design/dependency-management/` — where `load_command_versions` and the call from `to_ref`
  originate
- `liquers-core/src/assets.rs` (`load_command_versions`, `DefaultAssetManager::start`,
  `ImmediateAssetManager::ensure_started`, `register_plan_dependencies`),
  `liquers-core/src/context.rs` (`to_ref`, `init_with_envref`), `liquers-lib/src/environment.rs`
