---
id: ASSET-PAYLOAD-REQUIREMENT-NOT-RECORDED
kind: issue
title: An asset never records that its evaluation required a payload
status: closed
priority: P2
complexity: M
area: [core/assets, core/context]
design: evaluate-path-consolidation
created: 2026-09-03
github:
---

## Resolution

Closed 2026-09-04 by `design/evaluate-path-consolidation/` Step 1. The plan's payload requirement
is recorded in `interpreter::apply_plan`, beside the authoritative gate that already read it, and
reaches `MetadataRecord.payload_required` and `AssetInfo`. `AssetRef::payload_required()` reads it
back from metadata rather than duplicating it in an `AssetData` field.

Two tests, one verified to fail with the projection disabled. The recipe-preview projection in
`AsyncRecipeProvider::get_asset_info` is added for consistency but is unreachable and says so:
both callers pass `Some(key)`, so the plan is built with `to_plan_for_key`, which rejects a
payload-requiring recipe because keys are a payload boundary.

## Problem

`MetadataRecord` and `AssetInfo` both carry a `payload_required: PayloadRequirement` field
(`liquers-core/src/metadata.rs:913`, `:716`), with a setter (`set_payload_required`, `:1386`),
accessors (`:1381`, `:2308`), legacy-JSON extraction (`:1532`) and round-trip tests
(`test_asset_info_round_trip_preserves_payload_required`, `:2826`).

Nothing ever sets it during evaluation. A grep for `set_payload_required` outside `metadata.rs`
returns nothing: not `assets.rs`, not `context.rs`, not `interpreter.rs`, not `plan.rs`. Every
asset that is actually evaluated — including one that could not have run without a payload — ends
with `payload_required: PayloadRequirement::None`.

The requirement *is* computed, but only transiently and only in two places that discard it:

- `Context::apply` (`context.rs:707`) calls `query.requires_payload(envref)` to decide whether to
  route through `apply_immediately`, then drops the answer.
- `AssetManager::recipe_opt` and `evaluate_recipe_outcome` call `Recipe::to_plan_for_key`, which
  rejects a keyed recipe needing a payload, and likewise keeps nothing.

`AssetData` has no field for it either, unlike the parallel `is_volatile`, which
`resolve_volatility_before_evaluation` resolves before evaluation and which reaches metadata.

## Impact

Diagnostic, but in the one place diagnosis matters most. A payload-evaluated asset is
non-reproducible: it must not be cached, shared, or reused, and its stored form is not a valid
value for its key. `is_volatile` carries the operational consequence, so nothing is *broken* today
— but a client, a server response, or a stored `.metadata` sidecar cannot distinguish "volatile
because the command declared itself volatile" from "volatile because this run consumed a payload",
which is exactly the distinction `AssetInfo.payload_required` was added to express. The comment on
the field claims it is available for that purpose; it is not.

It also leaves the payload precondition without a single home: whether an evaluation may proceed
without a payload is re-derived at each entry point instead of being resolved once and recorded,
which is a case of the duplication `CORE-EVALUATE-PATH-CONSOLIDATION` is about.

## Expected behaviour

The payload requirement is resolved before evaluation, symmetrically with volatility: a
`payload_required` field on `AssetData`, resolved from the plan in the same pre-evaluation pass as
`resolve_volatility_before_evaluation`, written into `MetadataRecord` so it reaches `AssetInfo` and
the stored sidecar, and used as the single precondition check (`required && payload.is_none()` is
an error) instead of the per-entry-point pre-checks.

Note the property to preserve: what makes an asset non-reproducible is the *requirement*, not
whether a payload happened to be in scope. A plain query evaluated through
`EnvRef::evaluate_immediately` has a payload available that no command consumes, and must stay
reproducible.

## Discovery

Found on 2026-09-03 while designing `CORE-EVALUATE-PATH-CONSOLIDATION`, checking where a recorded
payload requirement would have to land. Filed separately because it is a self-contained gap with
its own test surface, and scheduled into that design (`design:` above) because the consolidated
evaluation path is where the resolution pass belongs.
