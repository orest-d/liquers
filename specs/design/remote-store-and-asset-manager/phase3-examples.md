# Phase 3: Examples & Use-cases — slice 1, the remote store

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Mounting a server folder | configuration and a query over the mount | AC-1, AC-6 |
| 2 | Example | Building the store in code | `ApiStore::new` with headers | AC-1 |
| 3 | Test | `liquers-axum/tests/remote_store_conformance.rs` `remote_store_passes_conformance` | the suite against an in-process server | AC-2 |
| 4 | Test | `…` `set_then_get_round_trips_data_and_metadata` | server store holds what the client wrote | AC-1 |
| 5 | Test | `…` `server_errors_keep_their_type` | `KeyNotFound`, `NotSupported` (read-only server mount) | AC-3 |
| 6 | Test | `…` `unreachable_server_is_read_or_write_error` | closed port | AC-4 |
| 7 | Test | `…` `factory_refuses_missing_url_and_metadata_section` | router build errors | AC-5 |
| 8 | Test | `…` `keys_map_under_the_mount_prefix` | URL shape, `KeyNotSupported` outside the prefix | AC-6 |

## Example 1: mounting a server folder

The configuration in Phase 1's problem example, built through the client's factory chain:

```rust
let factory = ChainedStoreFactory::new()
    .chain(Box::new(liquers_axum::client::api_store_factory()))
    .chain(Box::new(liquers_store::store_factory::default_store_factory()));
let router = StoreRouterBuilder::from_yaml(CONFIG_YAML, Box::new(factory))?.build()?;
```

Evaluating `-R/shared/sales.csv/-/ns-pl/head-10/-/to_text` (validated) in an environment using that
router issues `GET {url}entry/sales.csv?_entry_format=cbor` once, then runs `head` and `to_text`
locally. `ns-store` listing of `shared` issues `GET {url}listdir/`.

## Example 2: building the store in code

```rust
let store = ApiStore::new(parse_key("shared")?, ApiStoreConfig {
    url: format!("http://{addr}/liquer/api/store"),     // normalized to end in '/'
    headers: BTreeMap::from([("Authorization".into(), format!("Bearer {token}"))]),
    timeout: Some(Duration::from_secs(30)),
})?;
store.set(&parse_key("shared/a.txt")?, b"hello", &Metadata::new()).await?;
```

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| server answers 404 with `{"error":{"type":"KeyNotFound",…}}` | `ErrorType::KeyNotFound`, message names the key | 5 |
| server mount is `read_only: true` | `set` → `ErrorType::NotSupported` (server's 501 body) | 5 |
| nothing listens on the port | `get` → `KeyReadError`, `set` → `KeyWriteError`, URL in message | 6 |
| `url` missing | router build fails, `ConfigurationError`, names the entry | 7 |
| `metadata:` section on the mount | router build fails: no data-only access | 7 |
| key `other/x` on a store mounted at `shared` | `KeyNotSupported` before any request | 8 |
| key with a space or `~` segment | percent-encoded per segment; round-trips | 4 |

## Test Plan

File `liquers-axum/tests/remote_store_conformance.rs`, `#![cfg(feature = "client")]`, every test
`#[tokio::test]`. A helper `start_store_server() -> (String, Arc<dyn AsyncStore>)` builds a
`SimpleEnvironment<Value>` with an `AsyncMemoryStore`, mounts
`StoreApiBuilder::new("/liquer/api/store").build()`, binds `127.0.0.1:0` and spawns `axum::serve`
(as `assets_websocket.rs` `start_server` does), returning the base URL and the server's store so
tests can inspect what the client wrote.

- Integration — `remote_store_passes_conformance` (AC-2): `run_all` on a `GenericFixture` of the
  `ApiStore` with `SafetyLevel::Scratch` and capabilities `write, remove, directories,
  derived_directories, explicit_directories, remove_directories, stored_metadata, enumerate_keys`
  all `true` (the server's memory store has them all); `assert_conformant(&[])`. A rule that fails
  only because the API cannot express it is listed in `AllowedFailure` with the reason, and filed.
- Integration — `set_then_get_round_trips_data_and_metadata` (AC-1): client `set` with a title;
  the server store's `get` returns the bytes and title; client `get` returns the same.
- Integration — `server_errors_keep_their_type` (AC-3): absent key; a second server whose store is
  wrapped read-only (`ReadOnlyStore`).
- Integration — `unreachable_server_is_read_or_write_error` (AC-4): bind a listener, read its port,
  drop it, point the store there.
- Integration — `factory_refuses_missing_url_and_metadata_section` (AC-5): two YAML documents.
- Integration — `keys_map_under_the_mount_prefix` (AC-6): store mounted at `shared`, server at
  root; client writes `shared/a/b.txt`, the server store holds `a/b.txt`; `other/x` refused.
- Command: `cargo test -p liquers-axum --features client --test remote_store_conformance`.

## Learning Log

- `reqwest` 0.12 is already resolved in `Cargo.lock`, so the feature adds no crate to the lock.
- The Store API already answers every call slice 1 needs; the only route the trait has that the
  API lacks is data-only access, deliberately not forwarded.
