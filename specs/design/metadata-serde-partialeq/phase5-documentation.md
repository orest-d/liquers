# Phase 5: Documentation - Serde and Equality for `Metadata`

**Status: executed 2026-10-07** after implementation (Wave 1 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`). Awaiting approval.

## Completion Preconditions

- [x] Implementation is finished and validated (Phase 4)
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated (none yet)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR

## Documentation Plan

### New Reference Documents

None: the change extends behaviour that existing documents own.

### New Guide Documents

None.

### Existing Documents to Update (`affects_docs`)

None — see the candidates below for why. `affects_docs` is `[]`.

### Candidates Considered and Discarded

By area (`core/value`): `VALUE_TYPE_SYSTEM.md` and `TYPE_SYSTEM_GUIDE.md` describe value types and data formats, not `Metadata`'s Rust traits; `PROJECT_OVERVIEW.md` lists modules only. By content: `WEB_API_SPECIFICATION.md`, `ASSET_SET_OPERATION.md` and `ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` mention `LegacyMetadata` and the stored JSON form, which this change keeps byte-identical. So `affects_docs` is empty: no current document makes a claim this change alters. Reconsider at execution if a document gained such a claim.

### Links and Capability Map

None needed in `specs/README.md`: the capability is reached through the documents above.

### Issues to Close

`METADATA-LACKS-SERIALIZE-DESERIALIZE-AND-PARTIALEQ` → `closed`, resolution naming the five tests.

## Implementation Summary

`liquers_core::metadata::Metadata` derives `PartialEq` (structural) and has hand-written untagged
`Serialize` / `Deserialize` impls that mirror `to_json` and `from_json_value` (record first, legacy
fallback, buffered through `serde_json::Value`). The `ChunkDescriptor` doc comment in
`liquers-records/src/batch.rs` now names the remaining reason it is `Debug + Clone` only (`Query`'s
serialized form); its derives are unchanged. Tests in `metadata.rs` (`tests::metadata_serde`):
`metadata_serialize_matches_to_json`,
`metadata_deserialize_chooses_the_same_variant_as_from_json`,
`metadata_record_round_trips_through_serde` (JSON and YAML), `metadata_partial_eq_is_structural`,
`struct_embedding_metadata_derives_the_traits`.

## Documentation Delivered

None, as planned: no current document makes a claim about `Metadata`'s Rust traits, and the stored
JSON form is unchanged. The trait impls carry their own doc comments.

## Issues Filed

None.

## Important Learning

None beyond the design.

## Conformance and Remaining Work

Conforms to Phases 1–4.

## Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-records --all-features --lib
--tests`; `python3 scripts/docs_index.py --check`.
