---
id: SCHEMA-LESS-JSON-READS-SORT-COLUMNS-ALPHABETICALLY
kind: issue
title: A JSON table read without a schema gets its columns in alphabetical order
status: draft
priority: P3
complexity: S
area: [records]
design: record-streams
created: 2026-09-26
github:
---
# A JSON table read without a schema gets its columns in alphabetical order

## Problem

`liquers-records`' JSON readers take a `serde_json::Value`. Every JSON object key order in the
workspace is lost at parse time, because `serde_json` is built without its `preserve_order` feature,
so `Map` is a `BTreeMap`. So a table read from NDJSON, the `json` format, or the `records`,
`columns` and `index` shapes **without** a declared schema has its columns in alphabetical order,
not in the order of the file: `{"name": …, "age": …}` reads as `age, name`. A declared schema fixes
the order, since columns follow the schema.

Row order is not affected. Arrays keep their order, and the `columns` / `index` shapes restore
numeric order for integer row keys (`formats/shapes.rs`, `ordered_row_keys`).

## Why it matters

The table is still correct, but it looks different from its source. A round trip through the
`columns` shape reorders a pandas frame's columns, and a view whose column order a user chose
comes back reordered.

## Options

- Enable `serde_json`'s `preserve_order`. It is one feature flag, but feature unification turns it
  on for every crate that uses `serde_json`. Every JSON map in the workspace then keeps insertion
  order, which changes serialized output wherever a map was built out of order. That has to be
  checked before adopting it.
- Parse the records crate's JSON through a streaming `Deserializer` that records key order,
  without changing the rest of the workspace.

## Discovery

Found 2026-09-26, `record-streams` Phase 4 Step 3.2, while implementing the JSON shapes.
