---
id: SCHEMA-LESS-ORDERED-JSON-ORIENTS-LOSE-COLUMN-ORDER
kind: issue
title: A schema-less read of the split or values JSON orient sorts the columns its document orders
status: draft
priority: P3
complexity: S
area: [records]
created: 2026-10-07
github:
---
# A schema-less read of an ordered JSON orient sorts the columns its document orders

## Problem

Two JSON shapes state a column order: `split` lists it in `columns`, and `values` has positions
(read as `c0`, `c1`, …). `liquers-records/src/formats/shapes.rs` turns both into row objects (a
`serde_json::Map`) and passes them to `ndjson::objects_to_batch`. Without a schema, that function
ends in `read_inferred_objects`. (`table` builds its schema from `schema.fields` and keeps that
order. `list` takes its names from object keys, which have no order, so sorting them is right.)

`read_inferred_objects` sorts the column names. That was the maintainer decision of 2026-10-06
for JSON with no specified order. Before that sort existed, `Map`, which is a `BTreeMap` without
`serde_json/preserve_order`, already sorted each row's keys. So the order these documents give is
lost, and it was lost before the sort was added too. `split` with `"columns": ["z", "a"]` reads
as `a, z`. A `values` document with eleven columns reads as `c0, c1, c10, c2, …`.

The decision covers JSON whose column order is not specified. These orients do specify one.

## Expected behaviour

A schema-less read of `split` and `values` keeps the document's column order.
One possible approach: `objects_to_batch` takes the column order when the shape knows it, and
sorts only when it does not.

## Discovery

Found 2026-10-07 while implementing `design/json-table-column-order/`. That design sorts in
`read_inferred_objects`; it changes nothing for these orients, because their rows already arrived
sorted.
