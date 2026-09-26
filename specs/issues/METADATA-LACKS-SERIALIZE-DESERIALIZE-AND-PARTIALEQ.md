---
id: METADATA-LACKS-SERIALIZE-DESERIALIZE-AND-PARTIALEQ
kind: issue
title: Metadata lacks Serialize, Deserialize and PartialEq
status: draft
priority: P2
complexity: S
area: [core/value]
design: 
created: 2026-09-26
github:
---
## Problem

`liquers_core::metadata::Metadata` (`liquers-core/src/metadata.rs:1605`) derives only `Debug` and
`Clone`:

```rust
#[derive(Debug, Clone)]
pub enum Metadata {
    LegacyMetadata(serde_json::Value),
    MetadataRecord(MetadataRecord),
}
```

Its inner types are fully equipped — `MetadataRecord` derives `Serialize, Deserialize, Debug,
Clone, Default, PartialEq` (line 909) — but the wrapping `Metadata` enum has no manual or derived
`PartialEq`, `Serialize` or `Deserialize` anywhere in the file. Every other struct in the record
streams design that Phase 2 (`specs/design/record-streams/phase2-architecture.md`, §"Provenance
and validity: the chunk carries a `Metadata`") composes with `Metadata` inherits this gap: a struct
embedding `pub metadata: Metadata` cannot derive those three traits either.

## Impact

`liquers-records::batch::ChunkDescriptor` (Step 2.3 of the record-streams Phase 4 plan) is the
concrete case: Phase 2's code block specifies
`#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] pub struct ChunkDescriptor { ...
pub metadata: Metadata, ... }`, which does not compile as written. The workaround taken there is to
derive only `Debug, Clone` on `ChunkDescriptor`, which is sufficient because `ChunkDescriptor` is
not on any wire format yet (`RecordSource::describe_chunk`, landing in Step 2.6, returns it for
in-process use only). That workaround will stop being sufficient the moment something needs to
serialize a `ChunkDescriptor` — over HTTP, into a cache, or in a snapshot test comparing two of
them — and any other future type that wants to carry a `Metadata` field hits the same wall.

## Expected behaviour

`Metadata` should derive (or hand-implement, since `LegacyMetadata`'s `serde_json::Value` payload
already supports all three) `PartialEq`, `Serialize` and `Deserialize`, matching what
`MetadataRecord` already provides. There is no apparent reason for the enum wrapper to be more
restrictive than its own variants — this may simply have been missed when `Metadata` was
introduced as a wrapper over `LegacyMetadata`/`MetadataRecord`.

## Discovery

Found while implementing Step 2.3 of the record-streams Phase 4 plan (`liquers-records/src/batch.rs`,
`ChunkDescriptor`): the type as specified in Phase 2 failed to compile because `Metadata` lacks
these derives, confirmed by grepping `liquers-core/src/metadata.rs` for `impl Serialize for
Metadata` / `impl PartialEq for Metadata` (none found) and reading its derive line directly.
