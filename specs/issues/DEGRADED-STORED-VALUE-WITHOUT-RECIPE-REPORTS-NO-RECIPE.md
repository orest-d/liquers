---
id: DEGRADED-STORED-VALUE-WITHOUT-RECIPE-REPORTS-NO-RECIPE
kind: issue
title: Evaluating a stored value this build cannot read fails with "No recipe found" instead of naming the type
status: draft
priority: P3
complexity: S
area: [core/assets]
created: 2026-10-18
github:
---

## Problem

When `-R/<key>` names a stored value that `deserialize_stored_value` reads as *degraded* (its
type is not registered in this build, or is registered with no data format), `try_fast_track`
(`liquers-core/src/assets.rs`) logs the reason to stderr and returns `Ok(false)`. The manager then
submits the asset for computation. With no recipe for the key, the evaluation fails with
`GeneralError: No recipe found for key <key>`, which hides the real cause. The value is in the
store; this environment just cannot materialize it.

`get_any_status` (the store-read path further down the same file) already reports the degraded
reason with the key.

## Impact

Any integration whose value type is formatless in one build and serializable in another, and any
build missing a value type another build stored: the user is told the key has no recipe, which
sends them looking for a missing recipe instead of a missing codec. The safety property holds (the
bytes are never decoded); only the message is wrong. Workaround: read the key's metadata, whose
`type_identifier` names the type.

## Expected behaviour

When the fast track refuses a degraded stored value and the recipe provider cannot make the key,
the evaluation fails with the degraded reason (`degraded_reason`), naming the type identifier and
the key. When a recipe exists, recomputing stays the right outcome. `liquers-py`'s
`test_VALUE_pickle_off_error_names_the_type` is a strict `xfail` until this is fixed.

## Discovery

Implementing `liquers-py`'s AC-18 (`specs/design/liquers-py/`, Phase 4 step 10): a pickle-off
environment reading a `py.Object` that a pickle-on one stored correctly refuses to unpickle, as
required, but the test asserting the error names `py.Object` failed on "No recipe found".
