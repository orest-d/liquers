# Phase 5: Documentation

## Summary

Implemented 2026-10-07 as Wave 1 step 15 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`. No behaviour change.

- `Context::submit` doc comment and `reference/DEPENDENCIES_STATUS.md` Flow B step 2 now state the
  three cases: inline manager — finished on return; queued manager with capacity — started;
  saturated queued manager — queued on the asset's local queue (`Submitted`), started at the first
  `wait_for_dependency` / `evaluate` drain.
- New test `submit_queues_locally_when_queued_manager_is_saturated` (`liquers-core/src/context.rs`,
  job capacity 1 via `EnvironmentBuilder`) observed `Submitted` after `submit`, confirming the
  wording (Phase 4's containment branch was not needed). The two existing pinning tests are
  unchanged.
- The frozen `dependency-audit-and-expiry-provenance` Part E text is left as is (§5.1); this
  design and the issue's resolution record the correction.

## Documentation

`reference/DEPENDENCIES_STATUS.md` (History row, `reviewed:`); `Context::submit` doc comment.

## New issues

None.
