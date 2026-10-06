---
id: CSV-ROW-NUMBERS-COUNT-RECORDS-AND-SHORT-ROWS-READ-AS-NULL
kind: issue
title: CSV errors number records rather than file lines, and a row shorter than the header reads its missing cells as null
status: draft
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
