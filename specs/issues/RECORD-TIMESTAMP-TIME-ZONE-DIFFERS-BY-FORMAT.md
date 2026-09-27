---
id: RECORD-TIMESTAMP-TIME-ZONE-DIFFERS-BY-FORMAT
kind: issue
title: A record Timestamp column is a UTC instant in Parquet and a naive date-time in IPC and polars
status: draft
priority: P3
complexity: S
area: [records, lib/polars]
design: record-streams
created: 2026-09-27
github:
---
# A record `Timestamp` column is a UTC instant in Parquet and a naive date-time in IPC and polars

## Problem

`Column::Timestamp` holds microseconds since the epoch, and the text formats write it as RFC 3339
with `Z` (`phase2-architecture.md` §"Table formats"). So the value is a UTC instant. The binary
forms disagree about that:

- **Parquet** (`liquers-records/src/formats/parquet.rs`) writes `TIMESTAMP(MICROS)` with
  `isAdjustedToUTC = true`. polars and pyarrow read it back as `Datetime(us, "UTC")`.
- **Arrow IPC** (`formats/ipc.rs`) writes `Timestamp(Microsecond, None)`, with an empty time zone.
  Phase 2's IPC table specifies exactly this (`tsu:` with an empty zone).
- **The polars bridge** (`liquers-lib/src/records/polars.rs`) produces `Datetime(us, None)`.

So one `RecordView` becomes a different polars dtype depending on the format it passed through, and
a naive Arrow timestamp is, by Arrow's definition, a wall-clock time in an unspecified zone rather
than an instant.

## Expected behaviour

One meaning in every form. The likely answer is `"UTC"` in IPC and in the polars bridge, which
matches the text formats and Parquet. That means changing Phase 2's IPC table, and the IPC reader
then accepts both `"UTC"` and an empty zone. The alternative is `isAdjustedToUTC = false` in
Parquet, which would contradict the `Z` in every text format. This is a decision for whoever picks
it up, and the reference document records the choice.

## Discovery

Found 2026-09-27 by the formats review of the record-streams implementation, before Phase 5. It
was left unfixed because it contradicts a Phase 2 table rather than being a slip in the code.
