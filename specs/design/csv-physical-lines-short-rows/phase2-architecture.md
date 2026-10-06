# Phase 2: Solution and Architecture

## Changes in `liquers-records/src/formats/csv.rs`

```rust
struct ParsedRow {
    /// 1-based physical line on which the record starts.
    line: usize,
    fields: Vec<RawField>,
}

fn parse_rows(bytes: &[u8], separator: u8) -> Result<Vec<ParsedRow>, Error>
```

The parser counts `\n` (and a lone `\r` if it treats that as a line break) while scanning,
including inside quoted cells, and records `line` when a record begins.

```rust
/// Where a row is, for messages: "line 7", or "line 7 (record 5)" when they differ.
fn row_position(row: &ParsedRow, record: usize) -> String

fn check_row_width(row: &ParsedRow, width: usize, record: usize) -> Result<(), Error> {
    if row.fields.len() != width {
        return Err(Error::general_error(format!(
            "read_table: CSV {} has {} fields, but the table has {width} columns",
            row_position(row, record), row.fields.len())));
    }
    Ok(())
}
```

`cell_value(…, line)` receives the position string (or the `ParsedRow` and record number) for its
messages. `read_inferred` calls `check_row_width` too. Today it does not check width at all.
Inspect it before changing, and if it computes width as the max over rows, use the header width
(or the first row's, without a header).

## Errors

`Error::general_error`, as today. No new `ErrorType`.

## Known-issue preflight

None.

## Relevant commands

`ns-rec/to_record-csv`, `ns-rec/file_records` (any CSV read path).

## Documentation architecture

RECORD_STREAMS.md CSV notes, with History and `reviewed:`.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `formats/csv.rs` (+ its tests) |
| Workflows | Reading user CSV files |
| Existing tests | Short-row tests flip. Message-text assertions change (`"CSV row"` → `"CSV line"`). Search `"CSV row"` in all crates' tests. |
| Compatibility | Files with short rows stop reading. That is the decision above. |
| Performance | Negligible (a counter) |
| Recovery | Revert. If the decision is revisited, the option can be added. |
| Certainty | High |
