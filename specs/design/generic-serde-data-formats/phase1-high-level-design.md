# Phase 1: High-Level Design — generic serde data formats

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. The source is `L`, it adds a `pub` module and a `TypeInfo`
  builder to `liquers-core`, two optional dependencies and three feature flags, and it changes
  which serialized formats exist. That fails auto-fix rules 1, 3, 4 and 6.
- **Leading issue:** **Open design - the generic set.** Which formats every serde-capable type gets.
- **Explanation:** a single helper module covers every format, and it is checked against HEAD's
  `as_bytes` / `deserialize_from_bytes` arms. Phases 3-4 assume the recommended answers below.
  Nothing here changes JSON. Serde wire compatibility does not need to be kept.
- **Open questions:**
  1. **Open design - the generic set:** YAML, CBOR and MessagePack. TOML is left out, because a
     TOML document's top level must be a table and TOML has no null. That leaves `None`, every
     scalar, `Array` and `AssetInfo` unwritable, and an `Object` holding a `None` would fail at
     write time. Recommended: yes. A type that wants TOML can still use the helper explicitly.
  2. **Open design - the compact binary format:** use **CBOR** (`cbor`, RFC 8949) as the
     recommended format for caching and transfer, and also offer MessagePack (`msgpack`) for
     interoperability. Both are self-describing. Recommended: yes. Schema-dependent formats
     (`postcard`, `bincode` 2.x) are rejected: they have no `deserialize_any`, so they cannot read
     the untagged core `Value`, `json_f64` fields or the `serde_json::Value` inside metadata.
     `borsh` and `rkyv` are rejected because they are not serde, and every type would need a second
     derive. Because both chosen formats are self-describing, the issue's "refuse without a type
     identifier" rule never applies.
  3. **Open design - feature gating:** `liquers-core` gets features `cbor` (`ciborium`) and
     `msgpack` (`rmp-serde`), off by default under core's wasm-unification rule. `liquers-lib`
     gets `serde-binary = ["liquers-core/cbor", "liquers-core/msgpack"]`, on by default. YAML
     needs no feature, because `serde_yaml` is already non-optional in core. Recommended: yes.
     Both crates are already in `Cargo.lock` (ciborium 0.2.2, rmp-serde 1.3.1).
  4. **Open design - wire shape:** a generic format writes the variant's **payload**, as the JSON
     arm does for identifier dispatch (`core-value-json-identifier-dispatch`), and reading uses the
     type identifier to choose the payload type. An empty identifier reads by shape. YAML is
     different because people write it by hand. In YAML, `SimpleValue`'s `Array` and `Object` are
     written in **plain shape** (`ratio: 0.5`), not in serde's tagged form
     (`ratio: !F64 {value: 0.5}`), and the reader accepts the tagged form first and then the plain
     form. In CBOR and MessagePack they keep the tagged nested form, which is lossless.
     Recommended: yes. The plain YAML shape loses `I32` versus `I64` width and nested `Bytes`
     versus `Array`, just as hand-written YAML does. Core `Value` is untagged, so it is plain in
     every format.
  5. **Proposed resolution - floats:** `json_f64::serialize` writes a native float when the
     serializer is not human-readable (CBOR, MessagePack), and keeps the tokens otherwise. The
     deserializer accepts both forms. JSON output does not change.

## Purpose

Each value type writes every format by hand, one arm per format and crate. So YAML, CBOR and
MessagePack, which serde would give almost for free, are missing, and a core container holding a
non-finite float has no lossless format at all. This design gives every serde-capable payload
those formats from one helper. It also names a compact binary format for caching, which serves
the "N transformations × M formats" point in VISION.

## Problem Example

With `liquers-lib`'s value, `data/params.json` holds `{"ratio": 0.5}`:

```
-R/data/params.json/-/params.yaml
-R/data/params.json/-/params.cbor
```

Both queries validate today (each ends in a `Filename` step). Evaluation fails, because `Object`
declares only `json`. The write path refuses `yaml`, and `cbor` has a media type but no codec.
A hand-written `config/limits.yaml` containing `ratio: .nan` reads back as an `Object` with
`ratio: None`, because `SimpleValue` routes YAML through `serde_json::Value`. Under core `Value`,
`[1.0, NaN]` written as JSON reads back as `[1.0, None]`.

What should happen: the YAML write succeeds and reads back equal, NaN stays NaN, and the CBOR form
round-trips at any depth and is smaller than JSON.

## Scope and Acceptance Criteria

- **AC-1** YAML is written and read
  WHEN `-R/data/params.json/-/params.yaml` is evaluated in a `liquers-lib` environment
  THEN the stored bytes are YAML with data format `yaml`, and reading the key back gives an
  equal `Object`
- **AC-2** YAML reading keeps non-finite floats
  WHEN an untyped `limits.yaml` holding `ratio: .nan` is read as a resource
  THEN `ratio` is `F64(NaN)`, not `None`
- **AC-3** Binary formats round-trip at any depth
  WHEN every serde-capable identifier of core `Value` and `SimpleValue` is written and read as
  `cbor` and as `msgpack`, including core `Array([F64(1.0), F64(NaN), None])`
  THEN each value reads back equal (NaN compared as NaN), and NaN is never read as `None`
- **AC-4** One source for declaration and codec
  WHEN the registry's descriptions are listed in a build with the binary formats enabled
  THEN every serde-capable identifier declares exactly the enabled generic formats, and `as_bytes`
  accepts every pair it declares
- **AC-5** Opt-out is explicit
  WHEN an `Image`, a `polars.DataFrame` or a `RecordView` is written as `cbor`
  THEN the write path refuses it with the existing "cannot be serialized as" error
- **AC-6** Feature off
  WHEN `liquers-core` is built without `cbor`
  THEN no type declares `cbor`, and decoding `cbor` bytes returns a `SerializationError` that
  names the missing feature
- **AC-7** Compact
  WHEN a 1 MiB `Bytes` value and a 10 000-element `F64` `Array` are written as `cbor` and `json`
  THEN the CBOR forms are smaller (the `Bytes` form at most 1 MiB + 16 bytes)
- **AC-8** Untyped binary files read by shape
  WHEN a `cbor` file with no type identifier is read
  THEN a byte string becomes `Bytes`, a map becomes `Object`, and floats stay floats
- **AC-9** JSON is unchanged
  WHEN the existing JSON round-trip and `json_f64` tests run
  THEN they pass unchanged

Non-goals: TOML as a generic format (question 1); a format-choosing command
(`QUERY-CANNOT-CHOOSE-A-DATA-FORMAT`); generic formats for `Image`, `polars.DataFrame`,
`RecordView`, `UIElement` and foreign values; `liquers-py`'s own value type; changing JSON;
zero-copy (`rkyv`) caching.

## Core Interactions

Value types (`DefaultValueSerializer` of core `Value` and `SimpleValue`, `TypeInfo` declarations);
Assets and stores only through the formats they accept (a `.cbor` or `.yaml` key seeds its format,
as other extensions already do); HTTP media types already exist (`application/cbor`,
`application/x-msgpack`).

## Crate Placement

The helper and the core features belong in `liquers-core`, since core `Value` needs them and
`liquers-lib` only declares and dispatches. The feature switch lives in `liquers-lib`, and the wasm
build (`liquers-web`, which uses `default-features = false`) opts in explicitly.

## Design Dependencies

- `data-format-vocabulary`: **requires**. It adds `cbor` and `msgpack` to `DATA_FORMATS`, and
  its AC-3 ("every declared format is known") would fail without them. Land that design first, or
  add the two rows in whichever lands second.
- `core-value-json-identifier-dispatch`: **requires** (soft). Its `json_payload` per-identifier
  rule is generalized here into `serde_format::from_bytes`. Landing it first avoids two rewrites
  of one match.
- `value-type-definition-macro`: **overlaps**. A `serde` key can later point the macro at this
  helper. There is no ordering constraint.
- `value-serialization-writer`: **overlaps**. Both designs edit `as_bytes`, and a writer form
  would use `to_writer` from the same helper. Whichever lands second rebases.
- `serialization-lossy-write-warnings`: **overlaps**. CBOR is the lossless alternative its
  warnings can point to.
- `core-value-enum-oversized`: **overlaps**. If payloads move behind `Arc`, serde needs its `rc`
  feature, and that is already that design's concern.
- `extended-scalar-value-types`: **required-by** (soft). The scalar variants opt into these
  formats.
- The `QUERY-CANNOT-CHOOSE-A-DATA-FORMAT` design (cluster E): **overlaps**. Its command can
  select `cbor`, `yaml` or `msgpack`, and neither design orders the other.

## Documentation Intent

- Reference: extend `specs/reference/VALUE_TYPE_SYSTEM.md` §Reading (generic formats, identifier
  rule).
- Guide: rewrite `specs/guides/SERIALIZATION_GUIDE.md` §Float fidelity by format, and extend
  `specs/guides/TYPE_SYSTEM_GUIDE.md` step 4 with `with_serde_formats()`.
- Documents to update: the three above, plus both source issues on completion.

## Consolidated Findings

- At HEAD only JSON and the text formats exist for base values. `SimpleValue` reads `yaml`/`yml`
  through `serde_json::Value`, which loses NaN, and reads `toml` as plain text, not as TOML.
- Every reading of a generic format goes through the type identifier, exactly as JSON does. Only
  an empty identifier reads by shape, and a self-describing format makes that read safe.
- `SimpleValue`'s `json_f64` field attribute would write the string `"NaN"` into CBOR. The
  `is_human_readable` switch (question 5) makes binary formats native without touching JSON.
- `Vec<u8>` serializes as a sequence of integers. A top-level `Bytes` is written as a byte string
  (`serde_bytes`), and that is what makes AC-7 hold. Nested bytes stay sequences, as they are in
  JSON.
- New `check-build-matrix.sh` rows are needed: core with and without `cbor,msgpack`, and lib
  without `serde-binary`.
- Decisions 1-4 are user-facing, and decision 4 settles what a stored YAML file looks like.
  Decision 5 is a proposed resolution with no known alternative of equal value.
- Plain-shape YAML for `SimpleValue` containers is the only lossy generic path, because it drops
  integer width. Phase 3 asserts exactly that loss, so it cannot spread unnoticed.

## References

`specs/issues/SERDE-TYPES-HAVE-NO-GENERIC-DATA-FORMATS.md`,
`specs/issues/NO-COMPACT-BINARY-DATA-FORMAT.md`, `specs/guides/SERIALIZATION_GUIDE.md`,
`specs/design/data-format-vocabulary/`, `specs/design/core-value-json-identifier-dispatch/`.
