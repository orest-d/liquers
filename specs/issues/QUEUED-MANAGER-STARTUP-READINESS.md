---
id: QUEUED-MANAGER-STARTUP-READINESS
kind: issue
title: Queued asset manager accepts work before it is ready to run it
status: accepted
priority: P1
complexity: M
area: [core/assets]
design: 
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
