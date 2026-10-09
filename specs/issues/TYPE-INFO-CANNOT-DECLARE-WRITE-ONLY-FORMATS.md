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

## Update 2026-09-27 — after `record-streams` was implemented

Still open, and now met in practice. `record-streams` shipped HTML as a write-only table format.
`RecordView`'s `TypeInfo` (`liquers-lib/src/value/mod.rs`) declares `html` in its single format list,
because leaving it out would refuse `data.html`. So the registry claims a stored `.html` table can
be read back, and it cannot. The design took the first of the two bad options this record
describes; the representation is still to be settled.

## Update 2026-10-08 — folded into `DATA-FORMAT-CONSTANTS-AND-TOOLING`

Maintainer decision (backlog compaction D8): this gap is solved inside
[`DATA-FORMAT-CONSTANTS-AND-TOOLING`](DATA-FORMAT-CONSTANTS-AND-TOOLING.md), whose design must include a
way to declare a write-only format. Its standalone design `design/type-info-write-only-formats/` is
`abandoned` and kept as input. This issue stays open until that feature lands.

## Evidence (2026-10-09)

Before `specs/design/combined-value-identifier-dispatch/`, a `RecordView` stored as `html` loaded
through the combined `Value` as `Text`, which hid this issue for that case. It now fails to load (the
extension's refusal is final), so the "declare it" consequence above applies to it as written.
