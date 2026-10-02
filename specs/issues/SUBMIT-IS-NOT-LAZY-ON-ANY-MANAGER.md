---
id: SUBMIT-IS-NOT-LAZY-ON-ANY-MANAGER
kind: issue
title: Context::submit starts the dependency at once on both managers, contrary to the Phase 2 design text
status: draft
priority: P3
complexity: S
area: [core/assets]
created: 2026-10-02
github:
---

## Problem

`phase2-architecture.md` (Part E) says that `Context::submit` "does not drain" and that on the
immediate (inline) manager "the dependency runs when it is first waited for, so a command can submit
several dependencies before any of them runs". Neither half holds at HEAD:

- `ImmediateAssetManager` does not override `get_dependency_asset`; the trait default calls
  `get_asset`, which evaluates the asset inline. The dependency is `Ready` before `submit` returns.
- `DefaultAssetManager::get_dependency_asset` calls `job_queue.try_to_start_immediately`; only when
  there is no capacity does it park the dependency on the parent's local queue, to run at the
  wait. With capacity it starts at once.

## Impact

No defect today: the stale-dependency end-to-end tests pass on both managers, and a command that
submits several dependencies gets real concurrency on the queued manager. But the design text and
the doc comments that repeat it mislead a reader into expecting laziness (and an ordering guarantee
such as "nothing ran yet, so I can still expire it") that does not exist. Pinned by
`context::tests::submit_runs_dependency_at_once_on_inline_manager` and
`submit_returns_before_the_dependency_finishes_on_queued_manager`.

## Expected behaviour

Either correct the design text and `ASSETS.md` to say what the managers do, or give
`ImmediateAssetManager` a lazy `get_dependency_asset` (schedule only, evaluate on `wait_for_dependency`)
so the inline manager has the same observable ordering as a saturated queued manager. The first is
cheaper; the second changes inline evaluation order for every dependency.

## Discovery

Found while writing the Step 10 unit test `submit_does_not_run_on_inline_manager_until_waited`, which
failed on the first run.
