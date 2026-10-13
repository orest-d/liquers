# Phase 1: High-Level Design — Record and polars command interoperability

> **Revision 3, 2026-10-13.** This revision applies the gate decisions on Revision 2. `ns-pl`
> commands become async over a `LazyFrame` and return a DataFrame. Text filters are
> case-insensitive by default. polars sets the naming and argument conventions
> ([`conventions.md`](./conventions.md)). The LazyFrame question is answered in
> [`lazyframe-investigation.md`](./lazyframe-investigation.md).
>
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
  WHEN `starts_with`, `ends_with`, `contains` or `glob` is applied, with
  `case_sensitive` (default `false`, as in `ns-search/search`)
  THEN only the matching rows remain.
- **AC-3** Membership and nulls
  WHEN `is_in` is given a column and several values, or `is_null` / `is_not_null` a column
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
  WHEN `sort-<column>` is applied, optionally with `ascending` (default `true`) and `nulls_last`
  (default `false`, as in polars)
  THEN the rows are ordered by that column, stably, in both namespaces. A source is materialized
  first. `ns-rec/sort` and `ns-pl/sort` have the same signature.
- **AC-8** List cells
  WHEN a cell holds a JSON array, as `field_extract` writes lists
  THEN a filter matches when any element matches.
- **AC-9** One vocabulary
  WHEN an operation exists in both `ns-rec` and `ns-pl`
  THEN it has the same name and argument list in both, and gives the same rows on the same data.
  Every deliberate difference (for example, `select_columns` keeping Id columns) is stated in the
  command's `doc:` and in a comparison table in the reference documentation.
- **AC-10** `ns-pl` accepts records
  WHEN an `ns-pl` command receives a `DataFrame`, a `RecordView` or a `RecordSource`
  THEN it runs on a `LazyFrame` built from that input, and returns the collected result as a
  `PolarsDataFrame` value. A source is read chunk by chunk without blocking. A source larger
  than the materialization limit is refused, and the error names the limit.
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
  THEN both namespaces answer. `rec/schema` returns the `RecordSchema` document (§Schema
  Example) that a `schema` argument or a manifest's `uniform_schema` accepts.
  `rec/records_schema` remains as an alias. **The round trip holds:** linking `rec/schema`'s
  output into a `schema` argument gives back an equal schema for every field role. That includes
  the `FullText` roles `catalog` writes, and a hand-written `role` that omits `stored` or `fast`.
  Neither holds today (see §Schema Example).
- **AC-15** Without polars
  WHEN `liquers-lib` is built with `records` but without `polars` (including wasm32)
  THEN every `ns-rec` command in AC-1 to AC-14 is available and its tests pass.
- **AC-16** LazyFrame investigation
  WHEN the design is reviewed
  THEN [`lazyframe-investigation.md`](./lazyframe-investigation.md) records the outcome. It is
  done: polars' `AnonymousScan` is synchronous and returns one whole frame, so the commands build
  the `LazyFrame` themselves (AC-10). No `LazyFrame` value type is added.
- **AC-17** Conventions
  WHEN a command exists, or is added, in `ns-rec` or `ns-pl`
  THEN its name, argument names and defaults follow [`conventions.md`](./conventions.md):
  polars first, `ns-rec` copies `ns-pl`, and a future pandas namespace adapts to them. The
  reference documentation states where pandas differs.

Non-goals: aggregations, `describe` and grouping in `ns-rec` (they stay polars-only); joins; a new
expression language; predicate pushdown into an external engine (`EXTERNAL-INDEX-SYNC`); keeping
record roles through a round trip through polars (the bridge drops them, as documented).

## Core Interactions

- **Commands:** new `ns-rec` commands. Every `ns-pl` command becomes `async` with `context`,
  so every `ns-pl` signature and `impl_version` changes. The input conversion
  (`try_to_polars_dataframe`) gains an async counterpart that returns a `LazyFrame`.
  `specs/command_registry.yaml` is regenerated. Query spellings stay valid, because `ascending`
  becomes a `bool`, which still reads `t` and `f`.
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
- **Leading issue:** Question 1 (`pl/schema`'s output form)
- **Explanation:** Each criterion has a working answer, built on kernels that already exist. The
  questions below change command contracts, so they are listed for the gate rather than decided
  silently.
- **Open questions:**
  1. *Open design.* `pl/schema` today returns `{column: "<polars dtype>"}`, while `rec/schema`
     returns a `RecordSchema` document (§Schema Example). **Recommendation:** keep both forms,
     because each feeds its own namespace, and document the difference (AC-9). Alternative: make
     `pl/schema` return a `RecordSchema` derived by the bridge, which changes an existing output.
  2. *Proposed resolution.* The new commands take polars names (`is_in`, `is_null`,
     `is_not_null`, `starts_with`, `ends_with`), following the polars-first rule. These replace the
     pandas-style names (`isin`, `isnull`, `startswith`) proposed in Revision 2.
  3. *Proposed resolution.* `ns-pl/sort` becomes stable (`maintain_order`) and gains
     `nulls_last: bool = false`, so that both namespaces give identical order (AC-7, AC-9).
  4. *Implementation detail.* Phase 2 decides whether `collect` runs on `spawn_blocking`, and
     whether row-local commands are applied chunk by chunk
     ([`lazyframe-investigation.md`](./lazyframe-investigation.md) §Consequences).

### Decisions at the Revision 2 gate (2026-10-13)

| # | Question | Decision |
|---|---|---|
| 1 | Records into `ns-pl` | `ns-pl` becomes async over a `LazyFrame`, and always returns a collected DataFrame (AC-10, AC-16) |
| 2 | Case sensitivity | `case_sensitive` flag, default `false`, as in `search` (AC-2) |
| 3 | Sort | One column with an `ascending` flag. polars sets the conventions, and pandas will adapt (AC-7, AC-17) |
| 4 | `schema` | Added to `ns-rec`, with `records_schema` kept as an alias. Its form is shown in §Schema Example |
| 5 | `ns-pl` symmetry | Add `is_in` and the text filters to `ns-pl`. `filter-<expr>` stays records-only |

## Schema Example

A schema is a `RecordSchema` document in YAML or JSON. It is what `rec/schema` returns, what the
`schema` argument of `to_record` and `from_json` accepts, and what a manifest holds under
`uniform_schema:`. Below is an order table with one field of each kind:

```yaml
type_identifier: shop.Order          # optional: what each row describes
fields:
  - name: order_id
    data_type: Text
    nullable: false
    key: Id                          # at most one; gets role {indexed: [Exact], stored: true}
  - name: placed_at
    data_type: Timestamp             # µs since epoch, UTC
    nullable: false
    role: { indexed: [Range], stored: true, fast: true }
  - name: customer
    label: Customer name             # default would be "customer"
    data_type: Text
    role: { indexed: [Exact, !FullText { analyzer: Simple, positions: true }], stored: true, fast: false }
  - name: amount
    data_type: Float
    description: Order total in EUR
    role: { indexed: [Range], stored: true, fast: true }
  - name: tags
    data_type: Text
    media_type: application/json     # cells hold JSON arrays; filters match any element (AC-8)
  - name: origin
    data_type: UInt
    key: Source                      # index into the batch's origin dictionary
```

In YAML, an index kind with settings is written as a tag (`!FullText { … }`), as `serde_yaml`
0.9 requires. Omitted keys take defaults: `nullable` is `true`, `key` is `None`, `role` is not indexed, and
`label` is `name` with `_` replaced by a space. A `role`, when given,
must currently list `indexed`, `stored` and `fast`. `RecordSchema::new` checks the document, and
refuses a duplicate name or a second `Id` or `Source` field.

In a query, a stored schema is linked into the `schema` argument. A schema can also be read back
from either namespace:

```
-R/data/orders.csv/-/ns-rec/to_record-csv-t-~X~-R/data/orders.schema.yaml~E/schema
-R/data/orders.csv/-/ns-rec/to_record-csv-t-~X~-R/data/orders.schema.yaml~E/ns-pl/schema
```

The first query returns the document above. The second returns `pl/schema`'s current form, with
roles lost through the bridge: `{order_id: "String", placed_at: "Datetime(…, UTC)", amount:
"Float64", …}`. See Open Question 1.

**Checked on 2026-10-13** by deserializing the document above into `RecordSchema` with a scratch
test. It parses. Two defects surfaced, and AC-14 now covers both:

- `records_schema` writes JSON, where `FullText` is the map `{"FullText": {…}}`. The `schema`
  argument is parsed with `serde_yaml`, which refuses that map ("expected a YAML tag starting
  with '!'"). So the documented "links into a `schema` argument" fails for any schema with a
  full-text, stemming, named-analyzer or similarity role, including every `catalog` output.
- `FieldRole` has no per-field serde defaults, so a `role` without `fast` is refused.

## Scope Changes

- **2026-10-13, Revision 3: gate decisions applied.** AC-2, AC-3, AC-7, AC-10, AC-14 and AC-16
  were revised, and AC-17 was added. AC-14 also took the schema round-trip defect found while
  checking §Schema Example (triage case 1: same command, same contract, so no separate issue). Size stays L. `conventions.md` and
  `lazyframe-investigation.md` were added. Approval: pending at this gate.

- **2026-10-13: absorbed the remainder of `RECORD-AND-POLARS-COMMANDS-NOT-INTEROPERABLE`** (triage
  case 1: the source issue of the design in hand). Added AC-9 to AC-16. Size grew from M to L, so
  the design was converted to the full form, and the compact draft's Phase 2 notes moved to
  `phase2-architecture.md` as unreviewed input. Example: the Problem Example's `ns-pl` form.
  Approval: pending at this gate.

## References

- `specs/issues/RECORD-AND-POLARS-COMMANDS-NOT-INTEROPERABLE.md` (source)
- `specs/design/store-and-asset-search/phase1-high-level-design.md` (former AC-27)
- `specs/reference/RECORD_STREAMS.md`, `specs/reference/POLARS_COMMAND_LIBRARY.md`
