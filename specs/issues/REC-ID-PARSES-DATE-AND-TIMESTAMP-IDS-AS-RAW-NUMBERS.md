---
id: REC-ID-PARSES-DATE-AND-TIMESTAMP-IDS-AS-RAW-NUMBERS
kind: issue
title: ns-rec/rec_id reads a Date or Timestamp id as a raw day or microsecond count, not as ISO text
status: draft
priority: P3
complexity: S
area: [records, lib/commands]
design: rec-id-iso-date-parsing
created: 2026-09-27
github:
---
# `ns-rec/rec_id` reads a Date or Timestamp id as a raw day or microsecond count, not as ISO text

## Problem

`parse_id_value` (`liquers-lib/src/records/commands.rs`) turns the query's `id` into the declared
`Id` field's type. For `Date` it parses an `i32` (days since the epoch), and for `Timestamp` an
`i64` (microseconds). Every text format writes and reads these types as ISO 8601 (`2026-09-27`,
RFC 3339 with `Z`). So `…/ns-rec/rec_id-2026-09-27` fails to parse, and a user has to compute
`20723` by hand.

The ISO parsers exist, as `parse_date` and `parse_timestamp` in `liquers-records/src/formats/csv.rs`,
but they are `pub(super)`.

## Expected behaviour

`rec_id` accepts the ISO spelling a table shows for a Date or Timestamp id, and perhaps the raw
number as well. The parsers become public in `liquers-records`, for example as a
`FieldValue::parse(FieldType, &str)` that the text readers and `rec_id` share. The query's `-`
separators in a date need checking with `liquers-validate`.

## Discovery

Found 2026-09-27 while writing `specs/reference/RECORD_STREAMS.md` in record-streams Phase 5.
