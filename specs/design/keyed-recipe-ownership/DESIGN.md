---
id: KEYED-RECIPE-OWNERSHIP
kind: design
title: Non-evaluating ownership test for keyed recipes
status: complete
area: [core/assets, web]
gh_pr: []
issues: [CORE-IMMEDIATE-MANAGER-KEYED-RECURSION, VOLATILE-KEYED-RECIPE-SELF-DELEGATION]
created: 2026-08-09
superseded_by:
---
# keyed-recipe-ownership Design Tracking

**Created:** 2026-08-09

## Phase Status

- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution & Architecture
- [x] Phase 3: Examples & Testing
- [x] Phase 4: Implementation Plan
- [x] Implementation Complete

## Notes

Fixes two P1 issues on the same line of `AssetRef::evaluate_recipe`
(`liquers-core/src/assets.rs:1833`): the wasm stack-exhaustion recursion under
`ImmediateAssetManager`, and the spurious dependency cycle for volatile keyed recipes. The
regression guard is five `test.fixme` cases in `liquers-web/tests/e2e/store.spec.ts` plus a new
wasm keyed-evaluation test.

## Implementation outcome

Landed in six commits on `claude/core-immediate-manager-recursion-p9ll0u`, steps 1-9 of
`phase4-implementation.md`. Three things went differently from the plan, all recorded rather than
smoothed over:

1. **T2 could not assert what it was written to assert.** `create_asset` turned out to be inherent
   to `DefaultAssetManager`, not on the trait, so the scenario uses `AssetManager::apply` — the
   fallback Phase 3 named. It then found that the delegation branch *cannot succeed at all*: it is
   only reached when the delegate is registered under the caller's own key, so
   `record_dependency_on_asset` always sees a self-edge. The test now pins branch selection and
   asserts the known-broken outcome, with instructions to invert it.
   Filed as `ASSET-KEYED-DELEGATION-ALWAYS-CYCLES`.
2. **The command counter is not always a recompute signal.** For a key whose stored copy is
   `Ready`, an evicted asset fast-tracks the value from the store instead of re-running. T12 asserts
   the eviction; `volatile_keyed_recipe_recomputes_every_time` carries the counter, because a
   volatile result persists as `Status::Volatile` and `try_fast_track` refuses it.
3. **The re-entrancy guard was implemented and then reverted.** It broke
   `liquers-web/tests/async_ASYNCQ.rs`, which passes before the change and fails after: a
   manager-global id set cannot distinguish re-entrancy on one stack from two tasks legitimately
   awaiting the same asset, and a JavaScript `async` command yields, so the second caller was
   refused. Phase 4's rollback plan anticipated exactly this and authorised reverting step 6 alone;
   the recursion is fixed by step 4 regardless. The evidence is recorded in
   `INLINE-PATH-LACKS-EXECUTE-ONCE`, which owns the correct fix — the second caller must *wait*,
   not be turned away, and that is execute-once work.
4. **Two unrelated defects surfaced and were filed, not absorbed** —
   `ASSET-MANAGER-INSERT-KEY-ASSET-NO-OVERWRITE` and
   `EXPIRATION-INTEGRATION-SUITE-FAILING-AT-HEAD`.

The recursion reproducer was verified: with the ownership test temporarily reverted,
`keyed_eval_immediate` aborts with `stack overflow` (SIGABRT), which is why it had to land in the
same commit as the fix.

**Verification status:** `liquers-core` 506 passed, `liquers-lib` 368 passed, `registry_export`
green. The five `expiration_integration` failures are pre-existing and unchanged. The wasm and
Playwright loops are covered in the branch's final report.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
