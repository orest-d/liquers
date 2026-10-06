# Phase 2: Solution and Architecture

## Representation (`liquers-records/src/formats/shapes.rs` or a new `formats/ordered_json.rs`)

```rust
/// A JSON object with its keys in document order. Values are ordinary `serde_json::Value`s.
pub(crate) struct OrderedObject(pub(crate) Vec<(String, serde_json::Value)>);

impl<'de> Deserialize<'de> for OrderedObject { /* visit_map collecting entries in order */ }

/// Top-level shapes: an array of ordered objects, or an ordered object of ordered objects.
pub(crate) enum OrderedJson { Array(Vec<OrderedValue>), Object(OrderedObject) }
```

The `index`/`columns` shapes need two ordered levels. Use
`OrderedObject` whose values are deserialized as `OrderedObject` where the shape requires it:
deserialize the top level as `Vec<(String, OrderedObject)>` via a generic
`OrderedMap<V>(Vec<(String, V)>)` with `V: Deserialize`. That gives one generic type,
`OrderedMap<V>`, used as `OrderedMap<serde_json::Value>` for rows and
`OrderedMap<OrderedMap<serde_json::Value>>` for `columns`/`index`.

## Readers

Each schema-less reader computes column order as the first appearance across the ordered rows,
then builds columns as today. Inference of types is unchanged.

## Rejected alternative

`serde_json/preserve_order`. Feature unification enables it for every crate, including core's
`metadata_version` hashing (Phase 1). It could be revisited only with a hash that canonicalizes
map order, which is a separate decision.

## Known-issue preflight

None.

## Relevant commands

`ns-rec/from_json`, `ns-rec/to_record-json`, NDJSON reads.

## Documentation architecture

RECORD_STREAMS.md JSON shapes sentence.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-records/src/formats/{shapes.rs, ndjson.rs, mod.rs}` |
| Existing tests | Schema-less tests that asserted alphabetical columns flip. Search the records tests for column-name vectors in sorted order. |
| Performance | Comparable (Vec instead of BTreeMap; key lookup per row via a small index map) |
| Compatibility | Column order changes for schema-less reads, which is the fix |
| Recovery | Revert |
| Certainty | High |
