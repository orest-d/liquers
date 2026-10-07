# Phase 5: Documentation

**Status: executed and approved 2026-10-07.**

## Summary

Implemented 2026-10-07 together with `supplied-expired-status-reason` (merge M2 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`).

- New private `written_status(Status) -> Option<Status>` (every variant listed) and
  `written_status_with_recipe(bool) -> Status` in `liquers-core/src/assets.rs`. All four write
  sites (`DefaultAssetManager` and `ImmediateAssetManager`, `set_binary` and `set_state`) use them
  with `match … { Some(status) => status, None => written_status_with_recipe(…) }`, so the recipe
  lookup stays lazy. `rg "_ if self.recipe_opt" liquers-core/src` returns nothing.
- Tests: `written_status_keeps_expired_and_error` (iterates the test module's `ALL_STATUSES`),
  `written_status_defers_others_to_recipe`; existing set tests unchanged.

## Conformance and deviations

As designed. Pure refactor; no observable change.

## Documentation

`reference/ASSETS.md` §Why an asset is `Expired` now states the shared written-status rule (with
the M2 partner's change).

## New issues

None.
