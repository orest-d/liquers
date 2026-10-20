---
id: MISSING-RESOURCE-REPORTED-AS-GENERAL-ERROR
kind: issue
title: A key that is neither stored nor made by a recipe raises a General error without its key
status: draft
priority: P2
complexity: M
area: [core/assets, core/error]
design:
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
