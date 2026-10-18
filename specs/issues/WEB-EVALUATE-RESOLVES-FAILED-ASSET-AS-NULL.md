---
id: WEB-EVALUATE-RESOLVES-FAILED-ASSET-AS-NULL
kind: issue
title: liquers-web's evaluate resolves a failed command's asset to null instead of rejecting
status: draft
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
