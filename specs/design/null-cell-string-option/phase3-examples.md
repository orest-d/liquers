# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | integration (`liquers-lib/tests/record_scalar_reading.rs`, records-gated) | 1×1 view, nullable Text field, `Null` → `Value::from_record_view(..).try_into_string_option() == Ok(None)` |
| T2 | same file | non-null `"x"` → `Ok(Some("x"))` |
| T3 | same file | Int cell `5` → `Ok(Some("5"))` (whatever `try_into_string` gives today for an Int cell; assert equality with it) |
| T4 | regression | Existing numeric option tests |

Build the 1×1 view with `RecordSchema::new(vec![FieldSchema::new("t", FieldType::Text)])`
(nullable per the `FieldSchema` default; check it) and `RecordBatchMut::append_row(&[FieldValue::Null])`.
Names: `null_text_cell_string_option_is_none`, `text_cell_string_option_is_some`.
