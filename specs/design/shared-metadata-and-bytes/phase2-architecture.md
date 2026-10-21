# Phase 2: Solution & Architecture — Share metadata records and stored bytes

## Overview

Three stages, each a separate PR, each compile-checked across the workspace:

- **A — metadata.** `Metadata::MetadataRecord` holds `Arc<MetadataRecord>`; every `&mut self`
  method of `Metadata` writes through `Arc::make_mut`; the two type-field setters compare before
  writing; the asset installs values through one helper that syncs the type fields, so a read
  clones two pointers and copies nothing. `from_metadata` takes `Arc<MetadataRecord>`;
  `metadata_record()` returns `Option<Arc<MetadataRecord>>`.
- **B — bytes at the store, asset and HTTP boundaries.** `bytes::Bytes` in `liquers-core`;
  `AsyncStore::get`/`get_bytes`/`get_data` return it; `AssetData::binary` and the binary read
  family carry it; `BinaryResponse` sends it as the body.
- **C — bytes in values.** `Value::Bytes(Bytes)`, `SimpleValue::Bytes { value: Bytes }`,
  `ValueInterface::from_bytes(Bytes)`.

Rejected: copy-on-write in `AssetData` only (`Arc<Metadata>` there) — a `Value::Metadata` could not
share a record; `State::metadata: Metadata` instead of `Arc<Metadata>` — touches several hundred
readers for one pointer; the `bytes` `serde` feature — CBOR writes a byte string where `Vec<u8>`
wrote an array (Q5); `Arc<[u8]>` or `opendal::Buffer` as the byte type (Q1); `set(Bytes)` (Q3).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `CORE-VALUE-ENUM-OVERSIZED` | draft (design ready) | P2 | stage A shares into its `Arc<MetadataRecord>` variant; stage C edits the same enums | yes | no | `requires` |
| `CORE-STORE-OPENBIN-MISSING` | accepted (design needs-decision) | P3 | adds `bytes` too; same trait, other methods | no | no | either order |
| `SERIALIZED-BINARY-RETAINED-WITH-NO-DISPOSAL-POLICY` | draft | P2 | its `Arc::ptr_eq` on the binary becomes `as_ptr` + `len` | no | no | second rebases |
| `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` | draft | P2 | `Metadata::add_log_entry` legacy arm `unwrap`s; untouched here | no | no | none |
| `STORE-WHOLE-WRITE-COPIES-CALLER-BYTES` | filed here | P3 | the non-goal of Q3 | no | no | none |

## Interfaces

```rust
// liquers-core/Cargo.toml: bytes = "1.12.1"   (no features); liquers-core/src/lib.rs: pub use bytes::Bytes;

// liquers-core/src/metadata.rs — stage A
pub enum Metadata { LegacyMetadata(serde_json::Value), MetadataRecord(Arc<MetadataRecord>) }
impl Metadata {
    pub fn metadata_record(&self) -> Option<Arc<MetadataRecord>>;      // was Option<MetadataRecord>
    pub fn record_mut(&mut self) -> Option<&mut MetadataRecord>;        // new, Arc::make_mut
}
impl From<Arc<MetadataRecord>> for Metadata;                            // new; From<MetadataRecord> wraps
// with_type_identifier / with_type_name: no write (no make_mut) when the value is already equal

// liquers-core/src/value.rs — stage A, then C
fn from_metadata(metadata: Arc<MetadataRecord>) -> Self;               // was MetadataRecord
fn from_bytes(b: Bytes) -> Self;                                         // stage C, was Vec<u8>
pub enum Value { /* … */ #[serde(with = "bytes_as_seq")] Bytes(Bytes), /* … */ }   // stage C
pub mod bytes_as_seq { pub fn serialize<S>(…); pub fn deserialize<'de, D>(…) -> Result<Bytes, D::Error>; }

// liquers-core/src/store.rs — stage B (async_trait, unchanged ?Send handling)
async fn get(&self, key: &Key) -> Result<(Bytes, Metadata), Error>;
async fn get_bytes(&self, key: &Key) -> Result<Bytes, Error>;
async fn get_data(&self, key: &Key) -> Result<Bytes, Error>;

// liquers-core/src/assets.rs — stage B: every (Arc<Vec<u8>>, Arc<Metadata>) becomes (Bytes, Arc<Metadata>):
// AssetData::{binary, poll_binary, poll_binary_any_status, binary_unchecked}, PreparedVersion::binary,
// AssetRef::{serialize_to_binary, get_binary, poll_binary, poll_binary_any_status,
// get_binary_any_status, try_poll_binary}, AssetManager::get_binary_any_status.

// liquers-axum/src/api_core/response.rs — stage B
pub struct BinaryResponse { pub data: Bytes, pub metadata: Metadata }
```

All synchronous except the store methods, which stay `async` as today. `Bytes` is `Send + Sync`,
so the wasm `MaybeSend` bounds are unaffected.

## Integration Points

**Stage A.** `metadata.rs`: the enum, `new`, `from_error`, `from_json_value`, the 26 `&mut self`
methods, `metadata_record`, `record_mut`, `From` impls; `Serialize` unchanged (it serializes the
record through the `Arc`). `assets.rs`: a private `AssetData::install_value(&mut self, Arc<V>)`
that sets `data` and the two type fields, used at the production install sites (`AssetData`
fast-track load, `finish_run` value install, `set_value`, `AssetRef::set_state` path,
`DefaultAssetManager::set_state` and `ImmediateAssetManager::set_state` — the last two then share
the state's `Arc`, AC-9); the `if let Metadata::MetadataRecord(x) = &mut …` sites in `assets.rs`,
`interpreter.rs` and `store/pipeline.rs` become `record_mut()`. Callers of `metadata_record()`:
`context.rs`, lib `commands.rs` (`to_metadata`, `to_assetinfo`), lib `template/host.rs` (drops its
`map(Arc::new)`), `liquers-py/src/metadata.rs` (`(*arc).clone()` where it needs an owned record).
`from_metadata` implementors: core `Value`, `SimpleValue`, `CombinedValue`; callers:
`interpreter.rs` (two), lib `commands.rs`.

**Stage B.** `liquers-core/src/store.rs`: the trait, `SharedStore`, `NoAsyncStore`,
`AsyncMemoryStore` (entry `data: Option<Bytes>`, so `get` clones the handle), `AsyncFileStore`
(`Bytes::from(vec)`), `AsyncStoreRouter`, test stores; `store/read_only.rs`, `store/pipeline.rs`,
`store/storage.rs` (`read_object`), `store/test_support.rs`, `store_conformance/` stubs;
`assets.rs` binary family and its test stores; `recipes.rs` test store; `interpreter.rs`
`GetResource` (`from_bytes(Vec::from(data))` until stage C). `liquers-store/src/opendal_store.rs`:
`buf.to_bytes()` in `get_bytes` and `get_data`; `Buffer::from(data.to_vec())` replaces
`Buffer::from_iter(data.iter().copied())` in `set` and `set_data`. `liquers-web/src/store/`
`local_storage.rs`, `fetch.rs`, `js_store.rs`, `wrapper.rs`. `liquers-records/src/sources.rs`
(`read_resource`: `Vec::from(bytes)`), `provider.rs` test stores. `liquers-lib/src/mem/`,
`search/mod.rs`, `egui/widgets.rs` (deref). `liquers-axum`: `query/handlers.rs` and
`assets/common.rs` pass the `Bytes` without `(*data).clone()`; `store/handlers.rs` `get_data_handler`;
`axum_integration.rs` `Body::from(self.data)`.

**Stage C.** `value.rs` (`Value::Bytes`, `from_bytes`, `as_bytes`/`try_into_bytes` arms copy to
`Vec` as today, `bytes_as_seq`); `liquers-lib/src/value/simple.rs` (`SimpleValue::Bytes`,
`from_bytes`, serializer arms), `extended.rs` (`from_bytes`); `interpreter.rs` `GetResource` drops
the conversion; `liquers-web` one `Value::Bytes` site; tests constructing `Value::Bytes(vec)` use
`.into()`.

## Error Handling

No new failure modes and no new constructors. `record_mut` returns `None` for legacy metadata,
where callers already branch; `bytes_as_seq::deserialize` reports the element error serde gives
for a `Vec<u8>`.

## Relevant Commands

No signature change: `to_metadata` and `to_assetinfo` keep `fn(state, context) -> result`; their
bodies share the record. `specs/command_registry.yaml` is unchanged, but `to_metadata` has
`version: auto`, so its body change changes its `impl_version`: regenerate the registry and add a
CHANGELOG line (CLAUDE.md §Maintaining `specs/command_registry.yaml`).

## Documentation Architecture

- `specs/reference/VALUE_TYPE_SYSTEM.md` (reference, internal, `core/value`): §What a value variant
  may hold — `Bytes` holds `bytes::Bytes`; `Metadata` shares its record copy-on-write. History row,
  `reviewed:`.
- `specs/reference/ASSETS.md` (reference, `core/assets`): §AssetData `binary` is `Bytes`; reads
  share metadata. History row, `reviewed:`.
- `specs/reference/STORE_SEMANTICS.md` (reference, `core/store`): §11 data-only access — reads
  return `Bytes`, a store hands out its buffer. History row, `reviewed:`.
- `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` (guide): "return the backend's buffer"; History row.
- `affects_docs`: the four above. No `specs/README.md` change beyond the design-folder link.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | stage A: `metadata.rs`, `assets.rs`, `interpreter.rs`, `context.rs`, `store/pipeline.rs`, `value.rs`, lib `commands.rs`, `simple.rs`, `extended.rs`, `template/host.rs`, py `metadata.rs`; stage B: ~25 files in six crates (above); stage C: `value.rs`, `simple.rs`, `extended.rs`, `interpreter.rs`, tests |
| Crates and workflows affected | all store implementors; every embedder that implements `AsyncStore` or `ValueInterface` (maintainer: no compatibility needed) |
| Existing tests likely to change | test stores' `get` signatures; asserts comparing `get` results with `b"..."` literals (`Bytes` compares with `Vec<u8>` and `[u8]`, not `[u8; N]`: write `&b"..."[..]`); `Metadata::MetadataRecord(record)` constructions in tests (`.into()`) |
| New validation | AC-1..AC-9 tests (Phase 3); store conformance suites unchanged; `scripts/check-build-matrix.sh` (wasm rows: web stores) |
| Compatibility / data / concurrency / performance / security | wire formats pinned by AC-5; copy-on-write means a writer pays one copy while a reader holds a snapshot — the same cost as today's unconditional copy, paid less often; `Bytes` slicing could keep a large buffer alive through a small slice (no slicing is introduced here) |
| Recovery | stages revert independently; within a stage the compiler enforces completeness |
| Certainty and open questions | high on mechanics (APIs read in `opendal 0.55` `Buffer::to_bytes`, `bytes 1.12`); Q1-Q3 fix public signatures |
