---
id: RECORD-FILTER-COMMANDS
kind: design
title: Simple filter and sort commands for records (ns-rec)
form: compact
workflow: liquers-project
status: draft
phase: high-level
area: [records, lib/commands]
issues: []
created: 2026-10-11
---
# Simple filter and sort commands for records (`ns-rec`)

Split out of [`store-and-asset-search`](../store-and-asset-search/) on 2026-10-11, where these
commands were AC-27 of its revision-8 Phase 1. They reuse that design's matching kernels, so they
are built after it.

## Phase 1: High-Level Design

### Purpose

Select rows of a record view or source with one condition per command, written without escaping.
The search syntax can express every one of these filters. But in a query, `*`, `:`, `(`, `)` and
`|` must be escaped (`~nast~`, `~ncolon~`, …), so a person writing a URL by hand, or a recipe
author, is better served by `eq-phase-architecture` than by
`search-phase~ncolon~architecture-f-0`. `ns-pl` already offers `eq`, `ne`, `gt`, `gte`, `lt` and
`lte` over DataFrames. This design gives records the same vocabulary.

### Problem Example

Select the designs that are in the architecture phase:

```
-R-key/specs/design/-/ns-search-rec/catalog/eq-filename-DESIGN.md/field_extract/eq-phase-architecture
```

Today there is no `ns-rec` filter, so this needs the search syntax with escapes:
`…/search-filename~ncolon~DESIGN.md~.phase~ncolon~architecture-f-0`. Both forms are validated with
`liquers-validate`.

### Scope and Acceptance Criteria

- **AC-1** Comparison filters
  WHEN `eq`, `ne`, `gt`, `gte`, `lt` or `lte` is applied with a column and a value
  THEN only the rows satisfying the comparison remain. The value is parsed to the column's type,
  and ISO dates compare correctly on Text and Timestamp columns.
- **AC-2** Text filters
  WHEN `startswith`, `endswith`, `contains` or `glob` is applied (case-sensitive by default)
  THEN only the matching rows remain.
- **AC-3** Membership and nulls
  WHEN `isin` is given a column and several values, or `isnull` / `notnull` a column
  THEN only the matching rows remain.
- **AC-4** Expression filter
  WHEN `filter-<expr>` is applied
  THEN the result is that of `ns-search/search-<expr>` with `rank=false` and `limit=0`.
- **AC-5** Lazy over sources
  WHEN the input is a `RecordSource`
  THEN the result is a lazily filtered `RecordSource`, and nothing is materialized.
- **AC-6** Unknown column
  WHEN the named column does not exist
  THEN the command fails, naming the column and the available ones.
- **AC-7** Sort
  WHEN `sort-<column>` is applied, optionally with `descending` true
  THEN the rows are ordered by that column, stably, with nulls last. A source is materialized
  first.
- **AC-8** List cells
  WHEN a cell holds a JSON array, as `field_extract` writes lists
  THEN a filter matches when any element matches.

Non-goals: aggregation, grouping and joins (polars or SQL); a new expression language.

### Open Questions

1. Should the text filters default to case-sensitive (as here) or follow `search`
   (case-insensitive)? The recommendation is case-sensitive, because these are exact filters.
2. Should `sort` accept several columns (`sort-a-b`) through a `multiple` argument?

## Phase 2: Architecture

### Solution

`liquers-lib/src/records/filters.rs`, registered in `register_records_commands!`. Each command
builds a one-node `SearchExpr` directly (so nothing is parsed or escaped), then runs the
`rank = false`, `limit = 0` path of `ns-search/search`: `search_view` for a view, `FilteredSource`
for a source. `sort` uses `RecordView::take` with indices sorted by `Column::compare`.

```
async fn eq(state, column: String, value: String, context) -> result   // ne, gt, gte, lt, lte
async fn startswith(state, column: String, text: String, case_sensitive: bool = true, context) -> result
                                                                       // endswith, contains, glob
async fn isin(state, column: String, values: Vec<String> multiple, context) -> result
async fn isnull(state, column: String, context) -> result              // notnull
async fn filter(state, query: String = "", context) -> result
async fn sort(state, column: String, descending: bool = false, context) -> result
```

Names and the `column` argument mirror `ns-pl`. The input is converted as `ns-search/search`
converts it (`search_input`).

### Changes

- New file: `liquers-lib/src/records/filters.rs`.
- Updated: the registration in `liquers-lib/src/records/commands.rs`.
- Regenerated: `specs/command_registry.yaml`.
- Updated: `specs/reference/RECORD_STREAMS.md`, with the command list.

### Risks

The design depends on `store-and-asset-search` for `SearchExpr`, `search_view` and
`FilteredSource`. It is otherwise small and additive.

## Phase 3: Examples and Tests
### Examples
### Tests

## Phase 4: Implementation Plan
### Steps
### Validation

## Phase 5: Documentation
