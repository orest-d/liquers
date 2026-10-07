# Phase 5: Documentation

**Status: executed and approved 2026-10-07.**

## Summary

Implemented 2026-10-07 as Wave 1 step 13 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`.

- In `AssetRef::record_dependency_on_asset` (`liquers-core/src/assets.rs`; Phase 1–2 called the
  site `enter_dependencies`, which only sets the status), after `add_dependency` records the edge:
  if the recorded version and the map's current version for the dependency are both concrete and
  differ, the dependent takes the stale-dependency route through `note_expired_dependency`, so it
  finishes `Expired` with `Direct { StaleDependency }`. `add_dependency` is unchanged and
  `add_dependency_records_a_disagreeing_version_without_expiring` still passes.
- Tests: `edge_against_superseded_version_marks_dependent_stale`,
  `edge_with_unknown_version_marks_nothing`, `edge_with_current_version_marks_nothing`.

## Conformance and deviations

- T1 is a unit test on `record_dependency_on_asset` rather than the planned end-to-end
  `tests/stale_edge_at_birth.rs`: the window lies inside one dependency fetch, and
  `register_version` is crate-private, so an integration test cannot place a write inside it
  without a production hook. The finalization half (stale flag → `Expired` with
  `StaleDependency`) is the existing route, covered by the stale-dependency tests.
- The warning logged by `note_expired_dependency` says the dependency "expired during
  evaluation"; for this case it changed. The wording was kept to reuse the route unchanged.

## Documentation

`reference/DEPENDENCIES_STATUS.md` §Current contract (new bullet; the listing bullet no longer
names the window as uncaught) and the `StaleDependency` row of `reference/ASSETS.md`.

## New issues

None.
