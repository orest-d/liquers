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

Found while building `liquers-py`, whose first `evaluate` had the same shape: a command raising
`ValueError` made `evaluate` return `None`. There it was fixed with `state.value()?`. For
liquers-web this is inferred from reading the code and has not been reproduced in a browser. The
web `ERROR03` tests call the error bridge directly, not the evaluation path, which is why no test
catches it.

## Expected

`evaluate` rejects with the asset's error (`state.value()?`, or `value_state()?`), and a test
evaluates a query whose JavaScript command throws and asserts the rejection.
