# Phase 1: High-Level Design — SQL over a table (`ns-pl/sql`)

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a command (`specs/command_registry.yaml` changes) and
  turns on a dependency feature (`polars/sql`) (auto-fix rule 4)
- **Leading issue:** **Open design - feature placement:** whether `polars/sql` is enabled by the
  existing `polars` feature or by a new one. Recommended: by `polars` (see Q1).
- **Explanation:** a bounded first slice is fully specified: one command evaluating one SQL
  `SELECT` over its input table through polars' own SQL engine, which is already in `Cargo.lock`.
  Joins across several entries, a wasm-capable engine and an external SQL mirror are split off or
  covered elsewhere, so nothing unresolved remains inside the slice except the visible choices below.
- **Open questions:**
  1. **Open design - feature placement:** enable `polars/sql` from the `polars` feature (no new
     matrix row, the registry export and its six-group gates are unchanged) or behind a new
     `sql` feature (opt-out possible, but one more feature for `check-build-matrix.sh`, the
     registry gates and `liquers-web`'s forwarding). Recommended: from `polars`.
  2. **Proposed resolution - name and table name:** the command is `ns-pl/sql` and the input table
     is called `self`, as in polars' Python `DataFrame.sql(query, table_name="self")`
     (`POLARS_COMMAND_LIBRARY.md` §"Conventions: polars first"). Alternative: an `ns-sql`
     namespace — rejected because the engine and result are polars'.
  3. **Proposed resolution - read-only SQL surface:** only one query statement (`SELECT`, `WITH`,
     set operations) is accepted, and every relation it reads must be `self` or a CTE it defines.
     This refuses polars' file-reading table functions (`read_csv(…)` and others), which would
     bypass the store and its dependency tracking — a security requirement for `liquers-axum`.

## Purpose

Let a user group, aggregate, project and filter a table with one SQL statement, so that "rows per
region with their total" is a query, not a new command. Serves VISION priority area 2 (DataFrames
and records) and "useful out of the box": `ns-pl` has no grouping today
(`POLARS_COMMAND_LIBRARY.md` §"Grouping & Aggregation": "Out of scope").

## Problem Example

```
-R/data/sales.csv/-/ns-pl/sql-~X~-R/queries/by_region.sql/-/to_text~E
```

with `queries/by_region.sql` holding `SELECT region, sum(amount) AS total FROM self GROUP BY region
ORDER BY region`. Today no `sql` command exists; the only route is a hand-written command. Should
be: a `polars.DataFrame` with one row per region and columns `region`, `total`, cached like any
other `ns-pl` result and dependent on both the CSV and the `.sql` file.

## Scope and Acceptance Criteria

- **AC-1** Grouping through SQL
  WHEN the Problem Example is evaluated over a CSV with regions EU, EU, US and amounts 1, 2, 5
  THEN the result is a `polars.DataFrame` with rows (EU, 3) and (US, 5)
- **AC-2** SQL written inline
  WHEN the SQL is written in the query with entities
  (`sql-SELECT~.region~ncomma~~.sum~nlpar~amount~nrpar~~.AS~.total~.FROM~.self~.GROUP~.BY~.region`)
  THEN it gives the same result as AC-1
- **AC-3** Any table input
  WHEN the input is a `RecordView` (e.g. after `ns-rec/to_record`) or CSV/Parquet text or bytes
  THEN it is read as `ns-pl` reads every input (`try_to_polars_dataframe`), and a `RecordSource`
  is refused naming `ns-rec/materialize`
- **AC-4** SQL errors are Liquers errors
  WHEN the SQL does not parse, or names a missing column
  THEN the asset fails with an `Error` whose message carries polars' message, and nothing panics
- **AC-5** Read-only surface
  WHEN the SQL names a relation other than `self` or its own CTEs (`FROM other`,
  `FROM read_csv('/etc/passwd')`), or is not a single query statement (`DROP TABLE self`, two
  statements)
  THEN the command fails before running anything, naming the relation or the statement kind
- **AC-6** Dependencies and caching
  WHEN the `.sql` file changes after AC-1 was evaluated
  THEN the cached result is no longer reused (the link is a recorded dependency), and an empty
  SQL text is an error, not "return the input"

Non-goals: joins across several stored entries (split off:
[`SQL-COMMAND-CANNOT-JOIN-SEVERAL-TABLES`](../../issues/SQL-COMMAND-CANNOT-JOIN-SEVERAL-TABLES.md));
SQL in wasm builds, where polars does not build (split off:
[`NO-SQL-ENGINE-IN-WASM-BUILDS`](../../issues/NO-SQL-ENGINE-IN-WASM-BUILDS.md)); feeding an external
SQL database (covered by `external-index-sync`); writes of any kind; a new value type.

## Core Interactions

- **Commands:** one new command `sql` in namespace `pl`.
- **Value types:** none new — input as every `ns-pl` command, output `ExtValue::PolarsDataFrame`.
- **Query:** none; SQL travels as an ordinary string parameter or a link.

## Crate Placement

`liquers-lib/src/polars/` (new `sql.rs`), under the existing `polars` feature (Q1). No crate below
`liquers-lib` changes; `liquers-records` stays engine-free.

## Documentation Intent

- Reference: extend `specs/reference/POLARS_COMMAND_LIBRARY.md` (a `sql` section; grouping
  points to it).
- Guide: none — one command; its reference section carries the spelling advice.
- Other documents: `specs/command_registry.yaml` (generated).
- Documents to update: `specs/reference/RECORD_STREAMS.md` §"`ns-rec` commands" comparison table
  (one row: SQL is `ns-pl` only).

## Design Dependencies

- `external-index-sync` — **overlaps**: owns the external-SQL-mirror half (`interoperability-layer.md` §6).
- `relational-database-access` — **overlaps** (weak): the mirror image, Liquers over a database.
  Shares nothing but the record abstraction; no ordering.
- `record-filter-commands` (complete) — conventions this follows (polars-first names, `ns-pl`
  input rules).

## Consolidated Findings

- **Smallest slice:** polars' own SQL engine (`polars-sql` 0.55.2, `sqlparser` 0.62.0) is already
  in `Cargo.lock`; enabling it costs compile time, no new package.
- **Spelling:** the recommended form is a link to a stored `.sql` file (also a dependency). Inline,
  a named entity ends with `~`, so a following space is `~~.` (`~ncomma~~.`) — validator-confirmed.
- **Security:** polars SQL can read files through table functions; the AC-5 allow-list is mandatory.
- The source's other halves are split off or covered (Non-goals); the issue was re-sized L → M.

## References

- Source: [`NO-SQL-QUERY-CAPABILITY-OVER-STORED-AND-DERIVED-DATA`](../../issues/NO-SQL-QUERY-CAPABILITY-OVER-STORED-AND-DERIVED-DATA.md)
- `specs/design/store-and-asset-search/research-questions.md` §4 (GlueSQL), `specs/design/external-index-sync/interoperability-layer.md` §6
- `specs/reference/POLARS_COMMAND_LIBRARY.md`, `specs/reference/RECORD_STREAMS.md`
