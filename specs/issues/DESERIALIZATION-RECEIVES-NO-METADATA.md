---
id: DESERIALIZATION-RECEIVES-NO-METADATA
kind: issue
title: Deserialization receives only bytes, a type identifier and a data format, never the stored metadata
status: draft
priority: P3
complexity: M
area: [core/value, core/assets]
design:
created: 2026-10-19
github:
---

## Problem

`DefaultValueSerializer::deserialize_from_bytes(b, type_identifier, data_format)`
(`liquers-core/src/value.rs`) is the only way a stored value is read back, and the asset layer
calls it through `deserialize_stored_value` (`liquers-core/src/assets.rs`) with exactly those three
inputs. The stored `MetadataRecord` is in hand at that point but is not passed on.

So a fact that would let a reader interpret the bytes better cannot reach the reader through
metadata. The first such fact is a stored table's schema: `design/record-streams/` and the
2026-09-25 updates of `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` name metadata as a place for a
`RecordSchema`, so a CSV or NDJSON file written by Liquers reads back with its types and roles
without a manifest. `design/metadata-application-attributes/` gives metadata a place to carry it
(an `attributes` map); it does not give the reader access to it.

## Impact

A CSV written by Liquers reads back with inferred types; the schema it was written with is lost
unless the format embeds it (Parquet and Arrow IPC do; CSV and NDJSON do not). Workaround: use a
self-describing format, or read through a manifest.

## Expected behaviour

The deserialization path can consult the stored metadata. Options, not chosen here: a defaulted
`deserialize_from_bytes_with_metadata(b, metadata)` on the trait that the asset layer calls and that
defaults to the existing method; or a metadata-aware hook only on the record-view reader in
`liquers-lib`. The design decides, together with which attribute name a schema uses.

## Discovery

Filed 2026-10-19 while designing `design/metadata-application-attributes/` in a bulk-design run: the
issue it designs records this as a second requirement, which that design leaves out of scope.
Verified at HEAD: `deserialize_stored_value` passes no metadata.
