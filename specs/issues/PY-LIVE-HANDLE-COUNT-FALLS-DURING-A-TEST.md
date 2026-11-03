---
id: PY-LIVE-HANDLE-COUNT-FALLS-DURING-A-TEST
kind: issue
title: A live handle count can fall during a liquers-py test that compares it to its baseline
status: draft
priority: P3
complexity: S
area: [py]
design: 
created: 2026-11-03
github:
---
## Problem

`test_RECIPE_late_registration_and_release` (`liquers-py/tests/test_RECIPE_registration.py:110`)
takes `handle_baseline()` (`liquers-py/tests/conftest.py`), builds an environment, and asserts
`lq.Environment.live_handle_count() > baseline`. On CPython 3.14 in CI it failed with
`assert 8 > 12`: the count fell by more than the test's own handles between the baseline and the
assertion. `handle_baseline()` already runs `gc.collect()` first (commit `cd4dd02`), so the
handles released during the test are not ones left in reference cycles that a collection would
have freed. They are released later by something else, most likely an earlier environment whose
runtime is still shutting down on another thread when the next test starts.

The count is process-wide, so any handle released asynchronously by an earlier test moves the
baseline of the next one. `test_RUNTIME_runtime.py` asserts exact counts the same way.

## Impact

The `liquers-py-tests` workflow fails intermittently on a change that does not touch liquers-py
(seen on PR #248, `test (3.14)`; the same commit passed on 3.10 and 3.14t). A re-run usually
passes. No effect on users: it is the test's measurement, not a leak.

## Expected behaviour

A test comparing against the live handle count is deterministic: either the baseline waits until
earlier environments have released their handles (shutdown that joins, or a poll until the count
is stable, as `test_RUNTIME_runtime.py:93` already does for one case), or the assertion counts the
handles of this environment only.

## Discovery

CI on PR #248 (2026-11-03), job `test (3.14)` of `liquers-py-tests`.
