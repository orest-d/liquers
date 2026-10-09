---
id: SCHEMA-LESS-JSON-ORIENT-INDEX-COLUMN-OVERWRITTEN
kind: issue
title: A schema-less read of an indexed JSON orient silently drops the index when a data column is named index
status: draft
priority: P3
complexity: S
area: [records]
design: json-orient-index-column-collision
created: 2026-10-08
github:
---
# A data column named `index` overwrites the index of a schema-less indexed JSON read

## Problem

Without a schema, the `split`, `columns` and `index` orients put the document's index into a column
named `index` (`liquers-records/src/formats/shapes.rs`, `index_field_name`). Each row object is
built by inserting `index` first and the data cells after it. A data column that is also named
`index` replaces the index value in the row's `serde_json::Map`, and nothing reports it.

**Example.** `from_json(r#"{"columns":["index","a"],"index":[0,1],"data":[[50,1],[60,2]]}"#,
JsonOrient::Split, ReadSchema::Infer)` returns the columns `index, a`, with `index` = `50, 60`. The
index `0, 1` is lost. The `columns` orient `{"index":{"0":5},"a":{"0":1}}` gives `index` = `5`, and
the row key `0` is lost. Expected: the read either keeps both or refuses the document. It should not
drop data silently.

## Impact

Silent data loss on a schema-less read. pandas writes a frame that has a column named `index` this
way (`df.to_json(orient="split")`). A declared schema avoids the problem, because the `Id` field
takes the schema's own name and field names are unique.

## Expected behaviour

The index and the data column both survive, or the read fails with an error that names the clash.
Designed in design/json-orient-index-column-collision/ — readiness: needs-decision; automatic
fixing: not-eligible.

## Discovery

Found 2026-10-08 while implementing `design/ordered-json-orient-column-order/` (branch
`claude/ordered-json-orient-column-order`, PR #90). A probe test against the `split` and `columns`
orients reproduced it. The behaviour predates that change.
