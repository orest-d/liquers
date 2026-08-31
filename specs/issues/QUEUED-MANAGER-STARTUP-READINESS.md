---
id: QUEUED-MANAGER-STARTUP-READINESS
kind: issue
title: Queued asset manager accepts work before it is ready to run it
status: closed
priority: P1
complexity: L
area: [core/assets]
design: environment-builder
created: 2026-08-08
github:
---
## Problem

Initialization of a queued asset manager has no observable completion boundary.

`Environment::to_ref` calls the synchronous
`Environment::init_with_envref` hook and then returns `EnvRef`. In the built-in
native queued environments, `init_with_envref`:

1. Installs the environment back-reference with `AssetManager::set_envref`.
2. Spawns `AssetManager::start` as a detached Tokio task.
3. Returns without waiting for `start` to finish.

`DefaultAssetManager::start` loads command metadata and implementation versions
into the dependency manager. A caller can begin evaluation as soon as `to_ref`
returns, while that loading task may still be in progress. There is no readiness
future, state query, or error result through which application code can determine
that startup has completed.

This is separate from construction of `DefaultAssetManager`: its job-queue and
expiration-monitor tasks are already spawned by the manager constructor. The
unobservable startup phase discussed here is the environment-dependent
initialization performed by `AssetManager::start`.

The race is especially relevant to dependency-version registration and cache
validation. The current API does not establish whether evaluation is allowed to
observe a partially initialized dependency manager.

## Expected behavior

Environment initialization should provide one documented guarantee:

1. `Environment::to_ref` does not expose an environment until required manager
   startup has completed; or
2. Every evaluation entry point awaits an idempotent manager-startup barrier before
   reading startup-dependent state.

Startup failures should be returned to the caller rather than being confined to a
detached task. Multiple concurrent first evaluations must share one startup
operation.

`ImmediateAssetManager` already uses lazy, idempotent startup through its internal
`ensure_started` path. The queued and inline managers should expose equivalent
readiness semantics even if their execution models remain different.

## Fix direction

Consider one of:

1. Make environment initialization asynchronous and fallible.
2. Add a fallible `ensure_started` operation to the `AssetManager` contract and
   invoke it from all public evaluation entry points.
3. Return an initialization handle from `Environment::to_ref` that must be awaited
   before evaluation.

Avoid relying on task scheduling order between the detached `start` task and the
first evaluation.

## Verification

Add tests covering:

1. Evaluation immediately after `Environment::to_ref`.
2. A command whose metadata and implementation versions must be registered during
   startup.
3. Multiple concurrent first evaluations sharing one startup operation.
4. Startup failure propagation.
5. Equivalent readiness guarantees for `DefaultAssetManager` and
   `ImmediateAssetManager`.
6. Native queued execution and the Wasm-compatible inline path.

## Resolution (2026-08-31)

Closed by `design/environment-builder/`. Expected behaviour **1** was chosen: `Environment::to_ref`
does not expose an environment until manager startup has completed.

`Environment::try_to_ref` now owns a single readiness sequence — refresh command metadata versions,
create the `EnvRef`, then call `init_with_envref`, which constructs the asset manager with that
reference, installs it, and starts it. No reference escapes before startup finishes.
`EnvironmentBuilder::build` delegates to the same sequence rather than reimplementing it, so both
construction paths carry one guarantee with one implementation, and `to_ref` keeps its signature
while becoming correct.

`AssetManager::start` is synchronous and fallible. Its only reason to be async was
`scc::HashMap::entry_async`; the work is uncontended in-memory map writes and touches no store, so
`register_version_sync` (built on `entry_sync`) replaces it. `set_envref` is gone — both managers
take the `EnvRef` at construction — along with the two "environment not set" panics it required.

Verification items, all covered:

| Item | Where |
|---|---|
| 1. Evaluation immediately after `to_ref` | `manager_parametric::ready_on_return_{default,immediate}` |
| 2. A command whose versions must be registered during startup | `environment_builder::tests::command_version_present_immediately_after_build` |
| 3. Concurrent first evaluations sharing one startup | `manager_parametric::concurrent_first_evaluations_{default,immediate}` |
| 4. Startup failure propagation | `tests/environment_builder.rs::startup_failure_propagates_from_build` |
| 5. Equivalent readiness for both managers | `manager_parametric::ready_on_return_*`, run over both |
| 6. Native queued and wasm-compatible inline | the same parametric pair, plus `inline_builds_without_a_tokio_runtime` |

The defect itself is pinned as a differential: `plan_dependencies_registered_immediately_after_build`
asserts the edge now forms, and `an_unregistered_dependency_version_registers_no_edge` reproduces
the failure mode on a key with no registered version — showing that
`register_plan_dependencies` skips silently, which is why the fix had to be a construction-time
guarantee rather than a check.

`dependency_manager_integration.rs` no longer sleeps 50 ms before asserting that versions loaded.

Two things this did **not** do, both deliberate: the `Arc` cycles remain
(`ENVIRONMENT-MANAGER-REFERENCE-CYCLE`, deferred by decision), and registering a command after
construction still needs a rebuild (`POST-INIT-COMMAND-REGISTRATION`) — though
`refresh_command_versions` is now the re-runnable hook that work will need.
