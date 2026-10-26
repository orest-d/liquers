---
id: CROSS-REALM-TRANSFER-NOT-PLANNED
kind: feature
title: A value crossing a realm boundary is not checked or converted at planning time
status: draft
priority: P3
complexity: L
area: [core/plan, core/value]
design: cross-realm-transfer-planning
created: 2026-10-20
github:
---

## Problem

Once `design/realm-type-registry/` lands, every `TypeInfo` declares a `TransferPolicy`
(`Serialize`, `ConvertTo`, `Untransferable`) and a registry can hold every realm. Nothing acts on
it: `PlanBuilder` (`liquers-core/src/plan.rs`) does not know which type a step produces, does not
know where a step runs (`CORE-MULTI-REALM-INTERPRETER`), and so cannot refuse a plan that would send
a `js.Value` to a server or insert the conversion a `polars.DataFrame` needs before reaching a
browser.

## Problem Example

`-R/data/sales.csv/-server/heavy_stats/-/to_text`, with `server` a remote realm whose
`heavy_stats` returns a `polars.DataFrame` and a page without Polars. Should: the plan converts on
the server (`ConvertTo RecordView`) or is refused naming the type and both realms; a `js.Value`
sent toward the server is refused as `Untransferable`. Today nothing checks, and (with dispatch)
the page would fail to decode the bytes.

## Impact

None until realm dispatch exists; then a cross-realm query fails only when the receiver cannot
decode the value, which is the late, opaque failure `TYPE-REGISTRY-NOT-REALM-AWARE` set out to
prevent.

## Expected behaviour

At planning time, for each boundary crossing: an `Untransferable` value is a planning error naming
the type and both realms; a `ConvertTo` value gets the conversion step placed on the side that can
perform it (`VALUE-CONVERSION-CAPABILITY`); a `Serialize` value crosses in a format both realms'
registries support, or the plan is refused. Requires a command's output type to be known at plan
time, which command metadata does not record today.

## Discovery

Bulk design 2026-10-20 (cluster C6), split off from `TYPE-REGISTRY-NOT-REALM-AWARE` behaviours 3-4.
Requires `design/realm-type-registry/`, `design/multi-realm-dispatch/` and
`VALUE-CONVERSION-CAPABILITY`.
