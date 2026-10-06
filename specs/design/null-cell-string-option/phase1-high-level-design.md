# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** The numeric `_option` conversions were fixed during the record-streams review
  with a `ValueExtension` hook delegated by `CombinedValue`. This is the same pattern for strings,
  and the record-streams Phase 2 table already specifies the expected answer (`Null → None`).
- **Open questions:** None

## Problem

`ValueInterface::try_into_string_option` (`liquers-core/src/value.rs`) defaults to `is_none()`,
then `try_into_string`. `CombinedValue` (`liquers-lib/src/value/extended.rs`) delegates
`try_into_i64_option` / `try_into_f64_option` to the extension, but not the string variant. For an
extended value `is_none()` is `false`, and a single-cell `RecordView` holding `Null` converts
through its scalar delegation to `SimpleValue::None.try_into_string()`, which is the text
`"None"`. A command with an `Option<String>` argument bound to a null cell receives
`Some("None")`.

Filed during `record-streams` (complete, frozen). The issue's `design:` field pointed at that folder; it now points here, because a frozen design cannot own new work.

## Expected behaviour and acceptance

1. A single-cell `RecordView` with a `Null` Text cell: `Value::try_into_string_option() == Ok(None)`.
2. A non-null Text cell `"x"`: `Ok(Some("x"))`.
3. Other extended values (Image, DataFrame, …): unchanged behaviour (they refuse, as for i64).
4. Base values: unchanged.

## Scope

String option only. A `bool` option conversion does not exist on `ValueInterface`, so there is no
gap to close there.

## Design Dependencies

- `record-streams` — **overlaps** (complete). It defined the scalar-reading table.
- `register-command-option-value` — **overlaps**. If `Option<String>` arguments become bindable,
  they go through this conversion. Either order works.

## Documentation assessment

- Reference: `specs/reference/RECORD_STREAMS.md`, scalar reading. State that every `_option`
  conversion gives `None` for a null cell, if it currently lists only the numeric ones.

## Consolidated Findings

- Three edits mirror the i64 pattern exactly: the trait hook with a default, the `ExtValue`
  implementation with an explicit match over every variant (feature-gated arms as in
  `try_into_i64_option`), and the `CombinedValue` delegation.
- A `record_view_cell_string_option` helper next to `record_view_cell_i64_option` maps
  `FieldValue::Null → None` and other values → `Some(text)` using the existing text conversion of
  a cell.
