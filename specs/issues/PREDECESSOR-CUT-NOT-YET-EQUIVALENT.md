---
id: PREDECESSOR-CUT-NOT-YET-EQUIVALENT
kind: issue
title: Cutting a predecessor boundary is not yet equivalent to expanding it
status: draft
priority: P1
complexity: M
area: [core/plan, core/assets]
design: plan-cwd-freeze
created: 2026-08-15
github:
---
## Problem

`Plan::cut_predecessor` works and is covered, but turning it on for every recipe still changes
observable behaviour. Measured by calling it from `finalize_plan` and running
`cargo test -p liquers-core --tests --no-fail-fast`:

| Suite | Failures under the cut |
|---|---|
| `--lib` | 1 — `absolute_outer_resource_keeps_relative_link_on_live_cwd`, which asserts the **expanded** plan shape (`steps[1]` is a `GetAsset`, an `Evaluate` once cut). Not a defect. |
| `recipe_cwd_resolution` | 6 — `programmatic_and_provider_cwd_select_their_own_inputs`, `explicit_cwd_overrides_recipe_cwd_for_later_relative_link`, `resolved_dependency_identity_reuses_cached_asset`, `context_boundary_commands_use_active_cwd`, `recursive_links_and_multiple_parameters_use_active_cwd`, `nested_keyed_recipe_cwd_is_not_bypassed` |
| `expiration_integration` | 3 — `test_expired_dependency_is_recomputed_before_dependent_evaluation`, `test_get_any_status_has_no_side_effects_on_normal_get`, `test_manager_re_request_still_rebuilds_after_gate` |
| `injection` | 1 — `test_chained_commands_with_payload` |

Every `recipe_cwd_resolution` failure is CWD-related, which points at a nested **keyed** recipe
re-deriving its own working key behind the boundary rather than inheriting the frozen one. The
expiration failures cluster around dependency recomputation and re-request gating, which a boundary
changes by inserting an extra asset between dependent and dependency. The injection failure is a
payload crossing one more boundary than before.

## Impact

None today: nothing calls `cut_predecessor`, so the shipped behaviour is unaffected and the default
stays expanded. It blocks `CORE-PLAN-POLICY-AND-DEFAULTS` from flipping that default, which is the
reason the boundary machinery exists — intermediates that are individually cached, independently
expiring and separately schedulable.

## Expected behaviour

Cutting and expanding produce the same value, the same `is_volatile` / `payload_required` /
`expires`, and the same surfaced error, for every query shape. `specs/design/plan-cwd-freeze/`
Phase 3 specifies the suite that should establish this (twelve shapes, E1-E12, with E8 pinning the
one documented divergence — a payload command that omits `payload: required`). The failures above
are its starting worklist.

## Notes

Two equivalence differences have already been found and fixed this way, which is why the suite is
worth building rather than reasoning about:

- A stale `predecessor_steps` across the recipe CWD prefix made a cut plan run the predecessor's
  action **twice**, once in the boundary asset and once inline. Fixed in `plan-cwd-freeze`.
- A dependency's error was replaced by "did not produce a value", so a boundary hid the diagnosis.
  Fixed in the same design by chaining the cause.

Phase 2 of that design concluded cutting was a policy choice rather than a correctness one, on the
grounds that payload, volatility, side effects and cycles all reduce to declaration defects. Both
fixes above were differences that analysis did not anticipate.

## Discovery

`specs/design/plan-cwd-freeze/` implementation, 2026-08-15, by calling `cut_predecessor` from
`finalize_plan` and running the suite.
