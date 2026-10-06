# Phase 2: Solution and Architecture

## liquers-records: report type (`formats/mod.rs`)

```rust
/// What a read noticed but did not refuse. Empty for a clean read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReadReport {
    pub warnings: Vec<String>,
}

/// [`read_table`], also returning the warnings the reader collected (padded CSV rows, …).
pub fn read_table_with_report(bytes: &[u8], format: TableFormat, schema: ReadSchema<'_>,
    options: &ReadOptions) -> Result<(RecordBatch, ReadReport), Error>

/// Unchanged signature. Warnings are written to stderr, since there is no log here.
pub fn read_table(...) -> Result<RecordBatch, Error>
```

The non-CSV formats return an empty report. Their `match` arms in `read_table_with_report` are
explicit, as today.

## liquers-records: CSV (`formats/csv.rs`)

```rust
struct ParsedRow { line: usize, fields: Vec<RawField> }   // 1-based start line
fn parse_rows(bytes: &[u8], separator: u8) -> Result<Vec<ParsedRow>, Error>

/// Aggregate of padded rows for the report.
#[derive(Default)]
struct ShortRows { count: usize, min_cells: usize, max_cells: usize }
impl ShortRows {
    fn note(&mut self, cells: usize);
    /// The decided wording; `None` when nothing was padded.
    fn warning(&self, width: usize) -> Option<String>;
}
```

- The width check becomes: `fields.len() > width` → error (line-based message);
  `fields.len() < width` → `short_rows.note(fields.len())`.
- `cell_value(raw: None, field, …)`: nullable → `FieldValue::Null`; non-nullable `Text` →
  `FieldValue::Text("")`; non-nullable other → error naming the line and column (as today, but
  line-based). `cell_value` already handles `None` through `row.get(col)`. Only the non-nullable
  `Text` arm and the message are new.
- `read_inferred` uses the same width check (the header width, or the first row's without a
  header). Inferred fields are nullable, so padding is null.
- Both readers return `ShortRows::warning(width)` in the report.

## liquers-lib (`records/convert.rs`, `to_record`)

Where CSV/TSV bytes are read, call `read_table_with_report` and, for each warning,
`context.warning(&w)?`. Check whether `context` there is `&Context<E>` and whether `warning` is
sync (`Context::warning(&self, &str) -> Result<(), Error>` at `context.rs` ~919).

## Rejected alternatives

- Refusing short rows. Rejected by the decision.
- One warning per padded row. The decision asks for one aggregate warning per read.

## Known-issue preflight

None.

## Relevant commands

`ns-rec/to_record-csv`, `ns-rec/to_record-tsv`, and everything calling `records::convert::to_record`.

## Documentation architecture

RECORD_STREAMS.md CSV notes, with History and `reviewed:`.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-records/src/formats/{mod.rs, csv.rs}`; `liquers-lib/src/records/convert.rs`; tests |
| Existing tests | `"CSV row"` message assertions change to `"CSV line"`. Search all crates. |
| Compatibility | Short rows still read (now with a warning). Non-nullable `Text` gets `""` instead of an error. Messages change. |
| API | Additive (`ReadReport`, `read_table_with_report`) |
| Recovery | Revert |
| Certainty | High |
