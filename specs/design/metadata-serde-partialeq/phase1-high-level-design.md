# Phase 1: High-Level Design - Serde and Equality for `Metadata`

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** The wire form is already fixed by `Metadata::to_json` / `from_json`
  (untagged: the inner document, record first, legacy fallback), so the trait implementations
  only have to reproduce it; equality is plain structural equality of two already-`PartialEq`
  variants.
- **Open questions:** None

## Problem and Evidence

`liquers_core::metadata::Metadata` (`liquers-core/src/metadata.rs`, `pub enum Metadata`, ≈1819)
derives only `Debug, Clone`. Both payloads support all three missing traits:
`MetadataRecord` derives `Serialize, Deserialize, Debug, Clone, Default, PartialEq` (≈1115) and
`LegacyMetadata` holds a `serde_json::Value`. Any struct that embeds a `Metadata` therefore cannot
derive them; `liquers-records/src/batch.rs` `ChunkDescriptor` is the concrete case and carries a
doc comment explaining that it is `Debug + Clone` only because of this issue.

## Expected Behaviour and Acceptance Criteria

1. `Metadata: PartialEq` — two values are equal iff they are the same variant with equal payloads.
2. `Metadata: Serialize` produces exactly what `Metadata::to_json` produces today: the inner
   `MetadataRecord` or the legacy JSON value, with no variant tag.
3. `Metadata: Deserialize` accepts exactly what `Metadata::from_json` accepts and chooses the same
   variant: a document `MetadataRecord` accepts (`#[serde(default, deny_unknown_fields)]`) becomes
   `MetadataRecord`; any other JSON value becomes `LegacyMetadata`.
4. Serializing then deserializing a `Metadata::MetadataRecord` yields an equal value; a legacy
   document with unknown fields keeps them (no data loss).
5. A struct holding `pub metadata: Metadata` can `#[derive(PartialEq, Serialize, Deserialize)]`.
6. `to_json` / `from_json` keep their current output and behaviour.

## Affected Users, Workflows and Systems

Rust code composing `Metadata` into serializable types (record streams, future HTTP payloads,
snapshot tests). No query, command, store or wire format changes: stores already write the same
untagged form through `to_json`.

## Scope and Non-Goals

In scope: the three trait implementations, tests, and refreshing the now-stale
`ChunkDescriptor` doc comment. Not in scope: adding derives to `ChunkDescriptor` (its `query:
Query` field serializes in `Query`'s struct form, unlike `MetadataRecord`'s string form, which is
a separate record-streams wire decision), changing `LegacyMetadata` handling, or `liquers-py`.

## Compatibility, Migration, Security and Data Format

Purely additive. The serialized form is the existing store format, so no migration. A legacy
document that happens to be a valid partial record deserializes as `MetadataRecord`, exactly as
`from_json` already does — round-trip identity holds for the *JSON*, not necessarily for the
variant of a hand-built `LegacyMetadata`; this is recorded in the docs of the impl.

Deserialization goes through a buffered `serde_json::Value`, so it works for self-describing
formats (JSON, YAML, TOML) and not for non-self-describing ones (bincode). No current caller uses
the latter.

## Documentation Assessment

No reference document describes `Metadata`'s trait set. `specs/reference/VALUE_TYPE_SYSTEM.md`
and `reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` mention `Metadata` but make no claim
about its traits — review only. Close the issue on implementation.

## Design Dependencies

- `overlaps` `record-streams` (complete): its `ChunkDescriptor` is the motivating consumer; this
  design only refreshes the doc comment there and does not change that type's derives.

## Consolidated Findings

- The untagged form is not a design choice but a constraint: stores already persist `to_json`
  output, so a derived (externally tagged) `Serialize` would introduce a second, incompatible
  representation of the same type. The impls must be hand-written and delegate to the existing
  functions' logic.
- `#[serde(untagged)]` on the enum is not a substitute: variant order puts `LegacyMetadata`
  (a `serde_json::Value`, which accepts anything) first, and reordering the enum is a needless
  source change; a hand-written `Deserialize` mirroring `from_json_value` is explicit.
- `PartialEq` is structural: `LegacyMetadata(json)` never equals a `MetadataRecord` even when the
  JSON is the record's serialization. Acceptable and documented.
- Validation: focused unit tests in `metadata.rs`'s test module plus
  `cargo test -p liquers-core --lib` and the records crate's tests (they compile against the type).

## Review

Scope is one type, additive, with the wire form fixed by existing code. Every acceptance criterion
has a test in Phase 3.
