# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — changes the IPC serialized type and the polars dtype (rule
  4), in two crates (rule 6)
- **Leading issue:** **Open design question — Timestamp is a UTC instant everywhere (change IPC
  and the polars bridge), or a naive date-time everywhere (change Parquet and the text formats).**
  It changes a frozen design's IPC table (`record-streams` Phase 2) and the dtype users see in
  polars.
- **Explanation:** The issue and the 2026-09-27 update show four of six forms already agree on
  UTC: text formats with `Z`, Parquet `isAdjustedToUTC = true`, and the JSON `table` orient with
  `"tz": "UTC"`. The recommended answer changes the two outliers and keeps reading tolerant.
- **Open questions:**
  1. **Proposed resolution — UTC everywhere.** IPC writes `Timestamp(Microsecond, "UTC")`. The IPC
     reader accepts `"UTC"`, an empty zone (treated as UTC, for files written before the change),
     and any other zone (values are epoch-based instants in Arrow regardless of zone, so they are
     taken as they are). The polars bridge produces `Datetime(us, Some("UTC"))`, and reading
     accepts any `Datetime` time zone the same way.
  2. **Implementation detail:** the frozen design is not edited. The decision is recorded in
     `specs/reference/RECORD_STREAMS.md`, which becomes the authority for the IPC type mapping.

## Problem

`Column::Timestamp` holds microseconds since the epoch, and text formats write RFC 3339 with `Z`,
so the value is a UTC instant. Parquet agrees. Arrow IPC (`liquers-records/src/formats/ipc.rs`)
writes `Timestamp(Microsecond, None)`, and the polars bridge (`liquers-lib/src/records/polars.rs`)
casts to `Datetime(us, None)`. By Arrow's definition, a naive timestamp is wall-clock time in an
unspecified zone. One view therefore becomes a different polars dtype depending on the path it
took.

Filed during `record-streams` (complete, frozen). The issue's `design:` field pointed at that folder; it now points here, because a frozen design cannot own new work.

## Expected behaviour and acceptance

1. Writing a view to IPC and reading the bytes with pyarrow (or the in-tree IPC reader) yields
   `timestamp[us, tz=UTC]`.
2. `view → polars` yields `Datetime(Microseconds, Some("UTC"))`, and `view → Parquet → polars`
   yields the same dtype as `view → IPC → polars`.
3. IPC files written before the change (empty zone) still read, with identical values.
4. A polars frame with a naive `Datetime` or a non-UTC zone converts to records with the same
   epoch values (no shifting).

## Scope

IPC writer/reader and the polars bridge. Text, JSON and Parquet are unchanged.

## Design Dependencies

- `rec-id-iso-date-parsing` — **overlaps** (timestamp parsing semantics).

## Documentation assessment

- Reference: `specs/reference/RECORD_STREAMS.md`, the format type table: Timestamp = UTC instant;
  IPC `tsu:UTC`; polars `Datetime(us, "UTC")`; reading accepts any zone. History, `reviewed:`.
- Guide: `RECORD_STREAM_GUIDE.md` polars hand-off section, if it names the dtype.

## Consolidated Findings

- Arrow defines timestamp values as epoch offsets in UTC for zoned types. For naive types it
  defines them as wall-clock. Treating naive as UTC on read is the only shift-free choice, and it
  matches how Liquers itself wrote them.
- The IPC writer is hand-written FlatBuffers (`write_timestamp_type`, where field 1 is the zone
  string, currently unused). Adding the string means building a FlatBuffers string and setting
  field 1.
- The polars side has two cast sites (`polars.rs` ~113 and ~213). Both change.
