# Phase 2: Solution and Architecture

## Changes

1. `liquers-lib/src/value/extended.rs`, `trait ValueExtension`:

   ```rust
   /// Try to get a `String`, allowing an extension to report "no value". See
   /// [`ValueExtension::try_into_i64_option`].
   fn try_into_string_option(&self) -> Result<Option<String>, Error> {
       self.try_into_string().map(Some)
   }
   ```

   Check that `ValueExtension` has `try_into_string`. If the scalar delegation is named
   differently, mirror it.
2. `impl ValueInterface for CombinedValue`: add delegation:

   ```rust
   fn try_into_string_option(&self) -> Result<Option<String>, Error> {
       match self {
           CombinedValue::Base(base) => base.try_into_string_option(),
           CombinedValue::Extended(ext) => ext.try_into_string_option(),
       }
   }
   ```
3. `liquers-lib/src/value/mod.rs`, `impl ValueExtension for ExtValue`: `try_into_string_option`
   with the same variant arms as `try_into_i64_option`, where `RecordView` uses
   `record_view_cell_string_option(&value.single_cell()?)`. Image, UIElement and Foreign keep
   whatever `try_into_string` does today, because their string conversion may legitimately succeed
   (for example a description). Do not change them to refusals: call `self.try_into_string().map(Some)`.
4. Helper `record_view_cell_string_option(cell: &FieldValue) -> Result<Option<String>, Error>` next
   to the i64 helper (`records`-gated).

## Known-issue preflight

None blocking.

## Relevant commands

Any command with an `Option<String>` argument or state conversion over `ns-rec` single-cell views
(e.g. `-R/data/t.csv/-/ns-rec/to_record-csv/ns-rec/rec_id-…` followed by a text command).

## Documentation architecture

RECORD_STREAMS.md sentence, with History and `reviewed:`.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `value/extended.rs`, `value/mod.rs`, `tests/record_scalar_reading.rs` |
| Feature gates | The `records` arm. Run the build matrix. |
| Existing tests | `record_scalar_reading.rs` numeric cases unchanged |
| Compatibility | `Some("None")` becomes `None`, which is a bug fix |
| Recovery | Revert |
| Certainty | High |
