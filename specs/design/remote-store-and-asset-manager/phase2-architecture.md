# Phase 2: Solution & Architecture — a server's store and asset manager as a client's own

## Overview

The feature has three parts with different readiness, so it is split:

| Slice | What | Rides on | State |
|---|---|---|---|
| 1 | `ApiStore`: a native `AsyncStore` over the Store API (`StoreApiBuilder` routes, `WEB_API_SPECIFICATION.md` §4) | existing user API | designed here, Phases 3-4 |
| 2 | Browser `AsyncStore` over the same routes, with `fetch` | slice 1's mapping | `WEB-HAS-NO-READ-WRITE-REMOTE-STORE` |
| 3 | Remote asset manager + trusted system API (whole entries, versions, dependency records, expiration; WebSocket status) | peer identity, authority rules | `REMOTE-ASSET-MANAGER-AND-SYSTEM-API` (blocked on authority and identity) |

Slice 1 needs no new server route, no change to core traits and no answer to the authority
questions: the server's store stays authoritative for every key it serves, exactly as a mounted
S3 bucket is.

Rejected for slice 1: OpenDAL's `http`/`webdav` services (no Liquers metadata, read-only `http`);
building on the Assets API's `key/` routes (manager-owned metadata, and `axum-assets-endpoints`
deliberately refuses client-supplied manager fields); a client in `liquers-store` (cannot use
`liquers-axum`'s wire types: the dependency runs `liquers-axum` → `liquers-store`).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `CORE-SESSION-AND-KEY-ACL` | accepted | P2 | Slice 1 sends static headers; slice 3 needs peer identity | no (slice 1) | slice 3 only | `requires` |
| `STORE-WRITE-HAS-NO-PRECONDITION` | draft | P2 | `set_if` via trait default (check-then-write over HTTP) | no | no | note |
| `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` | draft | P2 | whole bodies | no | no | note |
| `HTTP-REQUEST-BODY-LIMIT` (design `http-request-body-limit`) | — | — | a large `PUT entry` meets the server's limit; surfaced as the server's error | no | no | note |

## Interfaces

```rust
// liquers-axum/src/client/mod.rs   (feature "client"; native only, like the crate)
pub struct ApiStoreConfig {
    pub url: String,                       // base of the Store API, normalized to end in '/'
    pub headers: BTreeMap<String, String>, // sent with every request
    pub timeout: Option<std::time::Duration>,
}

pub struct ApiStore {                       // owns a reqwest::Client (Arc inside; cheap clone)
    prefix: Key,
    config: ApiStoreConfig,
    client: reqwest::Client,
}

impl ApiStore {
    pub fn new(prefix: Key, config: ApiStoreConfig) -> Result<Self, Error>;
}

#[async_trait]
impl AsyncStore for ApiStore { … }         // mapping below

/// A factory resolving `type: liquers_api`; chain it before `default_store_factory()`.
pub fn api_store_factory() -> StoreTypeMap;
```

Method → route (key = the local key with `prefix` removed, percent-encoded per segment):

| `AsyncStore` | Route | Notes |
|---|---|---|
| `get` | `GET entry/{key}?_entry_format=cbor` | decode `DataEntry`; `Metadata::from_json_value(entry.metadata)` |
| `get_bytes` | `GET data/{key}` | |
| `get_metadata` | `GET metadata/{key}` | |
| `set` | `PUT entry/{key}?_entry_format=cbor` | body `DataEntry { metadata: record JSON, data }` |
| `set_metadata` | `PUT metadata/{key}` | JSON |
| `remove` / `removedir` / `makedir` | `DELETE entry/…` / `DELETE removedir/…` / `PUT makedir/…` | |
| `contains` / `is_dir` / `listdir` / `keys` | `GET contains/…` / `is_dir/…` / `listdir/…` / `keys?prefix=` | `ApiResponse` envelope, `result` field; `keys` results get `prefix` re-added |
| `is_supported` | none | `key.has_key_prefix(&prefix)` (sync, as `FetchStore`) |
| data-only methods, `data_capabilities` | none | trait defaults: refused / none |
| `store_name` | — | `"liquers_api {url}"` |

Async: every method awaits one HTTP request (two for none), no lock is held across an `.await`.
`reqwest` on native is `Send`, so the type is `Send + Sync` as `AsyncStore` requires natively.

Factory: `StoreTypeInfo::new("liquers_api")` with arguments `url` (required), `headers` (map,
optional), `timeout_ms` (optional); constructor builds `ApiStore`, refusing a missing `url` with
`Error::configuration_error`. A `metadata`
section is refused by the router builder already, because the store reports no data-only access
(`STORE_IMPLEMENTATION_GUIDE.md` §4a) — AC-5 checks that this holds.

## Integration Points

- `liquers-axum/Cargo.toml`: `[features] client = ["dep:reqwest"]`; `reqwest = { version = "0.12",
  optional = true, default-features = false, features = ["rustls-tls"] }` — the version already in
  `Cargo.lock` (via OpenDAL and the dev-dependency), so no new crate is resolved; dev-dependency
  `liquers-core` with `store-conformance`.
- `liquers-axum/src/lib.rs`: `#[cfg(feature = "client")] pub mod client;`.
- Wire types reused from `liquers-axum/src/api_core/response.rs`: `DataEntry`, `ErrorDetail`,
  `ApiResponse`; CBOR via the existing `ciborium`.
- No change to `liquers-core`, `liquers-store`, the server handlers or the bindings.

## Error Handling

- Error body present: `ErrorDetail.error_type` deserialized as `ErrorType` (variant names), mapped
  to the typed constructor for that type (`Error::key_not_found(&key)` for `KeyNotFound`, …) with
  the server message kept; an unknown type string → `ErrorType::General` via
  `Error::general_error` naming the original (AC-3).
- Transport failure, or a non-2xx response without a decodable body: `KeyReadError` /
  `KeyWriteError` through `Error::key_read_error` / `Error::key_write_error` naming the URL (AC-4).
- Undecodable success body: `Error::from_error(ErrorType::SerializationError, e)`.
- No `unwrap`/`expect`; no `Error::new`.

## Relevant Commands

None. `ns-store` commands work on the mount unchanged.

## Documentation Architecture

- `specs/reference/WEB_API_SPECIFICATION.md` (reference, internal, `axum`): new §"Client" (store
  type, mapping table, what is not forwarded); Appendix B updated. History, `reviewed:`.
- `specs/reference/STORE_CONFIG_FSD.md` (`store/config`): the `liquers_api` type and arguments.
- `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` §5: the new suite in the list of suites (guide row).
- `affects_docs`: the two references.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-axum/Cargo.toml`, `src/lib.rs`, new `src/client/mod.rs`, new `tests/remote_store_conformance.rs` |
| Crates and workflows affected | `liquers-axum` with `client` only; default build unchanged |
| Existing tests likely to change | none |
| New validation | conformance suite in-process; mapping and error tests; `check-build-matrix.sh` (new feature) |
| Compatibility / data / concurrency / performance / security | headers may carry a secret: never logged, not in `store_name`; one HTTP round trip per call (no batching); TLS via rustls avoids system OpenSSL |
| Recovery | feature-gated: dropping the feature removes everything |
| Certainty and open questions | High for slice 1; questions 1-2 are placement and auth proposals |
