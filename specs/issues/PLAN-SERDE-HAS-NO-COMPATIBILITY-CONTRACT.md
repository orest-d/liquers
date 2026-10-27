---
id: PLAN-SERDE-HAS-NO-COMPATIBILITY-CONTRACT
kind: issue
title: A serialized Plan has no compatibility contract
status: draft
priority: P3
complexity: S
area: [core/plan]
design: plan-serialization-internal-contract
created: 2026-10-20
github:
---
## Problem

`Plan`, `Step` and `ParameterValue` (`liquers-core/src/plan.rs`) derive `Serialize`/`Deserialize`
and are public, but their wire shape is not versioned. Plans leave the process through the HTTP
recipes API (`GET resolve/{*key}`) and Python (`Plan.to_json` / `from_json`). The `Plan` rustdoc
half-promises stability for the `serde(default)` fields. Fields added recently are `serde(default)`
case by case; `query`, `steps` and `is_volatile` are required; a new `Step` variant cannot be read
by an older build.

## Problem Example

A client stores the JSON plan from `GET resolve/data/report.csv` and reads it with `Plan.from_json`
after an upgrade that added a `Step` variant. Nothing documents whether that is supported; it fails.
Should be: the documentation says plans are an inspection format of one build, and to persist the
query or recipe instead.

## Impact

Anything persisting plans (metadata, a cache, a client) relies on an implicit, unversioned format.
Low impact today: no known consumer stores plans across versions.

## Expected behaviour

Either declare the serialized plan runtime-internal (and say so in its rustdoc), or version a
supported schema.

## Discovery

Found while reviewing `reference/api/DOC_08_RECIPES_PLANS.md` against the code at `96279f14` before promoting it to [`reference/RECIPES_AND_PLANS.md`](../reference/RECIPES_AND_PLANS.md) (2026-10-20).
