---
id: STORED-VALUE-WITHOUT-DATA-FORMAT-READ-AS-BIN
kind: issue
title: A stored value with no declared data format is deserialized as bin, not its own default format
status: draft
priority: P1
complexity: S
area: [core/assets]
created: 2026-10-18
design: stored-value-default-data-format
github:
---

## Problem

The read path passes `metadata.get_data_format()` to `deserialize_stored_value`
(`liquers-core/src/assets.rs`, both the fast-track and the store-fallback paths). That function
substitutes the constant `bin` when the metadata declares no format (`metadata.rs`
`MetadataRecord::get_data_format`). The write side serializes such a value in its *own* default
format (`state.rs`), for example JSON for an `I64` or an `Array`. A value stored with no declared
format is therefore read back with the wrong format and fails as corrupted.

This has not been reproduced in a test. First confirm whether every write path now fills
`data_format` before storing, which would leave only older stores affected.

Related, write side: `FREE-FUNCTION-EVALUATE-BAKES-A-BIN-DATA-FORMAT` (closed).

## Impact

If reachable, a stored value with no declared format cannot be read back, which looks like data
corruption. Workaround: always write an explicit `data_format`, as `liquers-py` does.

## Expected behaviour

The read path uses the stored type's default data format (`TypeInfo::default_data_format`) when
the metadata declares none, which is the level-1 rule `effective_data_format` already describes.

## Discovery

A reviewer of `specs/design/liquers-py/` Phase 2 found it by reading the read and write paths,
2026-10-18.

## Triage

Filed without a design: it is outside `liquers-py`'s scope, which designs around it by always
writing an explicit `data_format` (`specs/design/liquers-py/phase2-architecture.md` preflight). It
needs a reproduction before a design, and it is a candidate for automatic fixing once reproduced.

## Reproduction at HEAD (bulk design, 2026-10-20)

Confirmed by reading the code at 18732dd, and worse than filed. Every write path leaves
`data_format` undeclared for a key without an extension: `AssetManager::set_state` and
`AssetRef::serialize_to_binary` store the state's metadata as it is, and
`AsyncStore::finalize_metadata` seeds a format only from the key's extension. The bytes are
written in the value's own default format (`State::effective_data_format`). On read,
`deserialize_stored_value(…, "bin", …)` does not fail: `Value::deserialize_from_bytes` answers
`Value::Bytes` for `bin` whatever the type identifier, so `-R/counter` holding `I64(42)` returns the
two bytes `42` with no error. Priority raised from P3 to P1: a wrong value without an error on an
extensionless key (it meets the P0 wording if extensionless keys are a supported path; review
decides). Design: `stored-value-default-data-format`.
