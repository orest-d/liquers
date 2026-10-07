# Phase 5: Documentation

**Status: executed and approved 2026-10-07.**

## Summary

Implemented 2026-10-07 together with `immediate-set-state-status-match` (merge M2 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`).

- New private `log_supplied_expiry_record` / `log_supplied_expiry` in `liquers-core/src/assets.rs`,
  called at the four write sites when the written status is `Expired`. They append a warning
  `Asset expired` and an info entry `Expiry recorded after the fact: {key} was written already
  expired ({route}); its original cause is unknown` (or `…; supplied reason: …`). The structured
  `expiry_reason` stays as supplied; legacy metadata is untouched.
- Tests: `liquers-core/tests/supplied_expired_status.rs`, each over both managers:
  `supplied_expired_state_logs_asset_expired`, `supplied_expired_binary_logs_asset_expired`,
  `supplied_expiry_reason_is_kept_and_logged`, `supplied_ready_adds_no_expiry_log`.

## Conformance and deviations

As designed, with one documentation deviation: the "routes into `Expired`" material lives in
`reference/ASSETS.md` §Why an asset is `Expired` and `reference/ASSET_LIFECYCLE.md` §Routes into
`Expired`, not in `DEPENDENCIES_STATUS.md` as Phase 1 assumed, so those two documents were updated.
`ASSETS.md` had cited the issue as open; that sentence is replaced.

## New issues

None.
