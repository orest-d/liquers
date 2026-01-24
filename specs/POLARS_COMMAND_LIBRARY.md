# Polars Command Library Specification

## Overview

This specification defines a command library wrapping essential Polars DataFrame functionality for Liquers. The library enables users to perform efficient data transformations using Polars' vectorized operations and parallel evaluation.

Commands are registered in the **`pl` namespace** and use **verbose, mnemonic names** with arguments **separated by "-"** to avoid special characters.

**Module**: `liquers-lib::polars`
**Namespace**: `pl` (default realm)
**Value Type**: `ExtValue::PolarsDataFrame` (Arc-wrapped `polars::frame::DataFrame`)

## Query Format

Queries use the format:
```
-R/data/file.csv/-/<command>-<arg1>-<arg2>/<command>-<arg1>...
```

Where:
- `-R/data/file.csv` is the resource (file path)
- `/-/` separates the resource from operations
- Operations are chained with `/` (not `/-/`)
- Arguments within a command are separated by `-`
- Namespace is selected with `ns-pl` command before first Polars operation (or default if in pl namespace)

Example:
```
-R/data/sales.csv/-/from_csv-t/select-date-amount-status/gt-amount-1000/eq-status-completed/head-10
```

## Module Structure

```
liquers-lib/src/polars/
├── mod.rs              # Module declaration and registration
├── selection.rs        # Column and row selection
├── filtering.rs        # Row filtering and comparisons
├── sorting.rs          # Sorting and ranking
├── aggregation.rs      # Aggregations and grouping
├── transformation.rs   # Column transforms, reshaping
├── io.rs               # Input/output operations
└── util.rs             # Helper functions
```

## Command Reference

### Column Selection (1 argument)

| Command | Arguments | Description | Polars Method |
|---------|-----------|-------------|---------------|
| `select_columns` | `col1-col2-col3` | Select columns by name | `df.select()` |
| `drop_columns` | `col1-col2-col3` | Drop columns by name | `df.drop()` |

**Example**: `select_columns-date-amount-region`

### Row Slicing (0-3 arguments)

| Command | Arguments | Description | Polars Method |
|---------|-----------|-------------|---------------|
| `head` | `[n]` | Get first N rows (default 5) | `df.head()` |
| `tail` | `[n]` | Get last N rows (default 5) | `df.tail()` |
| `slice` | `offset-length` | Extract rows by range | `df.slice()` |
| `sample` | `[n-[seed]]` | Random sample of N rows (default 5); optional seed (u64) for reproducibility | `df.sample_n(seed=...)` |
| `shuffle` | `[[seed]]` | Randomly shuffle rows; optional seed (u64) for reproducibility | `df.sample_frac(1.0, true, seed=...)` |

**Example**: `head-10`, `slice-10-5`, `sample-10`, `sample-10-42` (with seed 42), `shuffle`, `shuffle-42` (with seed 42)

### Comparison Filters (2 arguments: column-value)

Filter rows where condition is true. Can be chained for AND logic.

| Command | Description | Polars Operator | Example |
|---------|-------------|-----------------|---------|
| `eq` | Equal to | `==` | `eq-status-completed` |
| `ne` | Not equal to | `!=` | `ne-region-west` |
| `gt` | Greater than | `>` | `gt-amount-1000` |
| `gte` | Greater than or equal | `>=` | `gte-age-18` |
| `lt` | Less than | `<` | `lt-price-50` |
| `lte` | Less than or equal | `<=` | `lte-inventory-10` |
| `contains` | String contains substring | `str.contains()` | `contains-name-john` |
| `startswith` | String starts with prefix | `str.starts_with()` | `startswith-code-us` |
| `endswith` | String ends with suffix | `str.ends_with()` | `endswith-file-csv` |
| `isnull` | Is null | `is_null()` | `isnull-email` |
| `notnull` | Is not null | `is_not_null()` | `notnull-phone` |
| `min_rows` | Rows where column has minimum value (numeric or string) |  | `min_rows-price` |
| `max_rows` | Rows where column has maximum value (numeric or string) |  | `max_rows-price` |

**Example chain**: `-R/sales.csv/-/gt-amount-1000/eq-status-completed`

### Sorting (1-2 arguments: column [direction])

| Command | Arguments | Description | Polars Method |
|---------|-----------|-------------|---------------|
| `sort` | `column-[ascending_flag]` | Sort by column (ascending by default) | `df.sort()` |

**Example**: `sort-date` or `sort-date-f` 

### Aggregations - Full DataFrame

Apply to entire DataFrame (no arguments):

| Command | Description | Polars Method |
|---------|-------------|---------------|
| `sum` | Sum all numeric columns | `df.sum()` |
| `mean` | Mean of all numeric columns | `df.mean()` |
| `median` | Median of all numeric columns | `df.median()` |
| `min` | Minimum of all numeric columns | `df.min()` |
| `max` | Maximum of all numeric columns | `df.max()` |
| `std` | Standard deviation of numeric columns | `df.std()` |
| `describe` | Statistical summary | `df.describe()` |

**Example**: `sum` returns DataFrame with row sums

### Grouping & Aggregation

Out of scope. 

### Reshaping (varies)

| Command | Arguments | Description | Polars Method |
|---------|-----------|-------------|---------------|
| `explode` | `col1-col2` | Expand list columns to rows | `df.explode()` |
| `transpose` | *(none)* | Flip rows to columns | `df.transpose()` |
| `unique` | `[col1-col2]` | Unique rows or by column(s) | `df.unique()` |

**Example**: `transpose`

### Column Operations (2-3 arguments)

| Command | Arguments | Description | Polars Method |
|---------|-----------|-------------|---------------|
| `rename` | `old-new` | Rename column | `df.rename()` |
| `cast` | `column-type` | Cast to type (int, float, string, etc.) | `df.with_column(col().cast())` |
| `fill_null` | `column-value` | Fill nulls with value | `df.fill_null()` |
| `drop_nulls` | `[col1-col2]` | Drop rows with nulls | `df.drop_nulls()` |

**Example**: `cast-age-int` or `fill_null-salary-0`

### Row/Column Info (0 arguments)

| Command | Description | Polars Method |
|---------|-------------|---------------|
| `shape` | Get row and column count | `df.shape()` |
| `schema` | Get column names and types | `df.schema()` |
| `nrows` | Get row count | `df.height()` |
| `ncols` | Get column count | `df.width()` |

**Example**: `shape` returns dictionary with nrows and ncols.

### String Operations (1-3 arguments)

Out of scope.

### I/O Operations

| Command | Arguments | Description | Polars Method | Notes |
|---------|-----------|-------------|---------------|-------|
| `from_csv` | separator | Read CSV from key (must have a header)  | `CsvReader::from_path()` | State provides path |
| `to_csv` | *(none)* | Write DataFrame as CSV string | `df.write_csv()` | Returns string or bytes |
| `from_parquet` | *(none)* | Read Parquet from key | `ParquetReader::new()` | State provides path |

**Example**: `-R/data/sales_csv.txt/-/from_csv` (path from resource)

---

## Command Usage Examples

### Example 1: Selection and Filtering

Filter sales where amount > 1000 and status is completed, select specific columns:

```
-R/data/sales.csv/-/from_csv/select_columns-date-amount-status/gt-amount-1000/eq-status-completed/head-10
```

---

## Implementation Notes

### General Principles

1. **State Parameter**: Commands receive `state: &State<V>`
   - Extract DataFrame: `state.as_polars_dataframe()?`
   - Return DataFrame: `V::from_polars_dataframe(df)`
   - Handle Arc wrapper transparently

2. **Parameter Parsing**:
   - Column names: split by "-", trim whitespace
   - Single vs. multiple: e.g., `select_columns-col1-col2-col3`
   - Optional arguments: provide defaults (e.g., `head` defaults to 5)
   - Default values can be specified with the `register_command!` DSL.
   - Numeric arguments parsed as i32, u64, f64 as needed

3. **Seeding for Random Operations**:
   - Commands `sample` and `shuffle` accept optional seed parameter (u64)
   - Polars supports `seed: Option<u64>` in `sample_n()`, `sample_frac()`, etc.
   - When seed is provided, operations are reproducible across runs
   - Without seed, operations are non-deterministic (depends on system randomness)
   - Example: `sample-10-42` uses seed 42, `shuffle-12345` uses seed 12345
   - Seed must be valid u64; invalid seeds should return error with clear message

4. **Error Handling**:
   - Use `Error::general_error()` for operation failures
   - Provide context: "Failed to filter: column 'amount' not found"
   - Never unwrap Polars operations

5. **Type Conversion**:
   - Data types as strings: `"int"`, `"float"`, `"string"`, `"date"`, `"datetime"`
   - Map to Polars `DataType` enum

### Chaining & Composition

Commands compose naturally via Liquers' path syntax:

```
input/-/gt-col1-100/eq-col2-active/select_columns-col3-col4/sum
```

Each command:
1. Receives state (DataFrame)
2. Applies transformation
3. Returns modified DataFrame to next command

### Async Considerations

- Core operations are **synchronous** (Polars handles parallelization)
- I/O commands (`from_csv`, `from_parquet`, etc.) could be async for large files
- For now, keep all as sync for simplicity

---

## Phase 1 Implementation (MVP)

Essential commands covering 80% of use cases:

**Selection & Slicing** (5):
- `select_columns`, `drop`, `head`, `tail`, `slice`

**Filtering** (6):
- `eq`, `ne`, `gt`, `gte`, `lt`, `lte`

**Sorting** (2):
- `sort`

**Aggregations** (5):
- `sum`, `mean`, `min`, `max`, `count`

**I/O** (2):
- `from_csv`, `to_csv`

**Total MVP**: ~22 commands

---

## Phase 2 (Future)

- Advanced filters: `contains`, `startswith`, `isnull`, `notnull`
- Reshaping: `explode`, `transpose`, `unique`
- Additional I/O: JSON, Parquet
- Advanced aggregations: `median`, `std`, `describe`

---

## Related Documentation

- [Polars DataFrame Docs](https://docs.rs/polars/latest/polars/frame/struct.DataFrame.html)
- [Polars Prelude](https://docs.rs/polars/latest/polars/prelude/index.html)
- `specs/COMMAND_REGISTRATION_GUIDE.md` - How to register these commands
- `CLAUDE.md` - Architecture and module organization
- `PROJECT_OVERVIEW.md` - Liquers query language design
