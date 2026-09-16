---
id: STALE-DEPENDENCY-PATH-HAS-NO-END-TO-END-TEST
kind: issue
title: The stale-dependency path cannot be reached deterministically from a command, so no test drives it end to end
status: draft
priority: P2
complexity: M
area: [core/assets]
design: stale-dependency-status-finalization
created: 2026-09-15
github:
---

## Problem

`AssetManager::wait_for_dependency`'s expired arm — use the dependency's retained value rather than
recompute, and mark the dependent for recomputation — is the behaviour
`ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY` and `DEPENDENCY-EXPIRED-STALE-VALUE-UNREACHABLE` are
both about. Every test that exercises it calls `wait_for_dependency` **directly**. Nothing reaches
it by running a command.

That is not an oversight in the tests; it is a property of the path. For a command's
`context.get_dependency_state(...)` to observe an expired dependency, the dependency must be
`Expired` *at the moment the parent waits*, and both surrounding moments exclude it:

- **At scheduling**, `get_dependency_asset` evicts and recomputes an already-expired dependency
  (`specs/design/dependency-scheduling/`), so a dependency expired before the parent asks is
  replaced, not used stale.
- **At waiting**, a dependency that is `Ready` returns immediately, so a dependency that expires
  after the parent has read it is never observed.

The window is therefore *between* scheduling and waiting — a race, not a state a test can arrange.
A gate placed inside the parent command cannot open it: a gate after the dependency read (which is
what `test_dependency_expiring_during_parent_evaluation_is_allowed`,
`liquers-core/tests/expiration_integration.rs:748`, does) proves only that the parent **completes**
with the value it already took. That test does not assert the parent becomes `Expired`, and it
would pass whether or not the stale-dependency machinery worked at all.

## Impact

The machinery has real coverage at the unit level — `wait_for_dependency` is a public trait method
and the tests that call it exercise the production code — so this is confidence rather than a known
defect. What is untested is the *reachability* claim: that an ordinary recipe evaluation can arrive
in this state at all.

That matters because the claim has been wrong before. `DEPENDENCY-EXPIRED-STALE-VALUE-UNREACHABLE`
(P0, closed) recorded a period in which the branch was dead code — `poll_state` had been gated and
this caller was not retargeted — and no test noticed, because every test drove the branch directly.
The same blind spot would hide the same regression again.

P2: the behaviour is covered, the gap is in the path to it, and the cost is the risk of a silent
reachability regression rather than a defect today.

## Expected behaviour

Either

1. a test that reaches the expired arm by evaluating a recipe — which needs a deterministic way to
   suspend a parent *between* scheduling a dependency and waiting on it, and no such seam is
   exposed today (`Context::wait_for_dependency` is `pub(crate)`); or
2. an explicit, documented decision that the path is reachable only by a race, with the reasoning
   recorded next to `wait_for_dependency` so the next reader does not assume a missing test rather
   than an unreachable one.

Option 2 is cheap and may be the honest answer. Option 1 would need a test seam, and whether one is
worth adding for this is the decision the issue is asking for.

Worth settling either way: if the window really is only a race, that is a statement about how often
this code path runs in production, and it belongs in `ASSETS.md` beside the policy it implements.

## Discovery

Found on 2026-09-15 while implementing `stale-dependency-status-finalization`. Its Phase 3 planned
an end-to-end integration test (`I2`) built on the gate test cited above, and the early assertion
that design required — "assert the parent is `Expired` early so a missed window fails there" —
fired immediately: the parent was `Ready`, because the gate had been placed after the dependency
read and could not be placed before it. The test was rewritten as a unit test driving
`wait_for_dependency` directly, and this issue records what that substitution costs.
