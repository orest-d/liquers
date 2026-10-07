---
id: NULL-CELL-READS-AS-THE-TEXT-NONE
kind: issue
title: A null record cell read as optional text gives Some("None") instead of None
status: closed
priority: P3
complexity: S
area: [lib/value, records]
design: null-cell-string-option
created: 2026-09-27
github:
---
# A null record cell read as optional text gives `Some("None")` instead of `None`

## Problem

Phase 2's scalar-reading table says a single-cell view converts its cell to the base value, and
that `Null → None` — "so the `_option` conversions give None". Since the implementation review,
`try_into_i64_option` and `try_into_f64_option` do. `try_into_string_option` does not:
`ValueExtension` has no hook for it. The default then calls `try_into_string`, and
`SimpleValue::None.try_into_string()` returns the text `"None"` (`liquers-lib/src/value/simple.rs`
~97, `extended.rs`). A command with an `Option<String>` argument bound to a null cell therefore
receives `Some("None")`.

## Expected behaviour

`try_into_string_option` (and any other `_option` conversion) gives `None` for a null cell, through
a hook on `ValueExtension` delegated by `CombinedValue`, as the numeric ones now are.

## Discovery

Found 2026-09-27 while fixing the implementation review's finding C4 (Null → None for the numeric
`_option` conversions).

## Resolution (2026-10-07)

Fixed by design `null-cell-string-option`. `ValueExtension` gained a `try_into_string_option`
hook (default: `try_into_string` wrapped in `Some`), `CombinedValue` delegates it instead of
inheriting the `is_none()` default, and `ExtValue` implements it with the same variant arms as
`try_into_i64_option`: a `RecordView`'s single cell reads through its base value, so `Null` is
`None`, and every other variant refuses as `try_into_string` does.

Evidence: `null_text_cell_string_option_is_none` (fails before the fix with `Some("None")`) and
`text_cell_string_option_is_some` in `liquers-lib/tests/record_scalar_reading.rs`;
`option_hooks_default_to_wrapping_the_scalar_hook` (`liquers-lib/src/value/extended.rs`).
