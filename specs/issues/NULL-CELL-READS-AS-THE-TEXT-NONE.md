---
id: NULL-CELL-READS-AS-THE-TEXT-NONE
kind: issue
title: A null record cell read as optional text gives Some("None") instead of None
status: draft
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
