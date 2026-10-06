# Phase 2: Solution and Architecture

## liquers-records

- Move `parse_date(text) -> Result<i32, Error>` and `parse_timestamp(text) -> Result<i64, Error>`
  from `formats/csv.rs` into `formats/mod.rs`, or a new `formats/text.rs`, as `pub(crate)`.
  `csv.rs` imports them.
- Add in `column.rs`:

  ```rust
  impl FieldValue {
      /// Parse the text form a table shows for `data_type` (ISO 8601 for Date, RFC 3339 for
      /// Timestamp). `Binary` and `Vector` are refused.
      pub fn parse_text(data_type: FieldType, text: &str) -> Result<FieldValue, Error>
  }
  ```

  It uses an explicit match over every `FieldType` variant. Text, Int, UInt, Float and Bool use
  the same parsing as today's `parse_id_value` arms.

## liquers-lib

`parse_id_value(data_type, id)` becomes:

```rust
match data_type {
    FieldType::Date => FieldValue::parse_text(data_type, id).or_else(|iso| id.parse::<i32>()
        .map(FieldValue::Date).map_err(|_| Error::conversion_error_with_message(id, "Date",
        &format!("expected YYYY-MM-DD (write '-' as '~' in a query) or days since 1970-01-01: {}", iso.message)))),
    FieldType::Timestamp => /* same with i64 microseconds */,
    other => FieldValue::parse_text(other, id),
}
```

The outer match lists every variant explicitly, with no default arm. `Text`/`Int`/… delegate.

## Known-issue preflight

None.

## Relevant commands

`ns-rec/rec_id` (signature unchanged: `id: String`). No registry regeneration.

## Documentation architecture

RECORD_STREAMS.md `rec_id` row and spellings table, with History and `reviewed:`.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-records/src/{column.rs, formats/csv.rs, formats/mod.rs}`; `liquers-lib/src/records/commands.rs` |
| Existing tests | CSV date tests unchanged (moved parser, same behaviour) |
| Compatibility | Additive: raw numbers still accepted |
| Recovery | Revert |
| Certainty | High |
