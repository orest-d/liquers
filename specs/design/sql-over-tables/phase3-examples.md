# Phase 3: Examples & Use-cases — SQL over a table (`ns-pl/sql`)

All queries below were checked with `liquers-validate --command sql` (the command does not exist
yet); each plans to `GetAsset[data/sales.csv]` followed by one `sql` action whose parameter is
the intended SQL text or link.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Totals per region from a stored `.sql` | link spelling, dependency | AC-1, AC-6 |
| 2 | Example | The same SQL written inline | entity spelling | AC-2 |
| 3 | Test | `polars_commands::test_sql_group_by_inline` | grouping result | AC-1, AC-2 |
| 4 | Test | `polars_commands::test_sql_from_stored_file_link` | link to `.sql`, result | AC-1, AC-6 |
| 5 | Test | `polars_commands::test_sql_over_record_view` | `RecordView` input via the bridge | AC-3 |
| 6 | Test | `polars_commands::test_sql_refuses_record_source` | refusal names `ns-rec/materialize` | AC-3 |
| 7 | Test | `polars_commands::test_sql_error_is_reported` | bad SQL, unknown column | AC-4 |
| 8 | Unit | `polars::sql::tests::read_only_check_*` | allow-list | AC-5 |
| 9 | Test | `polars_commands::test_sql_empty_query_is_rejected` | empty SQL | AC-6 |
| 10 | Test | `polars_commands::test_sql_link_change_invalidates` | dependency on the `.sql` file | AC-6 |

## Example 1: totals per region from a stored `.sql`

Store `queries/by_region.sql`:
`SELECT region, sum(amount) AS total FROM self GROUP BY region ORDER BY region`.

```
-R/data/sales.csv/-/ns-pl/sql-~X~-R/queries/by_region.sql/-/to_text~E/sales_by_region.csv
```

`GetAsset[data/sales.csv]` → `sql(query = <text of the link>)` → `Filename[sales_by_region.csv]`.
Over `region,amount\nEU,1\nEU,2\nUS,5\n` the result is a `polars.DataFrame`:

| region | total |
|---|---|
| EU | 3 |
| US | 5 |

The asset's dependencies include `-R/queries/by_region.sql/-/to_text` (relation `ParameterLink`).

## Example 2: inline SQL

```
-R/data/sales.csv/-/ns-pl/sql-SELECT~.region~ncomma~~.sum~nlpar~amount~nrpar~~.AS~.total~.FROM~.self~.GROUP~.BY~.region
```

decodes to `SELECT region, sum(amount) AS total FROM self GROUP BY region` (checked against the
validator's decoded parameter). Writing `~ncomma~.sum` instead decodes to `region,.sum` — the
named entity's closing `~` is not a space.

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| `SELECT nope FROM self` | asset error containing `nope` | 7 |
| `SELEC x` | asset error carrying the parse message | 7 |
| `SELECT * FROM other` | refused before execution, message names `other` | 8 |
| `SELECT * FROM read_csv('/etc/passwd')` | refused, message names `read_csv` | 8 |
| `WITH t AS (SELECT * FROM self) SELECT * FROM t` | accepted (CTE alias) | 8 |
| `DROP TABLE self`; `SELECT 1; SELECT 2` | refused: not a single query statement | 8 |
| `SELECT * FROM self UNION ALL SELECT * FROM self` | accepted | 8 |
| `RecordSource` input (`…/ns-rec/to_record_source/ns-pl/sql-…`) | conversion error naming `ns-rec/materialize` | 6 |
| empty `query` (`ns-pl/sql`) | `general_error` "sql requires a query" | 9 |

## Test Plan

All in `liquers-lib/tests/polars_commands.rs` (file-gated on `polars`), reusing `eval_over_csv`
and `eval_state_over_csv`; the store-backed ones add a helper building `DefaultEnvironment` with
an `AsyncMemoryStore::new(&Key::new())` holding `data/sales.csv` and `queries/by_region.sql`.

- `test_sql_group_by_inline` (AC-1, AC-2): Example 2's action over the CSV above; asserts columns
  `region`, `total` and the two rows with `column_strings` / `cell_f64`.
- `test_sql_from_stored_file_link` (AC-1, AC-6): evaluates Example 1 against the store; same
  rows; asserts the result's dependency records contain the `.sql` link query.
- `test_sql_over_record_view` (AC-3): `ns-rec/to_record/ns-pl/sql-SELECT~.count~nlpar~~nast~~nrpar~~.AS~.n~.FROM~.self`
  over the CSV; one row, `n = 3`. Gated additionally on `records`
  (`#[cfg(feature = "records")]` on the test).
- `test_sql_refuses_record_source` (AC-3): `ns-rec/to_record_source` input; error message contains
  `ns-rec/materialize`. Gated on `records`.
- `test_sql_error_is_reported` (AC-4): unknown column and parse error both return `Err`, the
  former's message containing the column name.
- `test_sql_empty_query_is_rejected` (AC-6): `ns-pl/sql` with no parameter fails (the argument
  has no default; whether at plan or at execution is not asserted) and `sql-~.` (a single space) → `general_error`.
- `test_sql_link_change_invalidates` (AC-6): evaluate Example 1, overwrite `queries/by_region.sql`
  with `SELECT count(*) AS n FROM self` through the store, evaluate again; the second result has
  column `n`, not the cached `total`.
- Unit, `liquers-lib/src/polars/sql.rs` `mod tests` (AC-5): `read_only_check_accepts_self`,
  `read_only_check_accepts_cte_and_union`, `read_only_check_refuses_other_table`,
  `read_only_check_refuses_table_function`, `read_only_check_refuses_ddl`,
  `read_only_check_refuses_two_statements`. Each asserts `Ok` or that the error message names
  the relation or statement.
- Command: `CARGO_INCREMENTAL=0 cargo test -p liquers-lib --test polars_commands sql` and
  `cargo test -p liquers-lib --lib polars::sql`.

The inline count query in `test_sql_over_record_view` decodes to `SELECT count(*) AS n FROM self`
(`~nlpar~~nast~~nrpar~~.` = `(`, `*`, `)`, space); validate it with `liquers-validate` when the
test is written, as for every query here.

## Learning Log

- A named entity (`~ncomma~`) carries its own closing `~`; a following space is a separate `~.`.
  Worth a line in `POLARS_COMMAND_LIBRARY.md`.
- Whether the AC-6 invalidation test passes depends on the store write expiring dependents — the
  existing dependency machinery, not this design. If it does not, the failure is a core issue to
  file, not a reason to weaken AC-6.
