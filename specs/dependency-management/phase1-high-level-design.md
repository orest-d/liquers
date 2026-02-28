# Phase 1: High-Level Design - Dependency Management System

## Feature Name

Dependency Management System (Revised)

## Purpose

Redesign and extend the existing dependency management in `liquers-core` to support versioned, typed, reason-annotated dependency tracking. Enables precise cache invalidation, cycle detection, plan-level pre-analysis, and metadata visibility of the full dependency graph. The dependency manager is progressively re-created from persistent metadata as assets load.

## Core Interactions

### Query System
Dependency keys are strings parseable as valid queries (query-compatible format):
- Asset data/metadata: `-R/<key>`
- Recipe: `-R-recipe/<key>` (consistent with `-R-recipe` plan header)
- Command metadata: `ns-dep/command_metadata-<realm>-<namespace>-<command_name>`
- Command implementation: `ns-dep/command_implementation-<realm>-<namespace>-<command_name>`
- Directory listing (reserved): `-R-dir/<key>`

Two commands in the `dep` namespace exist and return a versioned object: `command_metadata` and `command_implementation`.

### Store System
No new store implementations. `DependencyManager` is in-memory, progressively reconstructed as assets are loaded or created. Dependencies of persistent assets are serialized into `MetadataRecord` so they survive restarts.

### Command System
- New `dep` namespace with `command_metadata` and `command_implementation` commands (return versioned objects enabling dependency tracking on commands).
- `context::evaluate()` gains cycle-check and merges runtime dependencies with plan-level dependencies; irresolvable inconsistency raises an error.

### Asset System
- `AssetManager` owns (or holds a reference to) `DependencyManager`.
- `AssetManager` is responsible for maintaining `DependencyManager` state and detecting cascade expiration triggers: setting a value, removing a value, or a newly produced `Ready` asset with a new version.
- Deleting a stored value for an asset that has a recipe (i.e., was `Ready`) does **not** trigger cascade expiration; only a new value with a new version does.
- Unfinished assets (`Partial`, `Processing`, `Submitted`, `Dependencies`, etc.) **cannot** be tracked dependencies.
- Volatile assets are excluded entirely: they cannot be tracked dependencies and are not registered as dependents.
- Plan-level dependencies form the initial dependency set (without versions). Runtime dependencies (via `context::evaluate`) are merged; inconsistency is an error.

### Value Types
No new value types. `DependencyRecord` lists added to `MetadataRecord`. `dep` namespace commands return versioned object values.

### Web/API
No new endpoints. Dependency data visible via existing metadata endpoints.

### UI
N/A for this feature.

## Crate Placement

**`liquers-core`** — primary:
- `liquers-core/src/dependencies.rs` — `Version`, `DependencyKey`, `DependencyReason`, `DependencyRecord`, `DependencyManager`
- `liquers-core/src/metadata.rs` — extend `MetadataRecord` with dependency list
- `liquers-core/src/assets.rs` — `AssetManager` owns `DependencyManager`; cascade expiration logic
- `liquers-core/src/context.rs` — `Context::evaluate()` cycle check + dependency merge

**`liquers-lib`** — `dep` namespace commands (`command_metadata`, `command_implementation`)

## Design Decisions (Resolved)

1. `DependencyKey` is a newtype wrapping `String` with explicit conversions to/from `Query` and `Key`.
2. The relation enum is named `DependencyRelation` with variants:
   - `StateArgument` — input state entering the action
   - `ParameterLink(name)` — parameter that links to another asset
   - `DefaultLink(name)` — default value link
   - `RecipeLink(name)` — recipe-link parameter
   - `OverrideLink(name)` — override-link parameter
   - `EnumLink(name)` — enum-link parameter (mirrors `ParameterValue` enum)
   - `ContextEvaluate(query)` — dependency created via `context::evaluate`
   - `CommandMetadata` — dependency on command metadata
   - `CommandImplementation` — dependency on command implementation
   - `Recipe` — dependency on the asset recipe itself
3. Plan-level dependencies carry only `DependencyKey` + `DependencyRelation` (no version). `DependencyRelation` is tracked **only in the Plan** — neither `DependencyManager` nor `MetadataRecord` store it. For plan-known dependencies, the relation can be recovered by re-creating and inspecting the plan. Exception: `ContextEvaluate(query)` dependencies arise dynamically at runtime and are not present in the plan, so their relation is not recoverable after execution (accepted limitation). Versions are resolved at execution time via the asset manager cache. If an asset expires mid-evaluation causing a version inconsistency, `DependencyManager` rejects the stale dependency, the evaluation is marked failed, and `AssetManager` retries a configurable number of times.

## Open Questions

None — all design questions resolved.

## References

- `liquers-core/src/dependencies.rs` — current skeletal `DependencyManagerImpl`
- `liquers-core/src/plan.rs` — `find_dependencies()` pre-execution analysis
- `liquers-core/src/metadata.rs` — `MetadataRecord`, `Status`
- `specs/ASSETS.md` — asset lifecycle
- `specs/todo20260219.md` — `CORE-ASSETS-RECIPE-DELEGATION-DEADLOCK`
