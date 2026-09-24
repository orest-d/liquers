---
id: TYPE-INFO-CANNOT-DECLARE-WRITE-ONLY-FORMATS
kind: issue
title: TypeInfo cannot declare a format a type writes but cannot read back
status: draft
priority: P3
complexity: S
area: [core/value]
design:
created: 2026-09-24
github:
---
# `TypeInfo` cannot declare a format a type writes but cannot read back

## Problem

`TypeInfo::supported_data_formats` is one list, documented as the formats a type "can be written to
**and** read from" (`liquers-core/src/type_system.rs:100-104`). The write path refuses any format
outside it. So a format that is deliberately **write-only** has two bad options:

- **Declare it**, and the registry claims a stored file in that format can be loaded. It cannot: the
  load fails in `deserialize_from_bytes`, is logged as corrupted, and the asset is recomputed
  (`METADATA-ONLY-ENTRY-RELOADS-AS-CORRUPTED`).
- **Leave it out**, and the format cannot be requested by filename at all, although producing it
  works.

The case that raised it: `record-streams` renders a table as an HTML `<table>` for display. Writing is
~60 lines; reading HTML back needs an HTML parser. The same shape applies to any presentation format
— a chart rendered to SVG, a report to PDF.

## Expected behaviour

`TypeInfo` distinguishes writable from readable formats — two lists, or a per-format capability —
so the write path accepts a write-only format, the registry reports it honestly, and the load path
skips deserialization for it and goes straight to recomputation. `DATA-FORMAT-CONSTANTS-AND-TOOLING`
(the format vocabulary) is the natural place to settle the representation.

## Discovery

Found 2026-09-24 while specifying the table formats of `specs/design/record-streams/` Phase 2.
`DATA-FORMAT-CONSTANTS-AND-TOOLING` records *accidental* read/write asymmetries (`toml` readable but
not writable); this is about declaring a *deliberate* one.
