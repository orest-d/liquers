---
id: CORE-METADATA-NO-APPLICATION-ATTRIBUTES
kind: feature
title: Metadata cannot carry application-defined attributes
status: draft
priority: P2
complexity: M
area: [core/value]
design: 
created: 2026-09-15
github:
---
## Problem

`MetadataRecord` (`liquers-core/src/metadata.rs:871`) is a closed struct. Every field it carries —
`title`, `description`, `type_identifier`, `status`, `version`, `dependencies`, `expires` and the
rest — is defined by Liquers, and there is no field an application may write into. An application
that wants to annotate a stored value with facts Liquers has no opinion about (tags, a source URL,
a confidence score, a retention class, a last-used timestamp) has nowhere to put them.

The `Metadata::LegacyMetadata(serde_json::Value)` variant is not an answer. It is an
all-or-nothing alternative to `MetadataRecord`, not an extension of it: choosing it gives up every
typed field and every typed accessor, and `Metadata::from_json` already falls into it silently
whenever a document fails to deserialize as a full record, which
`CORE-LEGACY-METADATA-ACCESSORS-RETURN-JSON` showed is its own trap.

## Impact

Any application storing values whose meaning is richer than "a typed blob with a title" has to
keep its attributes somewhere else — in the document body, in a sidecar key, or in a separate
index — and then keep that second place in sync with the store. All three lose the property that
made store metadata useful: it travels with the value, through `set`, `get`, `listdir_asset_info`
and the HTTP metadata endpoints, without the consumer knowing the application's schema.

Surfaced while designing `AGENT-MEMORY-SERVICE`, where memory entries need tags and provenance.
The workaround there is to keep attributes in the document's YAML front-matter and re-parse it on
every read, which works for a Markdown corpus and does not generalize to binary values at all.

## Expected behaviour

`MetadataRecord` carries an open attribute map — `#[serde(default)] pub attributes:
BTreeMap<String, serde_json::Value>`, or an equivalent — that Liquers itself never interprets,
preserves across `set`/`get` round-trips, and merges rather than discards when metadata is
finalized by a store.

Questions for the design:

- Whether the map is flat or namespaced by application, and whether unknown keys should round-trip
  even when a store rewrites the record.
- Whether `finalize_metadata` and `finalize_metadata_empty` may touch it (they should not).
- Whether attributes participate in the content-hash `version` (they should not — a retagged
  document is not a changed document — which means they must be excluded deliberately rather than
  by accident).
- What, if anything, the HTTP metadata endpoints promise about them.

## Discovery

Analysis for `AGENT-MEMORY-SERVICE`, 2026-09-15. Verified at HEAD: `MetadataRecord` has no
extension field, and `Metadata` has exactly two variants.

## Update 2026-09-25 — a second use: a stored table's schema

`specs/design/record-streams/` Phase 2 ("Should the schema live in metadata?") names this as where a
stored table's `RecordSchema` would live, so that a CSV or NDJSON file written by Liquers reads back
with its types and roles without a manifest. It needs two things beyond this issue: the attributes
must be opaque to core (the schema type is `liquers-lib`'s), and the load path must hand metadata to
`deserialize_from_bytes`, which today receives only bytes, a type identifier and a data format. The
records design takes its schema as an argument to one schema-aware reader, so a metadata schema would
be one more source for it rather than a new mechanism.

