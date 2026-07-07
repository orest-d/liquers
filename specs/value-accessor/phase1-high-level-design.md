# Phase 1: High-Level Design - value-accessor

## Feature Name

value-accessor

## Purpose

A `ValueAccessor` trait and module in `liquers-core` providing uniform async read/write access to values, their sub-elements, and query/key structure. Accessors are cheap to clone (`Arc`-backed), `Send + Sync`, and can be stored inside `Value` as `dyn` trait objects. This enables reactive UI bindings, structured editing of queries, and programmatic mutation of store-backed or in-memory values.

## Core Interactions

### Query System
Accessors over `Query`, `Key`, `TransformQuerySegment`, and `ActionRequest` enable structured read/write of query sub-elements. `ActionAccessor` exposes dictionary-like access keyed by parameter names (resolved via `CommandMetadata`).

### Store System
Store-backed accessors (`StoreKeyAccessor`, `StoreTextAccessor`, `StoreBytesAccessor`) read/write raw bytes, text, or deserialized `Value` from/to an `AsyncStore` via a `Key`. `AssetManagerAccessor` variants go through the `AssetManager` instead for cache-aware access.

### Command System
No new commands in Phase 1. Accessors are a programmatic API, not a query-language feature.

### Asset System
`AssetManagerAccessor` variants wrap `AssetManager` get/set to provide cache-aware value access. The accessor does not own an asset; it reads/writes on demand.

### Value Types
A new `Value::Accessor(Arc<dyn ValueAccessor<Value>>)` variant is added to `liquers-core::value::Value` (and `ExtValue` in `liquers-lib` gets `ExtValue::Accessor`). Accessors stored in values must use `Arc` for cheap clone.

**Structured value accessors** provide dictionary-like access into structured `Value` variants:
- `CommandMetadataAccessor` — wraps `Arc<Mutex<CommandMetadata>>`, dict-like access to metadata fields (name, namespace, label, doc, parameters, …)
- `AssetInfoAccessor` — wraps `Arc<Mutex<AssetInfo>>`, dict-like access to asset info fields
- `MetadataRecordAccessor` — wraps `Arc<Mutex<MetadataRecord>>`, dict-like access to metadata record fields
These also support element accessors: accessing a field returns a sub-accessor (or `Value`) for that field.

### Web/API (if applicable)
None in Phase 1.

### UI (if applicable)
Primary motivation: `ValueBinding`-style two-way binding for UI widgets. No UI code in this module; UI crates (`liquers-lib`) will use accessor trait.

## Crate Placement

All of Phase 1 lives in **`liquers-core/src/accessor.rs`** (new module).

- `ValueAccessor` trait, `ArrayAccessor`, `DictAccessor` sub-traits
- Predefined concrete accessors for store, asset manager, and query/key structure
- `Value::Accessor` variant added to `liquers-core::value`
- No new crate dependencies; uses existing `AsyncStore`, `AssetManager`, `CommandMetadata`, `Query`, `Key`

## Decisions

1. `get()` returns `Result<Value, Error>` — it may fail (store unavailable, key missing, deserialization error). Relies on store and asset manager `get`.
2. Query-structure accessors use in-place mutation via `Arc<tokio::sync::Mutex<...>>` interior mutability.
3. Array and dictionary access are optional capabilities — not every accessor supports both. Attempting unsupported access returns `Error`. No fallback to stringified indices for dict access.

## References

- `liquers-core/src/query.rs` — `Query`, `Key`, `TransformQuerySegment`, `ActionRequest`, `ActionParameter`
- `liquers-core/src/assets.rs` — `AssetManager`, `AssetRef`
- `liquers-core/src/store.rs` — `AsyncStore`
- `liquers-core/src/value.rs` — `Value`, `ValueInterface`
- `liquers-core/src/command_metadata.rs` — `CommandMetadata`, parameter names
