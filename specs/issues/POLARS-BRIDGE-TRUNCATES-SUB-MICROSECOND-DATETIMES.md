---
id: POLARS-BRIDGE-TRUNCATES-SUB-MICROSECOND-DATETIMES
kind: issue
title: The polars bridge silently drops sub-microsecond precision from a nanosecond Datetime column
status: closed
priority: P3
complexity: S
area: [records, lib/polars]
design: 
created: 2026-11-01
github:
---
## Problem

`dataframe_to_record_batch` (`liquers-lib/src/records/polars.rs`, the `FieldType::Timestamp` arm,
around line 215) casts every `Datetime` series to `Datetime(Microseconds, …)` before reading it. A
`Datetime(Nanoseconds)` value with a non-zero sub-microsecond part (pandas and polars default to
nanoseconds) is truncated without any error or warning, so `12:00:00.000000999` becomes
`12:00:00.000000`.

## Impact

A DataFrame converted to records loses precision silently. That matters for event data, and for
`liquers-py-records`' `from_polars`, which uses this bridge. The Arrow inbound path of the same
design refuses a lossy cast, so the same data would be refused there and accepted here.

## Expected behaviour

Refuse a lossy conversion with a `ConversionError` naming the column and the first affected row,
or keep the truncation but report it (a warning in the state's log). A `Datetime` without
sub-microsecond parts converts as today.

## Discovery

Found 2026-11-01 by the second codebase review of `liquers-py-records` Phase 1 (question 9's polars
route).

## Triage

2026-11-01 (`liquers-py-records` Phase 2): **eligible for automatic fixing** (`auto-fix.md`): `S`,
a bug fix restoring the plainly intended behaviour (no silent loss, as the Arrow inbound path
already refuses), no new structure or interface, one crate (`liquers-lib`). Of the two expected
behaviours, refusing is the one consistent with the Arrow path, and is the fix to make. To be fixed
in its own spin-off PR.

## Resolution

2026-11-01, in its own spin-off PR (branch `claude/polars-sub-microsecond-datetimes`): the
`FieldType::Timestamp` arm of `append_series_values` first calls `refuse_sub_microsecond_values`,
which refuses a `Datetime(Nanoseconds)` series holding any value with a sub-microsecond part. The
`ConversionError` names the column and the first such row. Whole-microsecond nanosecond values,
and every millisecond or microsecond series, convert as before. Evidence: the unit tests
`bridge_refuses_sub_microsecond_datetimes_naming_column_and_row` and
`bridge_converts_whole_microsecond_nanosecond_datetimes` in `liquers-lib/src/records/polars.rs`.
