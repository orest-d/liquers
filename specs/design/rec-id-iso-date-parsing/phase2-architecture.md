# Phase 2: Solution and Architecture

## liquers-records

- Move `parse_date(&str) -> Result<i32, Error>` and `parse_timestamp(&str) -> Result<i64, Error>`
  from `formats/csv.rs` to `formats/mod.rs` (or a new `formats/text.rs`), `pub(crate)`.
  `csv.rs` imports them.
- `column.rs`:

  ```rust
  impl FieldValue {
      /// Parse the text a table shows for `data_type` (ISO 8601 extended date, RFC 3339 timestamp).
      /// `Binary` and `Vector` are refused.
      pub fn parse_text(data_type: FieldType, text: &str) -> Result<FieldValue, Error>
  }
  ```
  with an explicit match over every `FieldType`.

## liquers-lib (`records/commands.rs`)

```rust
/// `YYYYMMDD` → `YYYY-MM-DD`; anything else unchanged.
fn expand_basic_date(id: &str) -> Cow<'_, str>
/// `YYYYMMDDTHHMMSS[.ffffff]Z` → `YYYY-MM-DDTHH:MM:SS[.ffffff]Z`; anything else unchanged.
fn expand_basic_timestamp(id: &str) -> Cow<'_, str>

fn parse_id_value(data_type: FieldType, id: &str) -> Result<FieldValue, Error> {
    match data_type {
        FieldType::Date => FieldValue::parse_text(data_type, &expand_basic_date(id))
            .map_err(|_| Error::conversion_error_with_message(id, "Date",
                "expected YYYYMMDD or YYYY-MM-DD (written YYYY~MM~DD in a query)")),
        FieldType::Timestamp => /* same with expand_basic_timestamp and the two timestamp spellings */,
        FieldType::Text | FieldType::Int | FieldType::UInt | FieldType::Float | FieldType::Bool
        | FieldType::Binary | FieldType::Vector /* list every variant */ =>
            FieldValue::parse_text(data_type, id),
    }
}
```

Check the actual `FieldType` variant list and list them all (no default arm).

## Known-issue preflight

None.

## Relevant commands

`ns-rec/rec_id` (signature unchanged: `id: String`; no registry change).

## Documentation architecture

RECORD_STREAMS.md `rec_id` row and table. QUERY_ESCAPING_GUIDE example row. History rows and
`reviewed:` on both.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-records/src/{column.rs, formats/csv.rs, formats/mod.rs}`; `liquers-lib/src/records/commands.rs` |
| Compatibility | Raw epoch ids stop working, which is decided |
| Recovery | Revert |
| Certainty | High |
