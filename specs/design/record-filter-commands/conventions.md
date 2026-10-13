# Naming and argument conventions — polars first

**Maintainer decision, 2026-10-13.** polars is the main source of conventions. `ns-rec` copies
`ns-pl`. A future pandas namespace will be adapted to these conventions, not the other way
round. Where `ns-pl` already deviates from polars, the existing `ns-pl` spelling stands, because
renaming it would break queries.

| Operation | polars (Rust/Python) | pandas | `ns-pl` / `ns-rec` (this design) |
|---|---|---|---|
| Comparison | `eq`, `neq`, `gt`, `gt_eq`, `lt`, `lt_eq` | `==`, `.eq`, `.ne`, `.gt`, `.ge`, `.lt`, `.le` | `eq`, `ne`, `gt`, `gte`, `lt`, `lte` (existing `ns-pl` names) |
| Membership | `is_in` | `isin` | `is_in` |
| Nulls | `is_null`, `is_not_null` | `isna`/`isnull`, `notna`/`notnull` | `is_null`, `is_not_null` |
| Prefix / suffix / substring | `str.starts_with`, `str.ends_with`, `str.contains` | `str.startswith`, `str.endswith`, `str.contains` | `starts_with`, `ends_with`, `contains` |
| Case in text tests | case-sensitive; `(?i)` in a regex | `case=True` (sensitive) | `case_sensitive: bool = false`. **A deliberate difference**, matching `ns-search/search` |
| Sort direction | `descending: bool = false` | `ascending: bool = True` | `ascending: bool = true` (existing `ns-pl` argument) |
| Nulls in a sort | `nulls_last: bool = false` (nulls first) | `na_position='last'` | `nulls_last: bool = false`, as polars |
| Ties in a sort | `maintain_order: bool = false` | `kind='quicksort'` (not stable) | always stable, so both namespaces give identical order |
| Last rows | `tail(n)` | `tail(n)` | `tail(n = 5)` |
| Drop columns | `drop(columns)` | `drop(columns=…)` | `drop_columns` (existing `ns-pl` name) |
| Row / column count | `height`, `width`, `shape` | `len`, `shape` | `nrows`, `ncols`, `shape` (existing) |

Where polars and pandas differ (sort direction, null placement, case), the `ns-pl` choice is
recorded here so that the pandas namespace can follow it. These differences are restated in
`specs/reference/POLARS_COMMAND_LIBRARY.md` when the design is built (Phase 5).
