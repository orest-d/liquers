# Template data as an interface — `DataNode`

Supporting note for [Phase 1](./phase1-high-level-design.md), open question 10. Written 2026-10-17
from the maintainer's review of the same day: any type should be usable as a template's context
through one trait. Examples are a core `Value`, a polars frame or a record view seen as a list of
maps, a `MetadataRecord`, and a store folder. The trait belongs in `liquers-core`, because the store
pipeline needs the same access (`STORE-HANDLER-FIELD-ACCESS-HARDCODED`).

**Naming.** Mustache calls the data a *context*, while Liquers' `Context` is the per-command
execution context (`liquers-core/src/context.rs`). To keep the two apart, the trait is proposed as
**`DataNode`** in `liquers-core/src/data_node.rs`. "Context" is kept for the template's context
stack.

## What a consumer needs

The renderer, the hygienic expander and a store handler ask a value the same five questions:

| Question | Used by |
|---|---|
| What kind are you: null, scalar, list, map or opaque? | Section truthiness, iteration versus context push, the scalar-or-override split (AC-16) |
| Your scalar text? | Interpolation, hygienic fill (P1–P5) |
| Your field `name`? | Dotted-name lookup (`a.b`), `_meta`, store handlers (`title`, `media_type`, …) |
| Your items, in order? | Sections over lists, records and frames |
| Yourself as a JSON value? | Parameter overrides (`Recipe::arguments` is `serde_json::Value`), JSON output |

## Proposed interface

```rust
// liquers-core/src/data_node.rs
pub enum NodeKind { Null, Bool, Integer, Float, Text, List, Map, Opaque }

pub enum Scalar<'a> { Null, Bool(bool), Integer(i64), Float(f64), Text(Cow<'a, str>) }

/// Synchronous access: values held in memory.
pub trait DataNode: MaybeSend + MaybeSync {
    fn kind(&self) -> NodeKind;
    fn scalar(&self) -> Option<Scalar<'_>>;                       // None for List, Map, Opaque
    fn get(&self, name: &str) -> Option<NodeRef<'_>>;            // Map fields; None otherwise
    fn len(&self) -> Option<usize>;                              // List length, when known
    fn items(&self) -> Box<dyn Iterator<Item = NodeRef<'_>> + '_>; // List items; empty otherwise
    fn keys(&self) -> Box<dyn Iterator<Item = Cow<'_, str>> + '_>; // Map keys; empty otherwise
    fn to_json(&self) -> Result<serde_json::Value, Error>;
    fn is_truthy(&self) -> bool { /* Mustache rule, from kind() and scalar() */ }
}

/// A child, borrowed when the parent holds it, owned when the parent builds it (a frame row).
pub enum NodeRef<'a> { Borrowed(&'a dyn DataNode), Owned(Box<dyn DataNode + 'a>), Scalar(Scalar<'a>) }

/// Asynchronous access: values read on demand from a store, an asset or a record source.
#[async_trait]
pub trait AsyncDataNode: MaybeSend + MaybeSync {
    fn kind(&self) -> NodeKind;                                  // known without I/O
    async fn scalar(&self) -> Result<Option<Scalar<'static>>, Error>;
    async fn get(&self, name: &str) -> Result<Option<AnyNode>, Error>;
    fn items(&self) -> BoxStream<'_, Result<AnyNode, Error>>;
    async fn to_json(&self) -> Result<serde_json::Value, Error>;
}

/// What an async node hands back: a sync child stays sync, so a folder of files costs one await
/// per file, not one per field.
pub enum AnyNode { Sync(Arc<dyn DataNode>), Async(Arc<dyn AsyncDataNode>) }
```

`MaybeSend` / `MaybeSync` (`liquers-core/src/maybe_send.rs`) keep the traits usable on wasm32, like
`RecordView` and `RecordSource`. Writing a field, which the store pipeline also does
(`set_text_field`), is a separate `DataNodeMut` with `set(name, Scalar)`. It is left to the refactor
issue, because templates only read.

## Sync, async or both

| | Sync only | Async only | **Dual** (recommended) |
|---|---|---|---|
| Values in memory: `Value`, `MetadataRecord`, a record view, a polars frame | Natural and cheap: a lookup is a function call | Every field lookup is a boxed future (`async_trait` allocates), and iterators become streams | Implemented sync, so they stay cheap |
| `RecordSource` (`read_chunk` and `advance` are async) and store folders (`AsyncStore`) | **Excluded.** They must be materialized first: a source through `materialize` with its row cap, a folder through a full listing and read | Natural, and lazy: only the fields the template touches are read, so dependencies are exact | Implemented async, lazily |
| Sync consumers: store handlers (`CompiledFn` is sync by design, with no interpreter and no locks), validation, highlighting, egui | Usable | **Unusable.** Bridging needs `block_on`, which is blocking in an async context and does not exist on wasm32 | They take `DataNode` and refuse an `Async` node with a clear error |
| The renderer, which is async anyway because it evaluates queries | Usable, but it cannot take lazy data | Usable | Takes `AnyNode` and awaits only the async arms |
| Cost | One trait | One trait, slower everywhere | Two traits, and consumers match two arms. Each type implements only the trait that fits it |

**Recommendation: dual.** Sync is the default and the only one most types implement. Async exists
for the data that cannot be in memory. The renderer accepts both. A sync consumer, such as a store
handler, accepts sync nodes only, which is all a `MetadataRecord` needs. A blanket
`impl<T: DataNode> AsyncDataNode for T` was considered and set aside. It would make the async arm
uniform, but downstream crates would rely on coherence corner cases to implement `AsyncDataNode` for
their own async types, and `AnyNode` gives the same uniformity explicitly.

## Implementations

| Type | Crate | Trait | Shape |
|---|---|---|---|
| core `Value` | `liquers-core` | sync | Object is a map, Array a list, scalars are scalars; `Bytes` and the like are opaque |
| `MetadataRecord` | `liquers-core` | sync | A map of its fields. Also the `_meta` of a keyed entry |
| `serde_json::Value` | `liquers-core` | sync | Front matter and parsed JSON |
| `RecordView` | `liquers-records` | sync | A list of rows. Each row is a map of columns, built on the fly (`NodeRef::Owned`) |
| `RecordSource` | `liquers-records` | async | A list streamed chunk by chunk |
| `ExtValue`, polars `DataFrame` | `liquers-lib` | sync | `ExtValue` delegates to its payload. A frame is a list of row maps |
| Store folder (a `Key` and the asset manager) | `liquers-lib` | async | A map of child names. A file child is read through the asset layer and recorded as a dependency |

Every implementation is a thin adapter, and none copies the data.

## Open points for Phase 2

- Whether `NodeKind::Map` needs ordered keys (front matter keeps file order, while `Value::Object`
  is a `BTreeMap`).
- How a store folder distinguishes a file child from a folder child without reading it.
