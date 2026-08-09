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
Four `AsyncStore` implementations plus composition:

| Store | `type` | Writes | Notes |
|---|---|---|---|
| `LocalStorageStore` | `localstorage` | yes | full contract: get/set/metadata/listdir/remove/removedir/makedir |
| `FetchStore` | `http` / `https` | no | `url_prefix` + (key − `key_prefix`); read-only; listing from a known-key set |
| `JsStore` | `js` | depends | adapts a JavaScript object implementing the store protocol (`Q1`) |
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
`STORE01`–`STORE07` conformance tests can be written from JavaScript. The same surface, read in
the other direction, is the `JsStore` protocol — a page object providing those methods becomes a
store.

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

## Resolved Questions

**Q1 — scope of `STORE`: both readings, `JsStore` is in.** The guide's `STORE` is a store
*implemented in the language*; the request is Rust stores *exposed to* the language. Decision: do
both. `JsStore` adapts a JavaScript object implementing the store protocol, reusing the Promise
bridge that `ASYNCCMD` already built, so `STORE` is literally conformant rather than reinterpreted.

> **Guide gap, to be fixed separately.** §5 `STORE` asks only about adapting a *language value* to
> `AsyncStore`. It asks nothing about **stores the integration itself provides** (a fetch store, a
> browser-storage store) or about **declarative store configuration and composition** — both of
> which are the larger part of this design, and neither of which any of `STORE01`–`STORE07`
> exercises. `LANGUAGE-INTEGRATION_GUIDE.md` needs design questions and prescribed tests for both.
> Filed as `specs/issues/LANGUAGE-GUIDE-STORE-SCOPE-INCOMPLETE.md`; not fixed here, because
> amending the guide from inside the first design that trips over it would make the design its own
> conformance definition.

**Q3 — localStorage encoding and quota.** `localStorage` holds UTF-16 strings, so the value carries
an encoding tag: text that round-trips as UTF-8 is stored **directly**, anything else **base64**
(+33%, and only paid by binary). Quota is **configurable, unlimited by default** — a byte budget
the store enforces itself, since browsers give no portable way to ask. Exceeding it, or a browser
`QuotaExceededError`, becomes `Error::key_write_error` (`ErrorType::KeyWriteError`), which reads
correctly at the call site and is distinct from `KeyNotSupported`.

**Q4 — directories are real.** `LocalStorageStore` maintains an explicit **directory index**,
following `AsyncMemoryStore` (`liquers-core/src/store.rs:503-506`, `data` + `dir_index`) rather
than deriving directories by scanning key prefixes. So `makedir` creates a directory that survives
with no children, `removedir` is meaningful, and `listdir`/`contains`/`is_dir` stay consistent —
which is exactly what `STORE03` and `STORE04` assert.

**Q5 — `FetchStore` listing comes from a known-key set.** HTTP offers no listing, so the store
holds an explicit set of keys, populated from configuration in this design. Directory structure is
derived from that set, so `listdir` and `contains` are consistent with each other and with what
`get` will actually fetch. *Future:* populating the set by crawling configured web pages — out of
scope here, and the configured set is the fallback that keeps working when it lands.

**Q6 — key shape.** Two separate things, and only the first is `STORE05`:
- **Refusal:** a key with a `..` (or empty) segment parses fine but must not escape a URL prefix or
  a storage namespace. Both stores reject it with `Error::key_not_supported`. That is `STORE05`.
- **Acknowledged limitation:** not every reachable URL is representable as a `Key` — query strings,
  fragments, percent-encoded and reserved characters have no key spelling. Accepted for this
  design; a configurable key→URL mapping is the future escape hatch if a real case demands one.

**Q7 — no environment-variable expansion.** A browser has no environment, so the wasm builder
**does not expand** `${VAR}`; a config containing one is a configuration error rather than a silent
empty string. The syntax is deliberately left unclaimed so that substitution from JavaScript-supplied
variables can use it later. Out of scope here.

## Open Questions

1. **Where the config structs live** — move `StoreRouterConfig`/`StoreConfig` to `liquers-core`
   (recommended; pure serde, zero new dependencies, `liquers-store` re-exports them), or duplicate
   them in `liquers-web`, or extract a third `liquers-store-config` crate.

## References

- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §5 `STORE`, Appendix A `STORE01`–`STORE07`
- `specs/design/liquers-web/` — Phases 1-4 of the parent integration (`STORE` deferred there)
- `specs/reference/STORE_CONFIG_FSD.md` — the configuration format being reused
- `specs/issues/WEB-NATIVE-IO-TIER2.md` — the issue this design closes
- `specs/issues/LANGUAGE-GUIDE-STORE-SCOPE-INCOMPLETE.md` — the guide gap found by `Q1`
- `liquers-core/src/store.rs:268` (`AsyncStore`), `:1770` (`AsyncStoreRouter`)
- `liquers-store/src/config.rs`, `liquers-store/src/store_builder.rs`
