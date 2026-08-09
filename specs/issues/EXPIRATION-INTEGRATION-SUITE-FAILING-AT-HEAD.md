---
id: EXPIRATION-INTEGRATION-SUITE-FAILING-AT-HEAD
kind: issue
title: Five expiration_integration tests fail at HEAD
status: draft
priority: P1
complexity: M
area: [core/assets]
design:
created: 2026-08-09
github:
---

## Problem

`cargo test -p liquers-core --test expiration_integration` reports **27 passed, 5 failed** on a
clean checkout, with no local changes. Expiry is not taking effect: every failure is an assertion
that something should have been recomputed or marked `Expired` and instead was served from cache.

| Test | Line | Assertion |
|---|---|---|
| `test_dependent_expiration` | `:348` | status `Ready`, expected `Expired` |
| `test_expired_dependency_is_recomputed_before_dependent_evaluation` | `:726` | `"parent(1)"`, expected `"parent(2)"` |
| `test_expired_keyed_asset_does_not_fast_track_back` | `:1344` | `Ready`, expected `Expired` — *"expiry must be persisted, or an evicted asset reloads as fresh"* |
| `test_get_any_status_has_no_side_effects_on_normal_get` | `:1103` | `"1"`, expected `"2"` |
| `test_manager_re_request_still_rebuilds_after_gate` | `:1250` | `[49]`, expected `[50]` |

The shape is consistent: a value that should have been recomputed after expiry is returned
unchanged. Four of the five compare a generation counter or a status that only advances on
recompute.

## Impact

Unknown, and that is the problem. Either

- **expiry is broken in the library**, in which case these are five true reports of a P0-shaped
  defect — assets served stale past their expiration — and the priority here is wrong; or
- **the tests encode expectations the implementation deliberately moved away from**, in which case
  they are stale and are costing every contributor a red suite.

Nothing in the tree says which. That ambiguity is itself the cost: a permanently red suite trains
readers to ignore it, and the next real expiry regression lands invisibly. Triage should establish
which of the two it is before anything else.

## Reproduction

```bash
cargo test -p liquers-core --test expiration_integration
```

No setup, no feature flags, no network. Reproduced at commit `94ee1cb` and at every commit tested
above it.

## Discovery

Found on 2026-08-09 while implementing `specs/design/keyed-recipe-ownership/`. That design named
this suite as a watch list — it exercises the runtime-volatility path the design touches — so the
failures were checked against the pre-change tree to establish whether the design had caused them.
It had not: the same five fail identically with the working tree reverted, so they predate the work
entirely and are recorded here rather than absorbed into it.

Note for whoever picks this up: `CLAUDE.md`'s documented loop is
`cargo test -p liquers-lib --lib --tests`, which does not run this suite. That is a plausible
explanation for how five failures went unnoticed, and an argument for the loop including
`-p liquers-core --lib --tests`.
