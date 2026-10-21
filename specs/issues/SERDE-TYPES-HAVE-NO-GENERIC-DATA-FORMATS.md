---
id: SERDE-TYPES-HAVE-NO-GENERIC-DATA-FORMATS
kind: feature
title: A serde-capable value type gets no data format without a hand-written arm per format
status: draft
priority: P2
complexity: L
area: [core/value, lib/value]
design:
created: 2026-10-20
github:
---

## Problem

Every value type hand-writes each format in `as_bytes` and `deserialize_from_bytes`
(`liquers-core/src/value.rs`, `liquers-lib/src/value/simple.rs`, `liquers-lib/src/value/mod.rs`).
Most variants are plain `serde` types whose JSON support is one `serde_json` call, so adding YAML,
TOML, CBOR or MessagePack means editing every arm in every crate, and nobody has. At HEAD:

- `SimpleValue` reads `yaml`, `yml` and `toml` but writes none of them and declares none (an
  accidental read-only asymmetry); its `yaml` read goes through `serde_json::Value`, so `.nan` and
  `.inf` become `None`.
- CBOR and MessagePack have media types (`liquers-core/src/media_type.rs`) but no codec, so a core
  `Value` container holding a non-finite float has no lossless format
  (`specs/guides/SERIALIZATION_GUIDE.md`, "Float fidelity by format").

## Impact

Formats Liquers could support almost free stay unavailable because their cost is per type rather
than per format. Workaround: JSON (lossy for nested non-finite floats in core containers).

## Expected behaviour

Point 3 of `DATA-FORMAT-CONSTANTS-AND-TOOLING`, moved here when that feature's first slice was
designed (`specs/design/data-format-vocabulary/`): a blanket helper a value type opts into per
variant, so a `Serialize + DeserializeOwned` payload gets the generic formats, the `TypeInfo`
declaration and the implementation come from one place, and the opt-out is first-class (a polars
`DataFrame`, an image and a foreign handle keep their own encoders). It decides which formats are
generic (YAML, TOML, CBOR, MessagePack are the candidates), which dependencies are added and behind
which features, and how an untagged core `Value` keeps `F64(NaN)` distinct from `None` in a
self-describing format. Uses the format constants of `liquers_core::data_format`.

## Discovery

Split out on 2026-10-20 in a bulk-design run from `DATA-FORMAT-CONSTANTS-AND-TOOLING` (raised by
the user on 2026-08-18; the non-finite float note was added there on 2026-10-13), whose design
covers only the vocabulary and write-only formats.
