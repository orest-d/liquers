---
id: POLARS-BRIDGE-TRUNCATES-SUB-MICROSECOND-DATETIMES
kind: issue
title: The polars bridge silently drops sub-microsecond precision from a nanosecond Datetime column
status: draft
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
