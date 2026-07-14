# Phase 1: High-Level Design - dependency-scheduling

## Feature Name

Non-blocking dependency scheduling (asset-local dependency queues)

## Purpose

Evaluating a dependency from a running asset can deadlock: the parent occupies a JobQueue
slot while its child waits parked for a slot (F-1 / ASSETS-FIX1 #17). This feature makes
dependency evaluation deadlock-free: a dependency starts immediately when capacity allows
(parallel), otherwise it is placed on an asset-local queue of the dependent asset and
evaluated sequentially inline from the parent's own spawned future — zero extra slots.

## Core Interactions

### Query System
No changes to query syntax, parsing, or Key encoding. Dependencies remain expressed as
queries/keys (parameter links, GetAsset* / Evaluate steps, `context.evaluate`).

### Store System
No changes. Store access (`GetResource*`) is unaffected and remains outside asset
dependency control, as documented.

### Command System
No new commands. Commands gain a new Context API: `schedule_dependency(query)` returning
a `DependencyHandle`, plus `get_dependency_state(query)` (schedule + wait) and
`evaluate_local_queue()`. `Context::evaluate` stays backwards-compatible on top of it.

### Asset System
Main integration point. `JobQueue::submit` is refactored over a new
`try_to_start_immediately`; an atomic run-claim guarantees each asset body runs at most
once; each `AssetData` gains a local dependency queue drained inline by its own future.
The interpreter schedules all known plan dependencies first, then executes steps using
the captured handles. Volatile dependencies are resolved once per parent evaluation
(handle capture), guaranteeing execute-once. Cycle detection stays in DependencyManager;
a defensive inline-run re-entry guard fails fast instead of hanging (to be confirmed).
`Status::Dependencies` remains the sole waiting status (WP-1); the wait resumes the
parent as `Processing`. Supersedes WP-1 Phase 2A (slot-release/resubmit design).

### Value Types
None.

### Web/API (if applicable)
No new endpoints. Status flow `Processing → Dependencies → Processing` is observable.

### UI (if applicable)
No new UI; asset views see the same status flow change.

## Crate Placement

`liquers-core` only (`assets.rs`, `context.rs`, `interpreter.rs`); pure scheduler/runtime
mechanics, no rich types. `cargo check -p liquers-py` guards binding compatibility.

## Open Questions

1. Confirm the defensive inline-run cycle guard (assumed chosen; the answer was lost in a
   tool error). It is diagnostic-only; DependencyManager stays authoritative.
2. Leftover never-awaited local-queue entries at parent finish: hand off to the global
   JobQueue (current design) or drop with a warning? (Resolved in Phase 2.)
3. Exact placement of `DependencyHandle` (assets.rs vs a small new module). (Phase 2.)

## References

- `plan20260707.md` WP-1 (F-1, F-7); `review20260707.md` §3.1
- `specs/FEATURES/ASSETS-FIX1.md` issue #17 (deadlock walkthrough)
- `specs/FEATURES/SCHEDULER-IMPROVEMENTS.md`, `specs/DEPENDENCIES_STATUS.md`
- `specs/JOBQUEUE_FIX.md`, `specs/dependency-management/DESIGN.md`
- Design research and decisions: plan session 2026-07-14 (approved plan)
