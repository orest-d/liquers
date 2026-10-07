# Phase 5: Documentation

## Summary

Implemented 2026-10-07 as Wave 1 step 10 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`.

- Both lazy-expiry sites of `ImmediateAssetManager` (`get_asset`'s query loop and `get`'s keyed
  loop) call `expire_with_reason(Direct { Deadline })` instead of `expire_without_cascade`, so the
  dependents of a keyed root are expired with `Cascaded { Deadline, root, via }`.
- Lock check: `cascade_expire_dependents` → `expire_dependencies_result` →
  `expire_without_cascade` / `expire_stored_copy` takes no `key_mutation_lock`, so the call stays
  before the lock, as the queued monitor relies on.
- **Addition:** the external `MinimalInlineAssetManager` (`tests/common/minimal_manager.rs`) cascades
  through public API (`expire_without_cascade` then `cascade_expire_dependents`), since
  `expire_with_reason` is crate-private; the implementation guide says so.
- Tests: shared scenarios in `tests/common/manager_scenarios.rs` —
  `scenario_lazy_deadline_expiry_cascade` (E1 + E2) on the default, immediate and external
  managers, and `scenario_lazy_dependent_read_first` (E3) on the immediate manager. The immediate
  case fails without the fix.

## Conformance and deviations

- **E3 differs from the Phase 1 assumption.** A dependent inherits its dependencies' earliest
  deadline, so reading `b.txt` first after the deadline is *not* served stale: `b.txt` reaches its
  own deadline and is recomputed. The residual case remains only for a dependent that carries no
  such deadline; the reference states it with its remedies (audit, `OnLoad`).
- **Docs:** the immediate-manager expiry statements live in `ASSET_LIFECYCLE.md` §Routes into
  `Expired`, `ASSETS.md` (`Deadline` row), `DOC_03` (route table) and the asset-manager guide, not
  in `DEPENDENCIES_STATUS.md`; those were updated instead.

## Documentation

`reference/ASSET_LIFECYCLE.md`, `reference/ASSETS.md`,
`reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`, `guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md`
(the known-limit row is removed).

## New issues

None.
