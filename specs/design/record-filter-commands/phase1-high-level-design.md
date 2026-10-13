# Phase 1: High-Level Design — Record and polars command interoperability

> **Revision 2, 2026-10-13.** This revision widens the scope of the compact draft of 2026-10-11, which
> covered only the `ns-rec` filters and `sort` (AC-1 to AC-8, kept unchanged). It now covers all of
> `RECORD-AND-POLARS-COMMANDS-NOT-INTEROPERABLE` (AC-9 to AC-16). Because the scope is now size L,
> the design was converted from the compact form to the full form. See §Scope Changes.

## Purpose

`ns-rec` (records) and `ns-pl` (polars DataFrames) are two table libraries that grew separately. A
query cannot move from one to the other. Records cannot be filtered or sorted, and `ns-pl` refuses
records unless they are first written to a file. This design gives both libraries one vocabulary.
Each namespace will accept the other's values, and `ns-rec` gains the row operations it lacks. A
search result can then be narrowed, ordered and aggregated in one query, and a recipe can drop from
`ns-pl` to `ns-rec` where polars is unavailable (the wasm build).

## Problem Example

Select the designs in the architecture phase, newest first:

```
-R-key/specs/design/-/ns-search-rec/catalog/eq-filename-DESIGN.md/field_extract/eq-phase-architecture/sort-created-f
```

**Today.** `liquers-validate` rejects the query: `Action 'eq' not registered in namespaces 'search',
'rec', '', 'root'`. The filters can be written only in the search syntax, with escapes:
`…/catalog/search-filename~ncolon~DESIGN.md~.phase~ncolon~architecture-f-f-0`, which validates.
There is no way to sort at all. Switching to polars, as in `…/field_extract/ns-pl/eq-phase-architecture/sort-created`,
plans correctly, but at run time `try_to_polars_dataframe`
(`liquers-lib/src/polars/util.rs`) refuses the `RecordView`, because it accepts only a DataFrame or
serialized CSV/Parquet.

**After.** Both forms run and return the same rows in the same order. The `ns-pl` form returns a
DataFrame, and the `ns-rec` form returns a record view that keeps its Id and Source roles.

All queries on this page were checked with `liquers-validate`. A command that does not exist yet
was declared with `--command`, and each `encoded` form matched the text.

## Scope and Acceptance Criteria

AC-1 to AC-8 are unchanged from the compact draft. Ids are never renumbered.

- **AC-1** Comparison filters
  WHEN `ns-rec/eq`, `ne`, `gt`, `gte`, `lt` or `lte` is applied with a column and a value
  THEN only the rows satisfying the comparison remain. The value is parsed to the column's type,
  and ISO dates compare correctly on Text and Timestamp columns.
- **AC-2** Text filters
  WHEN `startswith`, `endswith`, `contains` or `glob` is applied (case-sensitive by default)
  THEN only the matching rows remain.
- **AC-3** Membership and nulls
  WHEN `isin` is given a column and several values, or `isnull` / `notnull` a column
  THEN only the matching rows remain.
- **AC-4** Expression filter
  WHEN `ns-rec/filter-<expr>` is applied
  THEN the result is that of `ns-search/search-<expr>` with `rank=false` and `limit=0`.
- **AC-5** Lazy over sources
  WHEN a filter's input is a `RecordSource`
  THEN the result is a lazily filtered `RecordSource`, and nothing is materialized.
- **AC-6** Unknown column
  WHEN the named column does not exist
  THEN the command fails, naming the column and the available ones.
- **AC-7** Sort
  WHEN `sort-<column>` is applied, optionally with `ascending` false
  THEN the rows are ordered by that column, stably, with nulls last. A source is materialized
  first. The signature is `pl/sort`'s.
- **AC-8** List cells
  WHEN a cell holds a JSON array, as `field_extract` writes lists
  THEN a filter matches when any element matches.
- **AC-9** One vocabulary
  WHEN an operation exists in both `ns-rec` and `ns-pl`
  THEN it has the same name and argument list in both, and gives the same rows on the same data.
  Every deliberate difference (for example, `select_columns` keeping Id columns) is stated in the
  command's `doc:` and in a comparison table in the reference documentation.
- **AC-10** `ns-pl` accepts records
  WHEN an `ns-pl` command receives a `RecordView`
  THEN it runs on the converted DataFrame, as it would on that DataFrame directly. A
  `RecordSource` is refused, and the error names `ns-rec/materialize` (see Open Question 1).
- **AC-11** `ns-rec` accepts DataFrames
  WHEN an `ns-rec` command receives a `PolarsDataFrame` (feature `polars` on)
  THEN it runs on the converted record batch.
- **AC-12** Missing row operations
  WHEN `ns-rec/tail-<n>` or `ns-rec/drop_columns-<columns…>` is applied
  THEN the result matches `ns-pl`'s command of the same name. `drop_columns` refuses to drop an
  Id column, as `select_columns` keeps one.
- **AC-13** Shape and counts
  WHEN `ns-rec/nrows`, `ncols`, `shape` or `count` is applied
  THEN the result has the same form as the `ns-pl` command. Over a source, it is computed chunk by
  chunk without concatenating the chunks.
- **AC-14** Schema naming
  WHEN a user asks for a table's schema with `schema` in either namespace
  THEN both namespaces answer. `rec/records_schema` remains as an alias.
- **AC-15** Without polars
  WHEN `liquers-lib` is built with `records` but without `polars` (including wasm32)
  THEN every `ns-rec` command in AC-1 to AC-14 is available and its tests pass.
- **AC-16** LazyFrame investigation
  WHEN Phase 2 is reviewed
  THEN it contains a recorded outcome on backing a polars `LazyFrame` with a `RecordSource` at
  polars 0.55 (scan API stability, zero-copy chunk conversion, how a `LazyFrame` value would be
  represented). The outcome is either a follow-up design filed as an issue or "not worth it" with
  the reason. Building the scan is not in scope.

Non-goals: aggregations, `describe` and grouping in `ns-rec` (they stay polars-only); joins; a new
expression language; predicate pushdown into an external engine (`EXTERNAL-INDEX-SYNC`); keeping
record roles through a round trip through polars (the bridge drops them, as documented).

## Core Interactions

- **Commands:** new `ns-rec` commands, and the `ns-pl` input conversion (`try_to_polars_dataframe`).
  `specs/command_registry.yaml` is regenerated.
- **Value types:** `ExtValue::RecordView`, `RecordSource` and `PolarsDataFrame` (no new variant)
  are converted through the existing bridge, `liquers-lib/src/records/polars.rs`.
- **Records:** the search kernels (`SearchExpr`, `search_view`, `FilteredSource` in
  `liquers-records/src/search/`) and the `RecordView` views (`take`, `filter`).
- **Query:** no syntax change.

## Crate Placement

- The command glue goes in `liquers-lib/src/records/`, as `ns-rec` already does. The polars input
  conversion goes in `liquers-lib/src/polars/util.rs`, gated on `records` and `polars` together.
- A sort or count kernel that needs no `Value` goes in `liquers-records`, which keeps its
  dependency on `liquers-core` only.
- Nothing goes in `liquers-core`.

## Documentation Intent

- Reference: extend `specs/reference/RECORD_STREAMS.md` with the `ns-rec` command list, and
  `specs/reference/POLARS_COMMAND_LIBRARY.md` with the accepted inputs. Add one rec ↔ pl comparison
  table (AC-9), kept in `RECORD_STREAMS.md`, with a link to it from the polars reference.
- Guide: extend `specs/guides/RECORD_STREAM_GUIDE.md` with filtering and sorting records, and when
  to switch to `ns-pl`.
- Other documents: `specs/design/record-filter-commands/lazyframe-investigation.md` (AC-16).
- Documents to update: `specs/command_registry.yaml` (regenerated), `specs/README.md` (link the
  design).

## Design Dependencies

| Design | Relation | Effect |
|---|---|---|
| `store-and-asset-search` | requires (complete) | Supplies `SearchExpr`, `search_view`, `FilteredSource`, `search_input`; met |
| `agent-memory-mvp` | required-by | Its execution plan (D7) needs filters, `sort` and a count; AC-1 to AC-7 and AC-13 serve it |
| `record-streams` | requires (complete) | Views, sources, and the polars bridge |

Weak overlaps (no merge): `POLARS-BRIDGE-VECTOR-COLUMNS-REFUSED`, which limits AC-10/AC-11 for Vector
columns (the conversion error stands); `RECORDS-ARROW-C-DATA-EXPORT-NOT-BUILT`, which shares AC-16's
zero-copy question; `EXTERNAL-INDEX-SYNC`, which owns pushdown.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. The source is size L, and the fix adds public commands and
  changes command contracts.
- **Leading issue:** Question 1 (how `ns-pl` reaches a source asynchronously)
- **Explanation:** Each criterion has a working answer, built on kernels that already exist. The
  questions below change command contracts, so they are listed for the gate rather than decided
  silently.
- **Open questions:**
  1. *Open design.* The `ns-pl` commands are synchronous and take only `&State`, but materializing a
     `RecordSource` is asynchronous. **Recommendation:** accept a `RecordView` in the synchronous
     path, and refuse a source, naming `ns-rec/materialize`. Alternatively, make the `ns-pl`
     commands async with `context`, which changes every `ns-pl` signature and its `impl_version`.
     AC-10 follows the recommendation. If the gate chooses the async route, AC-10 widens to accept a
     source within the materialization limit.
  2. *Open design.* Should the text filters be case-sensitive by default, or follow `search`, which
     is case-insensitive? **Recommendation:** case-sensitive, because these are exact filters.
  3. *Proposed resolution.* `sort` takes a single column, to keep `pl/sort`'s signature. Sorting by
     several columns would change both commands together, and is left for later.
  4. *Proposed resolution.* `schema` is added to `ns-rec`, and `records_schema` is kept as an alias
     (AC-14). The output stays in each namespace's own form (records versus DataFrame).
  5. *Open design.* Should `ns-pl` gain `filter-<expr>`, `startswith` and `isin` so that the
     vocabulary is symmetric (AC-9)? **Recommendation:** add `isin` and the text filters, which map
     directly onto polars expressions. Leave `filter-<expr>` records-only, because it is the search
     language.

## Scope Changes

- **2026-10-13: absorbed the remainder of `RECORD-AND-POLARS-COMMANDS-NOT-INTEROPERABLE`** (triage
  case 1: the source issue of the design in hand). Added AC-9 to AC-16. Size grew from M to L, so
  the design was converted to the full form, and the compact draft's Phase 2 notes moved to
  `phase2-architecture.md` as unreviewed input. Example: the Problem Example's `ns-pl` form.
  Approval: pending at this gate.

## References

- `specs/issues/RECORD-AND-POLARS-COMMANDS-NOT-INTEROPERABLE.md` (source)
- `specs/design/store-and-asset-search/phase1-high-level-design.md` (former AC-27)
- `specs/reference/RECORD_STREAMS.md`, `specs/reference/POLARS_COMMAND_LIBRARY.md`
