# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** Repository evidence settles the choice. The current eager behaviour is pinned
  by two tests and was already adopted by the code documentation and `DEPENDENCIES_STATUS.md`.
  Only one inaccuracy remains: the saturated queued manager parks the dependency, and neither
  text says so.
- **Open questions:** None. Making the inline manager lazy (the issue's second option) is not
  pursued. It would change inline evaluation order for every dependency, and nothing needs it.

## Problem

The frozen design `dependency-audit-and-expiry-provenance` (Phase 2, Part E) says that `submit`
"does not drain" and that the inline manager runs a dependency "when it is first waited for". At
HEAD:

- `ImmediateAssetManager` uses the trait default `get_dependency_asset` → `get_asset`, which
  evaluates inline. The dependency is finished before `submit` returns.
- `DefaultAssetManager::get_dependency_asset` calls `job_queue.try_to_start_immediately`. With
  capacity it starts at once. Without capacity it parks the dependency on the parent's local
  queue, and it runs when the parent waits or drains.

Since the issue was filed, `Context::submit`'s doc comment (`liquers-core/src/context.rs`) and
`specs/reference/DEPENDENCIES_STATUS.md` §2 were corrected to "not lazy". Both still say, or
imply, that the dependency "has started" on the queued manager. Under saturation that is false.

## Expected behaviour

Documentation states all three cases: inline manager — finished on return; queued manager with
capacity — started; queued manager without capacity — queued locally, started at the first
`wait_for_dependency` / `evaluate` drain. No behaviour changes.

Acceptance: the doc comment and the reference say the three cases. The two existing pinning tests
stay. One new test pins the saturated case.

## Scope

Documentation, plus one test. Frozen design documents are not edited (§5.1); the correction is
recorded here and in the issue's resolution.

## Design Dependencies

- `dependency-audit-and-expiry-provenance` — **overlaps** (complete; its Part E text is the stale
  source).

## Documentation assessment

- Reference: update `specs/reference/DEPENDENCIES_STATUS.md` §2 (one sentence).
- Code docs: `Context::submit` in `liquers-core/src/context.rs`.
- No guide.

## Consolidated Findings

- The saturated case needs a queue capacity of 1 that is occupied by the parent. The existing
  queued-manager tests construct `DefaultAssetManager` with a configurable capacity; the new test
  reuses that.
- After this change the issue can close. No behaviour work is left.
