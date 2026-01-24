# Polars Command Library Specification

## Table of Contents

1. [Overview](#overview)
2. [Query Format](#query-format)
3. [Module Structure](#module-structure)
4. [Command Reference](#command-reference)
   - [Column Selection](#column-selection-1-argument)
   - [Row Slicing](#row-slicing-0-3-arguments)
   - [Comparison Filters](#comparison-filters)
   - [Sorting](#sorting-1-2-arguments-column-direction)
   - [Aggregations](#aggregations---full-dataframe)
   - [Reshaping](#reshaping-varies)
   - [Column Operations](#column-operations-2-3-arguments)
   - [Row/Column Info](#rowcolumn-info-0-arguments)
   - [I/O Operations](#io-operations)
5. [Command Usage Examples](#command-usage-examples)
6. [Implementation Notes](#implementation-notes)
   - [General Principles](#general-principles)
   - [Type-Aware Comparison Value Parsing](#type-aware-comparison-value-parsing)
   - [Edge Cases & Error Handling](#edge-cases--error-handling)
7. [Phase 1 Implementation (MVP)](#phase-1-implementation-mvp)
8. [Recommended Implementation Order](#recommended-implementation-order)
9. [Related Documentation](#related-documentation)

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
-R/data/sales.csv/-/from_csv/select_columns-date-amount-status/gt-amount-1000/eq-status-completed/head-10
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

### Comparison Filters

Filter rows where condition is true. Can be chained for AND logic.

#### Value Comparison (2 arguments: column-value)

| Command | Description | Polars Operator | Example |
|---------|-------------|-----------------|---------|
| `eq` | Equal to | `==` | `eq-status-completed` |
| `ne` | Not equal to | `!=` | `ne-region-west` |
| `gt` | Greater than | `>` | `gt-amount-1000` |
| `gte` | Greater than or equal | `>=` | `gte-age-18` |
| `lt` | Less than | `<` | `lt-price-50` |
| `lte` | Less than or equal | `<=` | `lte-inventory-10` |

#### String Predicates (2 arguments: column-value)

| Command | Description | Polars Operator | Example |
|---------|-------------|-----------------|---------|
| `contains` | String contains substring (case-sensitive) | `str.contains()` | `contains-name-john` |
| `startswith` | String starts with prefix (case-sensitive) | `str.starts_with()` | `startswith-code-us` |
| `endswith` | String ends with suffix (case-sensitive) | `str.ends_with()` | `endswith-file-csv` |

#### Null Checks (1 argument: column)

| Command | Description | Polars Operator | Example |
|---------|-------------|-----------------|---------|
| `isnull` | Filter rows where column is null | `is_null()` | `isnull-email` |
| `notnull` | Filter rows where column is not null | `is_not_null()` | `notnull-phone` |

#### Min/Max Row Filters (1 argument: column)

| Command | Description | Implementation | Example |
|---------|-------------|----------------|---------|
| `min_rows` | Return all rows where column equals its minimum value | `df.filter(col(column).eq(col(column).min()))` | `min_rows-price` |
| `max_rows` | Return all rows where column equals its maximum value | `df.filter(col(column).eq(col(column).max()))` | `max_rows-price` |

**Notes:**
- `min_rows`/`max_rows` return **all rows** with the min/max value (not just one)
- Works for numeric, string (lexicographic), and date/datetime columns
- If multiple rows have the same min/max, all are returned

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
| `count` | Count non-null values per column | `df.count()` |
| `describe` | Statistical summary | `df.describe()` |

**Example**: `sum` returns single-row DataFrame with column sums

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
| `from_csv` | `[separator]` | Read CSV from key (assumes header row) | `CsvReader::from_path()` | Default separator: comma. Options: "comma", "tab", "semicolon", "pipe", or single char |
| `to_csv` | *(none)* | Write DataFrame as CSV string | `df.write_csv()` | Returns string with comma separator |
| `from_parquet` | *(none)* | Read Parquet from key | `ParquetReader::new()` | State provides path |

**Examples**:
- `-R/data/sales.csv/-/from_csv` (default comma separator)
- `-R/data/sales.tsv/-/from_csv-tab` (tab-separated)
- `-R/data/sales.txt/-/from_csv-semicolon` (semicolon-separated)
- `-R/data/sales.txt/-/from_csv-pipe` (pipe-separated)

**Note on `from_csv` vs automatic detection**:

With the `try_to_polars_dataframe` utility, many operations can work directly without explicit `from_csv`:

- **Explicit `from_csv` needed when**:
  - Custom separator required (tab, semicolon, pipe, etc.)
  - Need to override metadata-detected format
  - Starting a new DataFrame pipeline

- **Automatic detection works when**:
  - File has `.csv` extension (metadata indicates CSV format)
  - Using standard comma separator
  - Chaining operations after an explicit load

**Examples**:
```
# Explicit - required for custom separator
-R/data/sales.tsv/-/from_csv-tab/head-10

# Automatic - works if sales.csv has proper extension
-R/data/sales.csv/-/head-10

# Mixed - explicit first, then automatic
-R/data/sales.csv/-/from_csv/select_columns-date-amount/head-10
```

---

## Command Usage Examples

### Example 1: Selection and Filtering

Filter sales where amount > 1000 and status is completed, select specific columns:

```
-R/data/sales.csv/-/from_csv/select_columns-date-amount-status/gt-amount-1000/eq-status-completed/head-10
```

---

## Implementation Notes

### Overview

All Polars commands follow a consistent pattern:
1. **Extract DataFrame** from state using `try_to_polars_dataframe(state)?`
2. **Perform operation** using Polars methods
3. **Return result** wrapped in `Value::from_polars_dataframe(df)`

The `try_to_polars_dataframe` utility function is the foundation of the entire command library, providing automatic DataFrame extraction from multiple sources.

### General Principles

1. **State Parameter**: Commands receive `state: &State<V>`
   - Extract DataFrame: Use `try_to_polars_dataframe(&state)?` utility function
   - Return DataFrame: `V::from_polars_dataframe(df)`
   - Handle Arc wrapper transparently

2. **DataFrame Extraction Utility** (`util.rs`):

   All polars commands should use the `try_to_polars_dataframe` utility function to convert State into a DataFrame. This function:

   **Signature**:
   ```rust
   pub fn try_to_polars_dataframe<V: ValueInterface + ExtValueInterface>(
       state: &State<V>
   ) -> Result<Arc<polars::frame::DataFrame>, Error>
   ```

   **Logic**:
   1. **Direct conversion**: First try `state.data.as_polars_dataframe()`
      - If successful, return the DataFrame immediately
      - This handles the case where state already contains a PolarsDataFrame

   2. **Deserialization from binary/text**:
      - If direct conversion fails, check if value is Text or Bytes
      - Inspect `state.metadata.get_data_format()` to determine format
      - Attempt deserialization based on format:

   **Supported formats**:
   - `"csv"`: Parse as CSV using `polars::io::CsvReader::new()`
     - For Text: parse string directly
     - For Bytes: wrap in cursor and parse
   - `"parquet"`: Parse as Parquet using `polars::io::ParquetReader`
     - Requires Bytes value
   - `"json"` (future): Parse as JSON
   - Other formats: Return error with helpful message

   **Error handling**:
   - If value is not DataFrame, Text, or Bytes: `"Cannot convert {type} to DataFrame"`
   - If format is unknown: `"Unsupported data format '{format}' for DataFrame conversion. Supported: csv, parquet"`
   - If deserialization fails: `"Failed to parse {format} as DataFrame: {error}"`

   **Example implementation skeleton**:
   ```rust
   pub fn try_to_polars_dataframe<V: ValueInterface + ExtValueInterface>(
       state: &State<V>
   ) -> Result<Arc<polars::frame::DataFrame>, Error> {
       // Try direct conversion first
       if let Ok(df) = state.data.as_polars_dataframe() {
           return Ok(df);
       }

       // Get data format from metadata
       let format = state.metadata.get_data_format();

       // Try deserialization based on value type and format
       match format.as_str() {
           "csv" => {
               // Try to get as text or bytes
               if let Ok(text) = state.data.try_into_string() {
                   let df = CsvReader::new(Cursor::new(text.as_bytes()))
                       .has_header(true)
                       .finish()
                       .map_err(|e| Error::general_error(
                           format!("Failed to parse CSV as DataFrame: {}", e)
                       ))?;
                   return Ok(Arc::new(df));
               }
               // Handle bytes similarly...
           },
           "parquet" => {
               // Get bytes and parse as parquet
               // ...
           },
           _ => {
               return Err(Error::general_error(
                   format!("Unsupported data format '{}' for DataFrame conversion. Supported: csv, parquet", format)
               ));
           }
       }

       Err(Error::conversion_error(
           state.data.identifier().as_ref(),
           "Polars DataFrame"
       ))
   }
   ```

   **Usage in commands**:

   All polars commands should follow this pattern:

   ```rust
   use crate::polars::util::try_to_polars_dataframe;
   use liquers_core::{state::State, error::Error};

   // Example 1: Simple command (head)
   fn head(state: &State<Value>, n: i32) -> Result<Value, Error> {
       let df = try_to_polars_dataframe(state)?;
       let result = df.head(Some(n as usize));
       Ok(Value::from_polars_dataframe(result))
   }

   // Example 2: Command with error handling (select_columns)
   fn select_columns(state: &State<Value>, columns: String) -> Result<Value, Error> {
       let df = try_to_polars_dataframe(state)?;

       let col_names: Vec<&str> = columns.split('-').map(|s| s.trim()).collect();

       // Check all columns exist
       for col in &col_names {
           if !df.get_column_names().contains(col) {
               return Err(Error::general_error(format!(
                   "Column '{}' not found in DataFrame. Available columns: {:?}",
                   col, df.get_column_names()
               )));
           }
       }

       let result = df.select(col_names)
           .map_err(|e| Error::general_error(format!("Failed to select columns: {}", e)))?;

       Ok(Value::from_polars_dataframe(result))
   }

   // Example 3: Comparison filter with type-aware parsing (gt)
   fn gt(state: &State<Value>, column: String, value_str: String) -> Result<Value, Error> {
       let df = try_to_polars_dataframe(state)?;

       // Verify column exists
       let schema = df.schema();
       let dtype = schema.get(&column)
           .ok_or_else(|| Error::general_error(format!(
               "Column '{}' not found in DataFrame", column
           )))?;

       // Parse value based on column type
       let filter_expr = match dtype {
           DataType::Int64 => {
               let val = value_str.trim().parse::<i64>()
                   .map_err(|_| Error::general_error(format!(
                       "Cannot parse '{}' as Int64 for column '{}'", value_str, column
                   )))?;
               col(&column).gt(lit(val))
           },
           DataType::Float64 => {
               let val = value_str.trim().parse::<f64>()
                   .map_err(|_| Error::general_error(format!(
                       "Cannot parse '{}' as Float64 for column '{}'", value_str, column
                   )))?;
               col(&column).gt(lit(val))
           },
           _ => {
               return Err(Error::general_error(format!(
                   "Comparison 'gt' not supported for column '{}' of type {:?}",
                   column, dtype
               )));
           }
       };

       let result = df.filter(&filter_expr)
           .map_err(|e| Error::general_error(format!("Filter failed: {}", e)))?;

       Ok(Value::from_polars_dataframe(result))
   }
   ```

   **Key benefits of `try_to_polars_dataframe`**:
   - Commands work transparently with DataFrames already in memory
   - Commands can also load DataFrames from CSV/Parquet in binary/text state
   - Single extraction point for all DataFrame operations
   - Consistent error messages
   - Enables chaining: `-R/data.csv/-/from_csv/head-10` is equivalent to `-R/data.csv/-/head-10` if metadata indicates CSV format

3. **Parameter Parsing**:
   - Parsing of arguments is done by the framework.   
   - Column names: split by "-", trim whitespace
   - Single vs. multiple: e.g., `select_columns-col1-col2-col3`
   - Optional arguments: provide defaults (e.g., `head` defaults to 5)
   - Default values can be specified with the `register_command!` DSL.
   - Numeric arguments parsed as i32, u64, f64 as needed

4. **Seeding for Random Operations**:
   - Commands `sample` and `shuffle` accept optional seed parameter (u64)
   - Polars supports `seed: Option<u64>` in `sample_n()`, `sample_frac()`, etc.
   - When seed is provided, operations are reproducible across runs
   - Without seed, operations are non-deterministic (depends on system randomness)
   - Example: `sample-10-42` uses seed 42, `shuffle-12345` uses seed 12345
   - Seed must be valid u64; invalid seeds should return error with clear message

5. **Error Handling**:
   - Use `Error::general_error()` or `Error::execution_error()` for operation failures
   - The interpreter will automatically add command context to all errors
   - Provide clear, specific error messages: "Column 'amount' not found in DataFrame"
   - Never unwrap Polars operations - always use `?` or `map_err()`

6. **Type Conversion**:
   - Data types as strings: `"int"`, `"float"`, `"string"`, `"date"`, `"datetime"`
   - Map to Polars `DataType` enum

### Type-Aware Comparison Value Parsing

Comparison commands (`eq`, `ne`, `gt`, `gte`, `lt`, `lte`) automatically convert the string argument to match the column's data type. The implementation should:

1. **Inspect the column's `dtype`** from the DataFrame schema
2. **Parse the comparison value** according to the detected type
3. **Return clear errors** if parsing fails

#### Numeric Columns (Int8, Int16, Int32, Int64, UInt8, UInt16, UInt32, UInt64, Float32, Float64)

- Parse argument as the matching numeric type
- **Example**: `gt-amount-1000` parses "1000" as i64 if `amount` is Int64
- **Error if parsing fails**: `"Cannot parse '1000x' as Int64 for column 'amount'"`
- **Whitespace**: Trim before parsing

```rust
// Implementation approach
let column_dtype = df.schema().get(&column_name)?;
match column_dtype {
    DataType::Int64 => {
        let value = arg.trim().parse::<i64>()
            .map_err(|_| Error::general_error(format!("Cannot parse '{}' as Int64 for column '{}'", arg, column_name)))?;
        // Create filter expression
    }
    // ... other numeric types
}
```

#### Date Columns

Support three formats (tried in order until one succeeds):

1. **`YYYYMMDD`** (compact, no separators): `"20240115"`
2. **`YYYY-MM-DD`** (ISO 8601 standard): `"2024-01-15"`
3. **`YYYY_MM_DD`** (underscore separator): `"2024_01_15"`

**Examples**:
- `gt-order_date-20240115`
- `gte-order_date-2024-01-15`
- `lt-delivery_date-2024_12_31`

**Error if none match**: `"Cannot parse '2024/01/15' as Date (use YYYYMMDD, YYYY-MM-DD, or YYYY_MM_DD)"`

```rust
// Implementation approach
fn parse_date(s: &str) -> Result<NaiveDate, Error> {
    // Try YYYYMMDD
    if s.len() == 8 && s.chars().all(|c| c.is_ascii_digit()) {
        return NaiveDate::parse_from_str(s, "%Y%m%d")
            .map_err(|_| Error::general_error(format!("Invalid date format: {}", s)));
    }
    // Try YYYY-MM-DD
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(date);
    }
    // Try YYYY_MM_DD
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y_%m_%d") {
        return Ok(date);
    }
    Err(Error::general_error(format!(
        "Cannot parse '{}' as Date (use YYYYMMDD, YYYY-MM-DD, or YYYY_MM_DD)", s
    )))
}
```

#### DateTime Columns

Support ISO 8601 and compact formats:

1. **`YYYY-MM-DD HH:MM:SS`** (ISO with space): `"2024-01-15 14:30:00"`
2. **`YYYY-MM-DDTHH:MM:SS`** (ISO with T separator): `"2024-01-15T14:30:00"`
3. **`YYYYMMDDHHMMSS`** (compact): `"20240115143000"`

**Examples**:
- `gt-timestamp-2024-01-15T14:30:00`
- `gte-created_at-20240115143000`

**Optional microseconds**: Support `.SSS` or `.SSSSSS` suffix
- `"2024-01-15 14:30:00.123"`
- `"2024-01-15T14:30:00.123456"`

**Error**: `"Cannot parse '2024/01/15 14:30' as DateTime (use ISO 8601 or YYYYMMDDHHMMSS)"`

#### String Columns

- **Use argument as-is** (no conversion)
- **Case-sensitive** comparison
- **Whitespace preserved** (no trimming for string values)
- **Example**: `eq-status-completed` matches "completed" but not "Completed" or " completed"

**String predicates** (`contains`, `startswith`, `endswith`):
- Also case-sensitive
- `contains-name-john` matches "john" in "johnny" but not "John"

#### Boolean Columns

Accept multiple representations (case-insensitive):

| Input | Parsed As |
|-------|-----------|
| `true`, `t`, `1` | `true` |
| `false`, `f`, `0` | `false` |

**Examples**:
- `eq-active-true`
- `eq-active-t`
- `eq-verified-1`
- `ne-deleted-false`

**Error**: `"Cannot parse 'yes' as Boolean (use true/false, t/f, or 1/0)"`

#### Null Handling in Comparisons

- **Nulls are filtered out** by comparison operators (standard SQL behavior)
- `gt-amount-1000` on a column with nulls: nulls never match the condition
- To explicitly check for nulls, use `isnull-column` or `notnull-column`
- **Recommended pattern** for data quality: `notnull-amount/gt-amount-1000`

### Edge Cases & Error Handling

#### Column Name Validation

**Missing column**:
- Error: `"Column 'foo' not found in DataFrame. Available columns: [a, b, c]"`
- Check exists before operations: `df.schema().contains_key(&column_name)`

**Empty column name**:
- Error: `"Column name cannot be empty"`

#### Empty DataFrame Handling

**Slicing operations** (`head`, `tail`, `slice`):
- Return empty DataFrame (no error)
- `head-10` on 0-row DataFrame returns 0 rows

**Aggregations** (`sum`, `mean`, `min`, `max`):
- Return single-row DataFrame with null/NaN values (Polars default)

**Min/Max row filters** (`min_rows`, `max_rows`):
- Return empty DataFrame if input is empty
- No error

**Filters** (`eq`, `gt`, etc.):
- Return empty DataFrame (no rows match)

#### Type Mismatch Errors

**String comparison on numeric column**:
- `eq-amount-thousand` where `amount` is Int64
- Error: `"Cannot parse 'thousand' as Int64 for column 'amount'"`

**Numeric comparison on string column**:
- `gt-name-100` where `name` is String
- Error: `"Cannot compare String column 'name' with numeric operator 'gt'. Use string predicates like 'contains' or cast column first."`
- **Alternative**: Allow lexicographic comparison (implementation choice)

**Date comparison on non-date column**:
- Auto-detect based on parsing success vs. column dtype
- If column is String and value looks like date, error with helpful message

#### String Comparison Behavior

**Case sensitivity**:
- All string comparisons are **case-sensitive** by default
- `eq-status-completed` does NOT match "Completed"
- Future enhancement: add case-insensitive variants (`eq_i`, `contains_i`)

**Whitespace handling**:
- String values are **NOT trimmed**
- `eq-status-completed` does NOT match " completed " (with spaces)
- Numeric parsing DOES trim: `gt-amount- 100 ` (parses as 100)

**Partial matches**:
- `eq`, `ne`: exact match only
- `contains`: substring match
- `startswith`, `endswith`: prefix/suffix match

#### Numeric Edge Cases

**Integer overflow**:
- `gt-id-99999999999999999999999999` (exceeds i64::MAX)
- Error: `"Cannot parse '99999999999999999999999999' as Int64: number too large"`

**Float precision**:
- `eq-price-1.234567890123456789`
- Parse as f64 (limited precision)
- Note: Exact equality on floats is fragile; prefer `gte`/`lte` range checks

**Negative numbers**:
- `lt-temperature--10` (double dash for negative)
- **Problem**: Conflicts with separator syntax
- **Solution**: Use underscores or parentheses (out of scope for MVP)
- **Workaround**: Use `lte-temperature--11` or data preprocessing

#### Separator Argument Parsing

**`from_csv` separator options**:
- Recognized keywords: `"comma"`, `"tab"`, `"semicolon"`, `"pipe"`
- Single character: any single char (e.g., `from_csv-|` for pipe)
- Error for invalid: `"Invalid separator 'xyz'. Use 'comma', 'tab', 'semicolon', 'pipe', or single character."`

#### Error Message Format

All errors should follow this pattern:

```
"<Operation> failed: <reason>. <suggestion>"
```

**Examples**:
- `"Filter 'gt' failed: Cannot parse 'abc' as Int64 for column 'amount'. Check column type and value format."`
- `"Column selection failed: Column 'foo' not found. Available columns: [bar, baz]."`
- `"CSV read failed: File not found at '/data/sales.csv'. Check resource path."`

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
- `select_columns`, `drop_columns`, `head`, `tail`, `slice`

**Filtering** (6):
- `eq`, `ne`, `gt`, `gte`, `lt`, `lte`

**Sorting** (1):
- `sort`

**Aggregations** (6):
- `sum`, `mean`, `min`, `max`, `count`, `describe`

**Info** (2):
- `shape`, `nrows`

**I/O** (2):
- `from_csv`, `to_csv`

**Total MVP**: ~22 commands

**Note**: Additional filter variants (`isnull`, `notnull`, `min_rows`, `max_rows`, string predicates) and reshaping operations (`transpose`, `unique`) are Phase 2.

---

## Phase 2 (Future)

- Advanced filters: `contains`, `startswith`, `isnull`, `notnull`
- Reshaping: `explode`, `transpose`, `unique`
- Additional I/O: JSON, Parquet
- Advanced aggregations: `median`, `std`, `describe`

---

## Recommended Implementation Order

To minimize dependencies and enable incremental testing:

### Step 1: Foundation & Utilities (1-2 days)
1. **Module setup**: Create `liquers-lib/src/polars/mod.rs` and submodules
2. **Core utility function** (`util.rs`):
   - **`try_to_polars_dataframe<V>(state: &State<V>) -> Result<Arc<DataFrame>, Error>`** - The primary utility that all commands use to extract DataFrames from State
     - Tries direct conversion via `as_polars_dataframe()` first
     - Falls back to deserialization from Text/Bytes based on metadata `data_format`
     - Supports CSV and Parquet formats
3. **Parsing utilities** (`util.rs`):
   - `parse_date(s: &str) -> Result<NaiveDate>`
   - `parse_datetime(s: &str) -> Result<NaiveDateTime>`
   - `parse_boolean(s: &str) -> Result<bool>`
   - `parse_separator(s: &str) -> Result<u8>` (for CSV)
   - `column_exists_check(df, column_name)` helper
4. **Type detection helper**: Function to inspect column dtype and parse comparison values
5. **Test utilities**: Sample DataFrame generators for testing

### Step 2: I/O Operations (1 day)
5. **`from_csv`** - Essential for loading test data
6. **`to_csv`** - Enables output inspection
7. **Test**: Load CSV, verify DataFrame structure

### Step 3: Selection & Slicing (1 day)
8. **`select_columns`** - Core data selection
9. **`head`**, **`tail`** - Basic slicing (simple, no type parsing)
10. **`slice`** - Range-based slicing
11. **Test**: Chain operations: `from_csv/select_columns-col1-col2/head-10`

### Step 4: Info Commands (0.5 days)
12. **`shape`**, **`nrows`**, **`ncols`** - Metadata queries
13. **Test**: Verify output format (Value conversion)

### Step 5: Comparison Filters - Numeric (1-2 days)
14. Implement type-aware value parsing for **numeric types only**
15. **`eq`**, **`ne`**, **`gt`**, **`gte`**, **`lt`**, **`lte`** for Int/Float columns
16. **Test**: Filter numeric columns with various operators
17. **Test edge cases**: Parse errors, missing columns, null handling

### Step 6: Comparison Filters - Dates (1 day)
18. Extend type-aware parsing to **Date columns**
19. Test all three date formats (YYYYMMDD, YYYY-MM-DD, YYYY_MM_DD)
20. **Test**: Filter date columns, verify format fallbacks

### Step 7: Comparison Filters - Strings & Booleans (1 day)
21. Extend to **String** and **Boolean** columns
22. Implement **string predicates**: `contains`, `startswith`, `endswith`
23. **Test**: Case-sensitive matching, whitespace handling

### Step 8: Sorting & Aggregations (1 day)
24. **`sort`** - Single column sorting with direction flag
25. **Aggregations**: `sum`, `mean`, `min`, `max`, `count`, `describe`
26. **Test**: Sort by different column types, verify aggregation outputs

### Step 9: Null Checks & Min/Max Rows (0.5 days)
27. **`isnull`**, **`notnull`** - Simple null filtering
28. **`min_rows`**, **`max_rows`** - Find rows with extreme values
29. **Test**: Multiple rows with same min/max

### Step 10: Integration Testing & Refinement (1 day)
30. **Complex query chains**: Combine filters, sorts, aggregations
31. **Error message refinement**: Ensure all errors are clear and helpful
32. **Documentation examples**: Verify all examples in spec work
33. **Edge case coverage**: Empty DataFrames, all-null columns, etc.

### Total Estimated Effort: 8-10 days for MVP

**Dependencies for implementation**:
- `polars` crate with features: `csv-file`, `parquet`, `dtype-date`, `dtype-datetime`
- `chrono` for date/datetime parsing (or use Polars' native parsing)
- Existing Liquers infrastructure: `State`, `Value`, `Error`, `register_command!`

---

## Related Documentation

- [Polars DataFrame Docs](https://docs.rs/polars/latest/polars/frame/struct.DataFrame.html)
- [Polars Prelude](https://docs.rs/polars/latest/polars/prelude/index.html)
- `specs/COMMAND_REGISTRATION_GUIDE.md` - How to register these commands
- `CLAUDE.md` - Architecture and module organization
- `PROJECT_OVERVIEW.md` - Liquers query language design
