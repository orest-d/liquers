---
id: WEB-EVALUATE-RESOLVES-FAILED-ASSET-AS-NULL
kind: issue
title: liquers-web's evaluate resolves a failed command's asset to null instead of rejecting
status: closed
priority: P2
complexity: S
area: [web]
created: 2026-10-18
github:
---

## Problem

`liquers-web/src/eval.rs` `evaluate_query` returns `value_to_js(state.data_unchecked())` after
`asset.get().await?`. `AssetRef::get` returns `Ok` with a terminal *error state* when a command
fails (`liquers-core/src/assets.rs`, `get`: "a failed asset now has a terminal error-state that
poll_state() returns as Ok(...)"), so the error is never checked and the promise resolves with the
error state's empty value, `null`.

The web `ERROR03` tests call the error bridge directly, not the evaluation path, which is why no
test catches it. This is inferred from reading the code and has not been reproduced in a browser.

## Re-checked at HEAD (2026-10-20): partly — the common path already rejects

The defect is narrower than described above. On `wasm32` the environment uses
`ImmediateAssetManager`, whose `get_asset` runs the asset inline (`get_query_asset_run` →
`AssetRef::run_inline`), and `run_with_future_inline` → `finish_run_with_result` returns the
command's `Err` (`liquers-core/src/assets.rs`). So for the caller that runs the evaluation, the
`?` on `manager.get_asset(&query).await?` in `evaluate_query` already rejects, before
`data_unchecked()` is reached. `EVAL04` (`liquers-web/tests/eval_EVAL.rs`,
`eval04_invalid_query_maps_through_error`, its "the command throws" half) asserts exactly that, through
`evaluate_query`.

What is still wrong is the unchecked read itself, which is reached whenever `get_asset` returns
`Ok` for an asset that failed. The concrete path is a **second caller** of the same query while the
first is still running: `run_with_future_inline` → `claim_or_wait_inline` waits for the running
evaluation and returns `Ok(None)`, `get_asset` returns `Ok(asset)`, `asset.get()` returns the
terminal error state as `Ok`, and `state.data_unchecked()` hands `null` to the page. The rest of the
issue (the fix, `state.value()?`, and the test) stands; the test has to be the concurrent case,
since the single-caller case is already covered.

## Problem Example

```javascript
liquers.registerCommand({ name: "boom", async: true,
  run: async () => { await new Promise(r => setTimeout(r, 50)); throw new Error("bang"); } });
const [a, b] = await Promise.allSettled([liquers.evaluate("boom"), liquers.evaluate("boom")]);
// today:    a = rejected (LiquersError "Error: bang"), b = fulfilled with null
// expected: both rejected with the same execution_error
```

## Impact

Every JavaScript caller of `evaluate`: a failed command looks like a command that returned `null`,
so the error, its type and its traceback are lost, and a page shows an empty result instead of a
failure. No workaround from JavaScript short of `getAsset` and reading the status.

## Expected behaviour

`evaluate` rejects with the asset's error (`state.value()?`, or `value_state()?`), and a test
evaluates a query whose JavaScript command throws and asserts the rejection.

## Discovery

Building `liquers-py` (`specs/design/liquers-py/`, Phase 4 step 9): its first `evaluate` had the
same shape, and a command raising `ValueError` made `evaluate` return `None`. There it was fixed
with `state.value()?`; reading `liquers-web/src/eval.rs` showed the same pattern.

## Resolution (2026-10-26)

Fixed as described: `evaluate_query` in `liquers-web/src/eval.rs` reads the asset's state with
`value()?` instead of `data_unchecked()`, so the terminal error state of a failed asset rejects.
`eval04_failed_evaluation_rejects_a_concurrent_caller` in `liquers-web/tests/eval_EVAL.rs` starts
two evaluations of an async command that rejects and asserts both reject with the same error; it
failed before the fix (the second resolved) and passes after it.
