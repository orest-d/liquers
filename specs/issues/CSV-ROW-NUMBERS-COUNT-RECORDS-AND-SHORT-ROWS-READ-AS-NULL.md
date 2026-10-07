---
id: CSV-ROW-NUMBERS-COUNT-RECORDS-AND-SHORT-ROWS-READ-AS-NULL
kind: issue
title: CSV errors number records rather than file lines, and a row shorter than the header reads its missing cells as null
status: closed
priority: P3
complexity: S
area: [records]
design: csv-physical-lines-short-rows
created: 2026-09-27
github:
---
# CSV errors number records rather than file lines, and a row shorter than the header reads its missing cells as null

## Problem

In `liquers-records/src/formats/csv.rs` (`check_row_width`, `cell_value`), two things go wrong:

- **Row numbers drift.** An error names "row N" counted in records. When a quoted cell spans lines,
  that no longer matches the line an editor shows.
- **Short rows are accepted.** A row wider than the header is now refused, but a row with fewer
  cells than the header is still accepted. Its missing cells read as null without notice, so a
  truncated line in a not-null column fails only later, with an error about nulls rather than
  about the line.

## Expected behaviour

Error messages give the physical line, and the record number where it differs. A short row is
refused like a wide one; a lenient read option could keep today's behaviour if one turns out to be
wanted.

## Discovery

Found 2026-09-27 while fixing the implementation review's CSV findings.

## Resolution (2026-10-07)

Implemented by [`design/csv-physical-lines-short-rows/`](../design/csv-physical-lines-short-rows/),
following the maintainer decision of 2026-10-06. Short rows "may be padded with nulls or empty
strings. They should not be silently ignored. A warning should be written to log … in an
aggregate way for the whole CSV".

- CSV errors now name the physical line the record starts on, and the record number when the two
  differ.
- A short row is padded: null in a nullable field, `""` in a non-nullable `Text`, and an error
  naming the line in any other non-nullable field.
- Padding is reported once per read, through `ReadReport`. Commands write the report to the asset
  log; `read_table` writes it to stderr.

Tests:
- `csv_error_names_physical_line_and_record`
- `csv_short_rows_are_padded_and_reported_once`
- `csv_inferred_short_rows_are_padded_and_reported_once`
- `csv_short_non_nullable_text_pads_empty`
- `csv_short_non_nullable_int_is_an_error`
- `csv_trailing_newline_is_not_a_row`
- `to_record_logs_padded_csv_rows`
