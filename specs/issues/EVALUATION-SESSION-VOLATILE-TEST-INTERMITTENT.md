---
id: EVALUATION-SESSION-VOLATILE-TEST-INTERMITTENT
kind: issue
title: volatile_dependency_has_one_value_per_evaluation fails intermittently: a later evaluation sometimes reuses the volatile value
status: draft
priority: P2
complexity: S
area: [core/assets]
design: 
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
