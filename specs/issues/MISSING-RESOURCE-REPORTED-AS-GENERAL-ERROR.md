---
id: MISSING-RESOURCE-REPORTED-AS-GENERAL-ERROR
kind: issue
title: A key that is neither stored nor made by a recipe raises a General error without its key
status: closed
priority: P2
complexity: M
area: [core/assets, core/error]
design: key-without-recipe-answer
created: 2026-10-19
github:
---

## Problem

Evaluating `-R/data/missing.txt` against a store that does not hold the key, with no recipe for it,
fails with `ErrorType::General` and the message `No recipe found for key data/missing.txt`. The
error's `key` field is empty when it reaches the caller, although the recipe lookup builds it with
`.with_key` (`liquers-core/src/recipes.rs`, around line 878). From Python this is
`lq.GeneralError` with `e.key is None`.

## Impact

The commonest failure a new user meets cannot be caught as `KeyNotFound`, and code that wants to
tell "missing input" from any other failure has to match the message text. The message also talks
about recipes to a user who only asked for a file.

## Expected behaviour

A missing resource fails with `ErrorType::KeyNotFound`, carrying the key, and a message that says
the key is not in the store and no recipe produces it.

## Discovery

Found by the Phase 5 review of `design/liquers-py/` (2026-10-19). Related special cases:
`JS-STORE-RESOURCE-NOT-FOUND-WITHOUT-A-RECIPE`, `DIRECTORY-KEY-CANNOT-BE-EVALUATED-AS-A-RESOURCE`,
`DEGRADED-STORED-VALUE-WITHOUT-RECIPE-REPORTS-NO-RECIPE`.

## Reproduction at HEAD (bulk design, 2026-10-20)

Still reproduces at 18732dd. `AssetRef::evaluate_recipe_outcome` (`liquers-core/src/assets.rs`)
calls `get_recipe_provider().recipe(&key)`; `DefaultRecipeProvider::recipe` and
`RecipeProviderChain::no_recipe_error` (`recipes.rs`) build `Error::general_error("No recipe found
for key …").with_key(key)`, and `Error::with_key` writes the `query` field, not `key`
(`ERROR-WITH-KEY-SETS-QUERY-FIELD`) — which is why `e.key` is `None`. The integration test
`metadata_only_entry_without_recipe_answers_like_an_absent_key` pins the message today. Designed
together with `DEGRADED-STORED-VALUE-WITHOUT-RECIPE-REPORTS-NO-RECIPE` and
`DIRECTORY-KEY-CANNOT-BE-EVALUATED-AS-A-RESOURCE` in `design/key-without-recipe-answer/` (one root
cause: every fast-track decline ends at the recipe provider's error).

## Resolution

2026-10-25, `design/key-without-recipe-answer/`. A key that is neither stored nor made by a recipe now fails with `ErrorType::AssetNotFound` (`Error::asset_not_found`), `key` set, HTTP 404; Python raises `AssetNotFoundError`, which is a `KeyedAssetError` and a `NotFoundError`. Tests: `liquers-core/src/assets.rs` `key_without_recipe_tests::missing_key_without_recipe_is_asset_not_found`, `metadata_only_key_without_recipe_is_asset_not_found`; `liquers-py/tests/test_EVAL_evaluation.py::test_EVAL_missing_resource_raises_asset_not_found`; `liquers-axum/tests/assets_api_endpoints.rs` `aae92_submit_of_an_unmakeable_key_is_asset_not_found`.
