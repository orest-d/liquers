# Phase 1: High-Level Design - Non-Blocking Dependency Evaluation

> **Anchor note.** This Phase 1 document was reconstructed from `plan20260707.md`
> (WP-1 "Phase 2 — scheduler and delegation rework") and `specs/DEPENDENCIES_STATUS.md`
> to give the Phase 2 architecture a formal basis. It captures the *remaining* work of
> WP-1 Phase 2; the dependency-recording semantics (WP-1 Phase 1) and the event-driven
> `JobQueue` (WP-1 Phase 2C) are already implemented on `main`.

## Feature Name

Non-Blocking Dependency Evaluation (`dependency-eval-blocking`)

## Purpose

Make recipe delegation and dependency waiting **release the parent's job-queue slot**
instead of blocking it. Today a parent asset that delegates to a child (pure-key recipe
delegation) enters `Status::Dependencies` but then **runs the child inline inside the
parent's own job** (`assets.rs:1447-1455`) as a deadlock guard. This holds one queue slot
for the entire depth of the delegation chain, couples parent and child lifetimes, and only
works because the inline call bypasses the scheduler. The feature replaces inline blocking
with a **suspend/resume** model: a delegating parent returns from its job, frees its slot,
and is **resubmitted** by a one-shot completion hook once the child reaches a terminal
status.

## Core Interactions

### Query System
No changes. No new queries, no changes to parsing or `Key` encoding.

### Store System
No changes. Persistence of ready assets is unaffected; `waiting_on` is never persisted.

### Command System
No changes. No new commands or namespaces. Runtime dependencies discovered via
`Context::evaluate()` are already recorded (WP-1 Phase 1) and are out of scope here.

### Asset System (primary)
- `AssetRef::evaluate_recipe()` / `evaluate_and_store()` / `run()` gain an internal
  `EvaluationOutcome` type distinguishing `Completed(State)` from `Delegated(child)`.
- A delegating parent: records the dependency (existing `record_dependency_on_asset`),
  enters `Status::Dependencies` (existing `enter_dependencies`), ensures the child is
  submitted, then **returns from the job** so the slot is freed.
- `AssetData` gains a non-persisted `waiting_on` bookkeeping field plus a wait-generation
  counter to make resumption idempotent and to reject stale completion hooks.
- The asset manager registers a **one-shot completion hook** on the child; on the child's
  terminal status it calls `leave_dependencies_for_resubmit()` (existing) and resubmits the
  parent to the `JobQueue`. On re-entry the parent re-checks child status rather than
  trusting hook state.

### Value Types
No new `ExtValue` variants or value extensions.

### Web/API
No changes. The observable effect via existing metadata/status endpoints is that a
delegating parent is reported as `Status::Dependencies` with a freed queue slot, rather than
an invisible `Processing` parent.

### UI
No changes.

### Scheduler / JobQueue
No new queue mechanism (the event-driven `Notify`-based `JobQueue`, `with_capacity`, and
`shutdown` already exist). Resumption reuses `JobQueue::submit()`, whose duplicate guard
already makes re-submission idempotent.

### Dependency Graph
Unchanged as the source of truth. `waiting_on` is diagnostic-only, never persisted, never a
second dependency model. Cycle detection stays in `DependencyManager` / `find_dependencies()`.

## Crate Placement

**`liquers-core`** — the entire feature lives here:
- `liquers-core/src/assets.rs` — `EvaluationOutcome`, `waiting_on`/wait-generation on
  `AssetData`, non-blocking delegation in `evaluate_recipe`/`evaluate_and_store`/`run`,
  completion hooks + parent resubmission in `DefaultAssetManager`.

No other crate is touched. Rationale: delegation, the job queue, and asset lifecycle are all
core concerns; `liquers-lib`/`liquers-axum`/`liquers-py` observe the behavior only through
existing status/metadata APIs.

## Open Questions

1. Completion-hook transport: dedicated manager-owned watcher task per wait vs. reusing the
   child's `watch::Receiver<AssetNotificationMessage>` subscription. (Resolved in Phase 2:
   reuse `subscribe_to_notifications()`.)
2. Whether `Context::evaluate()`'s runtime-dependency wait path (Flow B) should adopt the
   same suspend/resume model, or remain a direct `child.get().await` since the calling
   command already occupies a slot by design. (Resolved in Phase 2: out of scope — only
   pure-key recipe delegation blocks a slot without doing useful work.)

## References

- `plan20260707.md` — WP-1 Phase 2A/2B/2D (2C already done)
- `specs/DEPENDENCIES_STATUS.md` — current delegation flows (Flow A step 5 is the inline guard)
- `liquers-core/src/assets.rs` — `evaluate_recipe` (1412), `enter_dependencies` (748),
  `leave_dependencies_for_resubmit` (767), `JobQueue` (3483)
