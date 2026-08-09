# Phase 1: High-Level Design - Browser stores for liquers-web (`STORE`)

## Feature Name

`STORE` for `liquers-web` — browser-native stores and declarative store composition

## Purpose

Give a browser page a real `AsyncStore` so `-R/` resource queries, recipes and asset persistence
work without the host page shuttling bytes in by hand. Three stores: **localStorage** (full
read/write/metadata), **fetch** (read-only HTTP under a configured URL prefix), and the existing
**`AsyncStoreRouter`** to combine them from a `liquers_store::config`-shaped declaration.
This is the browser half of `WEB-NATIVE-IO-TIER2`.

## Core Interactions

### Query System
No grammar change. It makes `-R/` *work* in the browser: today `WebEnvironment` is built on
`NoAsyncStore`, so every resource query fails. Keys are consumed as-is; each store strips its own
`key_prefix()` before addressing its backend.

### Store System
Three `AsyncStore` implementations plus composition:

| Store | `type` | Writes | Notes |
|---|---|---|---|
| `LocalStorageStore` | `localstorage` | yes | full contract: get/set/metadata/listdir/remove/removedir/makedir |
| `FetchStore` | `http` / `https` | no | `url_prefix` + (key − `key_prefix`); read-only |
| `AsyncStoreRouter` | — | — | reused unchanged from `liquers-core`; first prefix match wins |

`AsyncStoreRouter` is already `?Send` on wasm (`store.rs:1770`) and needs no change.
`http`/`https` are *already* OpenDAL store types (`config.rs:286`), so one configuration document
means the same thing natively (OpenDAL `services-http`) and in the browser (`fetch`) — the config
is portable, the backend is target-selected.

### Command System
No new commands. Stores are services, not commands; the existing `store`-reading paths use them.

### Asset System
Assets gain a persistent home: `ImmediateAssetManager` can now read recipes and write results
through the store. Volatility and expiration are unaffected.

### Value Types
None. Stores traffic in `Vec<u8>` + `Metadata`.

### Web/API
New `#[wasm_bindgen]` surface on the environment: configure a store from a JS object / YAML / JSON
string, and a small `LiquersStore` wrapper exposing get/set/metadata/listdir as `Promise`s so the
`STORE01`–`STORE07` conformance tests can be written from JavaScript.

### UI
None.

## Crate Placement

- **`liquers-web`** — `src/store/` : `LocalStorageStore`, `FetchStore`, the JS surface, and a
  wasm store builder. Needs `web-sys` features `Storage`, `Request`, `Response`, `Headers`.
- **`liquers-core`** — candidate new home for the *pure* config structs (`StoreRouterConfig`,
  `StoreConfig`), which depend only on `serde`/`serde_json`/`serde_yaml`, all already core
  dependencies. `liquers-store` cannot be a dependency of `liquers-web`: it pulls OpenDAL.
- **`liquers-store`** — re-exports the moved types so nothing downstream breaks. No behaviour change.

**Footprint decision (b): `web-sys` `fetch`, not `reqwest`.** `reqwest`'s wasm backend is itself a
wrapper over `web_sys::fetch` and drags in `http`, `bytes`, `tower-service`, `url` and
`serde_urlencoded` for a store that only issues `GET`. The reuse argument does not apply either:
the *native* read-only HTTP store already exists as OpenDAL's `http` service, so a `reqwest`
version would duplicate it rather than be reused. `web-sys` is already a `liquers-web` dependency,
so the fetch path costs no new crate.

## Open Questions

1. **Scope of `STORE`.** The guide's `STORE` is a store *implemented in the language* (a JS object
   adapted to `AsyncStore`). The request is Rust stores *exposed to* JS. Both are legitimate
   readings of the feature's "Objects/API" list. Recommend: build the three Rust stores now, and
   add a thin `JsStore` adapter (a JS object implementing get/set/... as Promises) in the same
   phase — the Promise bridge from `ASYNCCMD` already exists, so it is cheap, and it is what makes
   `STORE` literally conformant rather than reinterpreted.
2. **Where the config structs live** — move to `liquers-core` (recommended), or duplicate in
   `liquers-web`, or extract a third `liquers-store-config` crate.
3. **localStorage encoding and quota.** Values are UTF-16 strings, so bytes need encoding
   (base64 — +33% against a ~5 MB budget — vs. a binary-string codec). What error does
   `QuotaExceededError` become?
4. **Directory model in localStorage.** Derive directories by scanning key prefixes, or maintain an
   explicit index entry? Do empty directories (`makedir`) survive?
5. **Directory listing over `fetch`.** HTTP has no listing. Empty list, an optional index document
   per folder, or a configured manifest?
6. **`STORE05` — which key shape does each store refuse?** Candidate: any key containing a `..`
   segment (parses fine, must not escape a URL prefix or a storage namespace).
7. **`expand_env_vars` on wasm.** `std::env::var` is meaningless in a browser; substitute from a
   configured map, or skip expansion?

## References

- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §5 `STORE`, Appendix A `STORE01`–`STORE07`
- `specs/design/liquers-web/` — Phases 1-4 of the parent integration (`STORE` deferred there)
- `specs/reference/STORE_CONFIG_FSD.md` — the configuration format being reused
- `specs/issues/WEB-NATIVE-IO-TIER2.md` — the issue this design closes
- `liquers-core/src/store.rs:268` (`AsyncStore`), `:1770` (`AsyncStoreRouter`)
- `liquers-store/src/config.rs`, `liquers-store/src/store_builder.rs`
