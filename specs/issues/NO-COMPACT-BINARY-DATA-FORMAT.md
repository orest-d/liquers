---
id: NO-COMPACT-BINARY-DATA-FORMAT
kind: feature
title: There is no compact binary data format for caching and cross-process transfer
status: draft
priority: P3
complexity: L
area: [core/value, lib/value]
design: generic-serde-data-formats
created: 2026-10-20
github:
---

## Problem

Every serialized base value is text (JSON, the text formats) or raw bytes. Where JSON's size and
parse cost dominate (caching, moving values between processes or realms) there is no compact
binary encoding of a Liquers value.

## Problem Example

A 1 MiB `Bytes` value has two stored forms today: `b`, which is raw and carries no structure, and
`json`, which writes `[137, 80, 78, …]` at about 3.5 bytes per byte. A 10 000-element float
`Array` exists only as JSON text. `-R/data/x.json/-/x.cbor` validates and is refused when it is
written, because `cbor` has a media type but no codec. Should be: a self-describing binary format
that writes the bytes as a byte string and floats natively, and reads back exactly.

## Impact

Large structured values (metadata lists, arrays, objects) cost JSON's size and parse time in the
store and over the wire. Workaround: JSON; for tables, `ipc` or `parquet`.

## Expected behaviour

Point 5 of `DATA-FORMAT-CONSTANTS-AND-TOOLING`, moved here when that feature's first slice was
designed (`specs/design/data-format-vocabulary/`). Evaluate `postcard`, `bincode` 2.x, `borsh`,
`rmp-serde` (self-describing) and `rkyv` (zero-copy, not `serde`) on: serde compatibility, wasm
support, stability of the encoding across crate versions (a cache written by one build must be read
by the next), and self-describing versus schema-dependent. A schema-dependent format cannot be
decoded without the type, so it must refuse to decode when the type identifier is absent or
unregistered. Probably built on `SERDE-TYPES-HAVE-NO-GENERIC-DATA-FORMATS`.

## Discovery

Designed together with `SERDE-TYPES-HAVE-NO-GENERIC-DATA-FORMATS` in
`specs/design/generic-serde-data-formats/` (bulk design, 2026-10-26). There, CBOR is recommended
as the compact format, and the schema-dependent candidates are rejected because they cannot read
the untagged core `Value`.


Split out on 2026-10-20 in a bulk-design run from `DATA-FORMAT-CONSTANTS-AND-TOOLING` (point 5,
added by the user on 2026-08-18).
