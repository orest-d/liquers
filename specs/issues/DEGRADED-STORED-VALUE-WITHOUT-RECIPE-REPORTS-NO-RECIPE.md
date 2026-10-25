---
id: DEGRADED-STORED-VALUE-WITHOUT-RECIPE-REPORTS-NO-RECIPE
kind: issue
title: Evaluating a stored value this build cannot read fails with "No recipe found" instead of naming the type
status: closed
priority: P2
complexity: S
area: [core/assets]
design: key-without-recipe-answer
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

## Reproduction at HEAD (bulk design, 2026-10-20)

Still reproduces at 18732dd: the `Ok(None)` branch of `AssetData::try_fast_track` prints the
degraded reason and returns `Ok(false)`; the evaluation then fails in
`AssetRef::evaluate_recipe_outcome` with the recipe provider's "No recipe found". `liquers-py`'s
`test_VALUE_pickle_off_error_names_the_type` is still a strict `xfail`. Merged into
`design/key-without-recipe-answer/` with `MISSING-RESOURCE-REPORTED-AS-GENERAL-ERROR` (same root
cause and change site); it cannot be fixed alone without the `liquers-py` test edit, so it is not
eligible for automatic fixing on its own either.

## Resolution

2026-10-25, `design/key-without-recipe-answer/`. The fast track records why it declined a stored value; with no recipe that reason is the answer. A degraded value fails with `AssetNotSupported` naming the type and the reason, the same error `get_any_status` gives (`degraded_value_error`). Tests: `key_without_recipe_tests::degraded_stored_value_without_recipe_names_the_type`; `liquers-py` `test_VALUE_pickle_off_error_names_the_type`, no longer `xfail`.
