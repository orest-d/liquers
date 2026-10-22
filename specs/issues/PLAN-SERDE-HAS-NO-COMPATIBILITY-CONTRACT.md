---
id: PLAN-SERDE-HAS-NO-COMPATIBILITY-CONTRACT
kind: issue
title: A serialized Plan has no compatibility contract
status: draft
priority: P3
complexity: M
area: [core/plan]
design:
created: 2026-10-20
github:
---
## Problem

`Plan`, `Step` and `ParameterValue` (`liquers-core/src/plan.rs`) derive `Serialize`/`Deserialize`
and are public, but their wire shape is not versioned. Fields added recently are `serde(default)`
case by case; `query`, `steps` and `is_volatile` are required; a new `Step` variant cannot be read
by an older build.

## Impact

Anything persisting plans (metadata, a cache, a client) relies on an implicit, unversioned format.
Low impact today: no known consumer stores plans across versions.

## Expected behaviour

Either declare the serialized plan runtime-internal (and say so in its rustdoc), or version a
supported schema.

## Discovery

Found while reviewing `reference/api/DOC_08_RECIPES_PLANS.md` against the code at `96279f14` before promoting it to [`reference/RECIPES_AND_PLANS.md`](../reference/RECIPES_AND_PLANS.md) (2026-10-20).
