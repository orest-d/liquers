---
id: EVALUATION-SESSION-VOLATILE-TEST-INTERMITTENT
kind: issue
title: volatile_dependency_has_one_value_per_evaluation fails intermittently: a later evaluation sometimes reuses the volatile value
status: closed
priority: P2
complexity: S
area: [core/assets]
design: evaluation-session-volatile-test-flake
created: 2026-10-13
github:
---
## Problem

`liquers-core/tests/evaluation_session.rs` `volatile_dependency_has_one_value_per_evaluation`
(through `volatile_scenario`) fails intermittently at line 113:

```
assertion `left == right` failed: a later evaluation runs v again
  left: 1
 right: 2
```

The second evaluation of the volatile query reused the value of `v` from the first instead of
running it again. In five runs of `cargo test -p liquers-core --all-features --lib --tests` on
2026-10-13 it failed twice and passed three times, with no code change between runs. The fixture
uses a plain `AsyncMemoryStore` (`tests/common/chain_fixture.rs`), so stores and routing are not
involved.

## Problem Example

Run `cargo test -p liquers-core --test evaluation_session volatile_dependency_has_one_value_per_evaluation`
20 times. Today about half the runs fail with `left: 1, right: 2`; every run should pass.

## Impact

CI on any PR touching `liquers-core` can go red without cause, and a real regression of the
once-per-evaluation contract (`design/dependency-resolved-once-per-evaluation/`) would look like the
same flake. Workaround: re-run.

## Expected behaviour

The test passes deterministically. Likely cause to check first: the evaluation session is kept alive
only by running contexts (`assets.rs`, `evaluation_session_ends_with_its_last_run`), so whether the
second evaluation starts a new session depends on when the first one's last reference is dropped —
a timing dependency the test does not control.

## Discovery

Full `liquers-core` test runs during `design/store-router-forwards-asset-info/` Step 5, 2026-10-13.

## Update 2026-10-13: still failing, now most of the time

Reproduced on a clean `origin/main` worktree at `2bc4228`. On this machine it fails in most runs:
4 of 6 runs of the single test on the merged branch, and 2 of 4 on clean main. The failure is the
same assertion at line 113 (`left: 1, right: 2`). Seen while validating the unrelated
`design/value-to-json-non-finite-float/` branch, whose changes do not touch evaluation.

## Resolution (2026-10-21)

Fixed at HEAD (`18732dd`) by runtime volatility (`design/runtime-volatility/`, commit `b87faa1`,
PR #142), found during the 2026-10-20 bulk design. The failing second evaluation fast-tracked a
stored `Ready` copy of `data/c.txt` that a background save wrote after `manager.remove` (log:
"Fast track successful for asset with key data/c.txt"). Now `a`, `b` and `c` become volatile at run
time because they read the volatile `v`; their stored copies are `Volatile` and fast track refuses
them. Measured on one machine: the test binary built from `2bc4228` failed 16/20 and 21/30 runs; the
HEAD binary passed 280/280 (queued, including under parallel load) and 20/20 (inline). The design
`evaluation-session-volatile-test-flake` is abandoned. The underlying product race (a late
background save reviving a removed or replaced non-volatile key) is filed as
`REMOVED-OR-REPLACED-KEY-REVIVED-BY-A-LATE-BACKGROUND-SAVE`.
