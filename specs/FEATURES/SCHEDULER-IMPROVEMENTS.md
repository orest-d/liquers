# SCHEDULER-IMPROVEMENTS

Status: Draft

## Summary
`SCHEDULER-IMPROVEMENTS` captures scheduler/runtime changes needed to make dependency delegation robust in `liquers-core/src/assets.rs`.

Primary motivation: remove deadlock-prone behavior where a running parent asset blocks on a delegated dependency while still holding queue capacity.

## Motivation
`ASSETS-FIX1` issue #17 identifies a liveness bug in delegation:
- `evaluate_recipe()` delegates by calling `asset.get().await`.
- The parent can already be running inside a queue worker.
- If queue capacity is saturated, delegated child cannot run, and parent cannot finish.

This is a scheduler-level correctness issue, not only a local logging or status issue.

## Findings (Scheduler Review)
1. Delegation deadlock risk is real and deterministic:
   - Parent blocks in `evaluate_recipe()` on delegated `asset.get().await`.
   - Queue slot is released only when parent `run()` returns.
   - With low capacity (e.g. 1), child may be submitted but never started.
2. Current queue model tracks only queued/running, not dependency wait:
   - No scheduler state for "waiting on dependencies".
   - No mechanism to release capacity while waiting and resume later.
3. Cycle handling is incomplete:
   - Only direct self-delegation is guarded (`asset.id() == self.id()`).
   - Multi-asset dependency cycles remain unresolved.
4. Queue execution strategy is polling-based:
   - Worker loop scans submitted jobs periodically.
   - This is simple but weak for responsiveness and fairness under load.
5. Missing dedicated tests for dependency-scheduler liveness:
   - No regression case proving delegated dependency execution with constrained capacity.

## Goals
1. Remove delegation deadlock class under bounded queue capacity.
2. Make dependency waiting explicit in both scheduler state and asset status.
3. Preserve single-submit semantics for delegated dependencies.
4. Provide deterministic tests for liveness and dependency transitions.

## Non-goals
1. Full execution-engine rewrite.
2. Command class policy redesign (`fast`/`slow`/`default`) tracked in `EXTENDED-FAST-TRACK`.
3. Changing recipe semantics or key/query identity rules.

## Proposed Direction
Minimal robust redesign (recommended):
1. Add dependency-aware scheduler states:
   - `Queued`
   - `Running`
   - `WaitingDependencies`
   - `Finished`
2. When parent must wait on delegated dependency:
   - transition parent to `WaitingDependencies` (`Status::Dependencies`);
   - release parent running capacity;
   - register parent->child edge in scheduler bookkeeping.
3. On child completion:
   - wake/requeue parent;
   - parent resumes once capacity is available.
4. Add cycle detection on dependency edges:
   - fail fast with `Error` status and diagnostic metadata/log.

## Why Redesign Helps
Without scheduler changes, local fixes in `evaluate_recipe()` cannot guarantee liveness because blocking waits can still consume scarce capacity.
Dependency-aware suspension/resume directly removes the deadlock condition and gives a clear runtime model for dependency blocking.

## Acceptance Criteria
1. With queue capacity `1`, delegated evaluation completes (no deadlock).
2. Parent transitions to `Status::Dependencies` while waiting.
3. Delegated child is submitted at most once.
4. Parent resumes and reaches terminal status after child completion.
5. Cycle case returns deterministic error (no indefinite wait).
6. New tests are deterministic and documented with brief rationale comments.

## Suggested Tests
1. `delegation_capacity_one_no_deadlock`
2. `parent_enters_dependencies_and_resumes`
3. `delegation_submitted_once`
4. `dependency_cycle_fails_fast`

## Related Features
- Source issue tracking: [ASSETS-FIX1.md](/home/orest/zlos/rust/liquers/specs/FEATURES/ASSETS-FIX1.md)
- Queue class policy (separate scope): [EXTENDED-FAST-TRACK.md](/home/orest/zlos/rust/liquers/specs/FEATURES/EXTENDED-FAST-TRACK.md)
