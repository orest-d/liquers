---
id: NO-SQL-ENGINE-IN-WASM-BUILDS
kind: feature
title: SQL over records is unavailable in wasm builds, where polars does not build
status: draft
priority: P3
complexity: L
area: [records, lib/commands, web]
design:
created: 2026-10-21
github:
---

## Problem

The SQL command designed in `design/sql-over-tables/` runs on polars' SQL engine, behind the
`polars` feature, which does not build for `wasm32` (`liquers-lib/Cargo.toml`: "polars pulls
dependencies that do not build for wasm"). A browser-only deployment (`liquers-web`) therefore has
`ns-rec` filters and search but no SQL over its record views.

## Impact

Only wasm hosts. Server, desktop and Python builds have `ns-pl/sql`. No workaround in the browser
other than the `ns-rec` filters.

## Expected behaviour

A wasm-clean SQL engine over `RecordView` / `RecordSource`. The candidate named by
`store-and-asset-search/research-questions.md` §4 is GlueSQL, whose custom-storage `Store` trait
(rows and tables; unrelated to `AsyncStore`) would be implemented over a record view; it supports
schemaless `MAP`/`LIST` data. Questions: the type mapping to `FieldType`, whether its result is a
`RecordView`, the dependency weight in the wasm module (cf. the ~3 MB `ns-rec` cost noted in
`liquers-web/Cargo.toml`), and whether the same command name (`sql`) can be offered in an `ns-rec`
form so a query means the same on every host (VISION "one query language everywhere").

## Discovery

Split out of `NO-SQL-QUERY-CAPABILITY-OVER-STORED-AND-DERIVED-DATA` by the 2026-10-21 bulk design
(`design/sql-over-tables/`, Phase 2 rejected alternatives).
