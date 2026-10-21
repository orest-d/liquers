# Phase 3: Examples & Use-cases — Share metadata records and stored bytes

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Serving a stored binary | one buffer from store to asset | AC-6, AC-7, AC-8 |
| 2 | Example | Reading an asset twice | metadata shared, copied only on write | AC-1, AC-2, AC-3 |
| 3 | Test | `liquers_core::metadata::tests::shared_record::*` | clone shares, write copies | AC-1, AC-2 |
| 4 | Test | `liquers-core/tests/shared_payloads.rs` | assets, resources, managers | AC-3, AC-7, AC-8, AC-9 |
| 5 | Test | wire-format pins in `metadata.rs`, `value.rs`, `simple.rs` | byte-identical output | AC-5 |
| 6 | Test | `store.rs` `memory_store_get_shares_buffer` + conformance suites | store reads | AC-6 |
| 7 | Test | lib `commands.rs` `to_metadata_shares_state_record` | metadata value | AC-4 |

## Example 1: Serving a stored binary

A memory store holds `data/x.bin`. `-R-stored/data/x.bin` (validated: one `GetResource` step) reads
the stored bytes as a value; `-R/data/x.bin` (validated: `GetAsset`) loads the keyed asset by the
fast track.

```rust
let store = env.get_async_store();
store.set(&parse_key("data/x.bin")?, &vec![7u8; 1 << 20], &Metadata::new()).await?;
let (stored, _) = store.get(&parse_key("data/x.bin")?).await?;
let raw = envref.evaluate(&parse_query("-R-stored/data/x.bin")?).await?.get().await?;
match raw.value()?.as_ref() {
    Value::Bytes(b) => assert_eq!(b.as_ptr(), stored.as_ptr()),    // AC-7: no copy into the value
    other => panic!("{other:?}"),
}
let asset = envref.evaluate(&parse_query("-R/data/x.bin")?).await?;
let (b1, _) = asset.get_binary().await?;
let (b2, _) = asset.get_binary().await?;
assert_eq!(b1.as_ptr(), stored.as_ptr());                            // AC-8: the store's buffer
assert_eq!(b1.as_ptr(), b2.as_ptr());
```

The fast-track value itself is decoded from `&[u8]` (one copy, out of scope); serving uses the
cached binary, so no copy remains on that path.

## Example 2: Reading an asset twice

```rust
let s1 = asset.get().await?;
let s2 = asset.get().await?;
let (r1, r2) = (s1.metadata.metadata_record().unwrap(), s2.metadata.metadata_record().unwrap());
assert!(Arc::ptr_eq(&r1, &r2));                                      // AC-3
let mut changed = (*s1.metadata).clone();
changed.add_log_entry(LogEntry::info("x".into()))?;                  // copy happens here
assert!(!Arc::ptr_eq(&changed.metadata_record().unwrap(), &r2));     // AC-2
assert_eq!(s2.metadata.metadata_record().unwrap().log.len(), r1.log.len()); // reader unchanged
```

## Edge and Error Cases

- **Type sync on read must not copy.** A value whose type fields already match: `exposed_value_state`
  shares (AC-3). A value installed by a path that bypassed `install_value` would copy on every read;
  the test reads after each install kind (fast track, evaluation, `set_state`).
- **Legacy metadata.** `record_mut` and `metadata_record` return `None`; `add_log_entry` on legacy
  behaves as at HEAD (existing tests in `metadata.rs`).
- **Serialization round trip.** HEAD JSON of a record (with log, dependencies, error data) and of a
  `Value::Bytes(vec![0, 255])` (`[0,255]`) and `SimpleValue::Bytes` tagged form, YAML of each;
  reading the HEAD output back (AC-5).
- **`Bytes` from a non-contiguous OpenDAL buffer.** `to_bytes` copies once (as today); content
  equal (conformance suite with the OpenDAL fs and memory services).
- **wasm.** Web stores compile and pass their Node suite (`--features debug-handles`).

## Test Plan

- Unit: `liquers-core/src/metadata.rs` `mod tests::shared_record` —
  `clone_shares_record` (AC-1), `write_after_clone_copies_and_isolates` (AC-2),
  `equal_type_identifier_does_not_copy` (AC-3), `json_and_yaml_unchanged` (AC-5, against literals
  produced at HEAD).
- Unit: `liquers-core/src/value.rs` — `value_bytes_wire_format_unchanged` (AC-5),
  `value_bytes_clone_shares_buffer` (AC-7 support).
- Unit: `liquers-lib/src/value/simple.rs` — `simple_value_bytes_wire_format_unchanged` (AC-5).
- Unit: `liquers-core/src/store.rs` — `memory_store_get_shares_buffer` (AC-6).
- Integration: new `liquers-core/tests/shared_payloads.rs` (`SimpleEnvironment<Value>`,
  `AsyncMemoryStore`) — `shared01_asset_reads_share_metadata` (AC-3),
  `shared02_resource_value_shares_store_buffer` (AC-7), `shared03_get_binary_shares_cached_buffer`
  (AC-8), `shared04_set_state_shares_value_default_manager`,
  `shared05_set_state_shares_value_inline_manager` (AC-9).
- Unit: `liquers-lib/src/commands.rs` tests — `to_metadata_shares_state_record` (AC-4).
- Existing: store conformance suites (`liquers-core/tests/store_conformance_CONF.rs`, liquers-store's
  OpenDAL suites) — content unchanged (AC-6); `liquers-axum` query and assets route tests — body
  unchanged (AC-8).
- Commands: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`,
  `cargo test -p liquers-lib --lib --tests`, `cargo test -p liquers-store`,
  `cargo test -p liquers-axum`, `cargo test -p liquers-records --lib --tests`, the `liquers-web`
  Node loop, `bash scripts/check-build-matrix.sh`.

## Learning Log

- `opendal::Buffer::to_bytes` is zero-copy for a contiguous buffer (and a single-part
  non-contiguous one); `to_vec` always copies (`opendal 0.55.0` `types/buffer.rs`).
- `bytes::Bytes` compares with `Vec<u8>` and `[u8]` but not with byte-array literals; tests write
  `&b"…"[..]`.
- `From<Bytes> for Vec<u8>` reuses the allocation only for a unique, `Vec`-backed `Bytes`.
