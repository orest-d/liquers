---
id: RECORD-AND-POLARS-COMMANDS-NOT-INTEROPERABLE
kind: feature
title: Record and polars commands differ in names and inputs, so a query cannot move between them
status: closed
priority: P2
complexity: L
area: [records]
design: record-filter-commands
created: 2026-10-11
github:
---
## Resolution

Resolved by `specs/design/record-filter-commands/`, implemented 2026-10-13 on branch
`claude/kind-goldberg-9oxjza`. Against the four expected behaviours:

1. **Same names, same meaning.** `ns-rec` uses `ns-pl`'s names and arguments (polars first;
   `conventions.md`). `rec/schema` was added, with `records_schema` as an alias; `pl/schema` keeps its
   polars form, by decision. The deliberate differences are tabled in
   `specs/reference/RECORD_STREAMS.md` §"`ns-rec` commands". `records_table_ops.rs`
   `shared_commands_have_the_same_arguments` checks every shared command.
2. **Each accepts the other's values.** `ns-pl` accepts a `RecordView` and refuses a source, naming
   `ns-rec/materialize`; `ns-rec` accepts a `polars.DataFrame` (`pl_commands_accept_a_record_view`,
   `rec_commands_accept_a_dataframe`).
3. **`ns-rec`'s gaps are filled:** filters, `sort`, `tail`, `drop_columns`, `nrows`, `ncols`,
   `shape`, `count`.
4. **`RecordSource` as a `LazyFrame`: not worth it at polars 0.55**, because `AnonymousScan` is
   synchronous and returns one whole frame. Laziness lives in `ns-rec` instead
   (`lazyframe-investigation.md`).

## Problem

`ns-rec` (records, `liquers-lib/src/records/commands.rs`) and `ns-pl` (polars DataFrames,
`liquers-lib/src/polars/`) are two table libraries that grew separately. Three gaps follow.

**1. `ns-rec` has almost no row operations.** It can project (`select_columns`), take rows
(`head`, `slice`, `row`, `rowid`, `rec_id`), convert and serialize. It cannot filter or sort, and
it has no `tail`, `drop_columns`, `shape` or counts. `ns-pl` has `eq`, `ne`, `gt`, `gte`, `lt`,
`lte`, `sort`, `tail`, `drop_columns`, `count`, `describe`, `shape`, `nrows`, `ncols` and the
aggregations. The `record-filter-commands` design covers only the filters and `sort`.

**2. The names and meanings only partly agree.** Checked against `specs/command_registry.yaml` on
2026-10-11:

| Both namespaces | Agreement |
|---|---|
| `head(n = 5)`, `slice(offset, length)`, `select_columns(columns…)` | Same name and arguments. `rec/select_columns` also keeps the Id and Source columns |
| `pl/schema` ↔ `rec/records_schema` | Same purpose, different names |
| `pl/sort(column, ascending = true)` | `record-filter-commands` now drafts `rec/sort` with the same signature |

**3. `ns-pl` commands do not accept records.** `liquers-lib/src/polars/util.rs`
`try_to_polars_dataframe` takes a `PolarsDataFrame`, or CSV/Parquet text or bytes with a
`data_format`. A `RecordView` or `RecordSource` is refused, although
`liquers-lib/src/records/polars.rs` already converts both ways (`record_batch_to_dataframe`,
`dataframe_to_record_batch`). Records reach `ns-pl` only through a stored file. In the other
direction, `ns-rec` commands accept a DataFrame only if it has been serialized.

## Impact

A query cannot move from one library to the other. A search result (records) cannot be aggregated
or grouped with `ns-pl` without being written to CSV first. A recipe written against `ns-pl` cannot
run in the wasm build, where polars is unavailable, by switching to `ns-rec`. Each library must also
be learnt separately.

## Expected behaviour

1. **Same names, same meaning, where possible.** Every operation both libraries have uses one name
   and one argument list. A deliberate difference is documented next to the command (as for
   `select_columns` keeping Id columns). Decide whether `rec/records_schema` gains a `schema` alias,
   or the two are made to agree some other way.
2. **`ns-pl` accepts records.** `try_to_polars_dataframe` converts a `RecordView`, or a
   `RecordSource` after materializing it, through the existing bridge. Roles are lost, which the
   bridge already documents. Symmetrically, `ns-rec` commands should accept a `PolarsDataFrame`
   (`dataframe_to_record_batch`) when the `polars` feature is on.
3. **Fill `ns-rec`'s gaps** with the operations that make sense without polars: `tail`,
   `drop_columns`, `nrows`/`ncols`/`shape`, and `count`. Aggregations and `describe` stay polars-only
   unless a records implementation is cheap.
4. **Investigate a `RecordSource` as a polars `LazyFrame`.** A source is a re-openable stream of
   views, which is the shape a lazy scan consumes. polars lets a custom source back a `LazyFrame`
   (its anonymous-scan / custom-scan mechanism; the exact API at polars 0.55 needs checking). If
   that works, `ns-pl` could run over a record stream, such as a manifest of many Parquet chunks,
   without materializing it, and could push projections and, possibly, predicates down to the
   source. Questions:
   - whether the scan interface is stable at our polars version;
   - how a chunk's view becomes a polars `DataFrame` without copying (Arrow layout compatibility);
   - whether `ns-pl` commands should operate on `LazyFrame`s internally (many already call
     `.lazy()`), and how a `LazyFrame` value would be represented, since today only `DataFrame`
     is an `ExtValue`.
   The result is either a design or a recorded "not worth it" with the reason.

## Discovery

Raised by the user on 2026-10-11 while splitting the simple `ns-rec` filters out of
`store-and-asset-search` into `record-filter-commands`. The command-name comparison above was done
then against the exported registry. That `try_to_polars_dataframe` refuses records was confirmed in
`liquers-lib/src/polars/util.rs` at HEAD.
