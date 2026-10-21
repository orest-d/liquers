# Phase 2: Solution & Architecture — Configuring a store from a URI

## Overview

A URI entry is an ordinary entry whose store type is **resolved** rather than written. Five steps,
all inside machinery that exists at HEAD:

1. The builder expands `${VAR}` in `uri` (percent-encoding the substituted values) and checks that
   exactly one of `type` and `uri` is present.
2. `ChainedStoreFactory` asks each factory in order to `resolve` the entry. The default
   implementation, and `StoreTypeMap`'s, match the URI's scheme against the `uri_schemes` each
   declared `StoreTypeInfo` carries; `OpendalStoreFactory` overrides `resolve` with the same rule over
   its table.
3. First `Some` wins; the chain writes the resolved name into `store_type` of a clone (already the
   case for any resolution, `ChainedStoreFactory::create`).
4. `create` runs. A core constructor reads what it needs from the URI; `OpendalStoreFactory` builds
   `OperatorUri::new(uri, config pairs)` and hands it to the service's `Configurator::from_uri`.
5. No factory resolves → an unclaimed-scheme error listing the declared schemes.

Rejected, one line each (the 2026-08-29 audit, unchanged where still true):
- **`Operator::from_uri`**: bypasses the chain (a browser build could not override `http`) and its
  registry knows 10 services; per-service `Configurator::from_uri` reaches 61 of 62.
- **A central normalizer that rewrites a URI into `config:` keys**: the authority means a bucket
  (`s3`, `gcs`), a host (`ftp`, with a synthesized scheme), or nothing (`fs`); that is per-backend
  knowledge.
- **A per-entry interpreter** (`schemes: opendal`): the maintainer chose the unified form.
- **Expanding variables per value after parsing** (the 2026-08-29 recommendation): parsing happens
  inside OpenDAL's `OperatorUri`, and `from_uri` consumes the parsed URI; the values are never handed
  back to Liquers between the two. Q1.
- **`filesystem` claiming `fs://`**: would silently shadow OpenDAL's `fs`, which behaves
  differently; `file://` is free. Q3.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `STORE-OPENDAL-SERVICES-NOT-ENABLED` | closed | P0 | a URI naming a compiled-out service fails as the `type:` form does | no | no | none |
| `STORE-OPENDAL-LIST-OPTION-MISPARSED` | closed | P2 | a URI query carries `a,b` lists natively | no | no | none |
| `STORE-OPENDAL-SLASH-HANDLING` | see index | — | why `fs://` must not be shadowed | no | no | Q3 |

## Data Structures

`liquers-core/src/store_config.rs`:

```rust
pub struct StoreConfig {
    #[serde(rename = "type", default)] pub store_type: String,   // unchanged
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,                                     // NEW
    /* prefix, config, read_only, metadata: unchanged */
}
impl StoreConfig {
    pub fn with_uri(self, uri: &str) -> Self;                    // NEW, test and builder helper
    /// The scheme of `uri`, lower-cased; `None` without a URI or a `scheme://`.
    pub fn uri_scheme(&self) -> Option<String>;                  // NEW
}
/// A parsed URI for factories that do not use OpenDAL (Q6).
pub struct StoreUri { pub scheme: String, pub authority: Option<String>,
                      pub path: String, pub options: Vec<(String, String)> }
impl StoreUri { pub fn parse(uri: &str) -> Result<StoreUri, Error>; }
```

`StoreConfig::expand_env_vars` also expands `uri`, through a new private
`expand_env_vars_uri_encoded`, which substitutes each `${VAR}` with its value percent-encoded
(everything outside RFC 3986 unreserved characters).

`liquers-core/src/store_factory.rs`:

```rust
pub struct StoreTypeInfo { /* … */ #[serde(default)] pub uri_schemes: Vec<String> }  // NEW
impl StoreTypeInfo { pub fn with_uri_scheme(self, scheme: &str) -> Self; }           // NEW
```

## Trait Implementations

- `StoreFactory::resolve` default: exact `store_type` match as today; when `store_type` is empty and
  `uri_scheme()` is `Some(s)`, the first declared type whose `uri_schemes` contains `s`.
  `StoreTypeMap::resolve` the same over its map.
- `core_store_factory`: `memory` declares `memory`, `filesystem` declares `file`. The filesystem
  constructor takes `path` from `config` or, with a URI, from `StoreUri::parse(uri)?.path`
  (authority empty or `localhost`; an absolute path).
## Function Signatures

- `ChainedStoreFactory::create`, no resolution: with a URI and an empty type,
  `unknown_uri_scheme_error(scheme, &self.store_types())`.
- `StoreRouterBuilder::build_router`: before `factory.create`, `check_type_or_uri(store_config)`.

`liquers-store/src/store_factory.rs`:

- `OpendalStoreFactory::type_info` declares `uri_schemes` from a new `service_uri_scheme(store_type)`
  (the OpenDAL scheme of the type: `s3` → `s3`, `fs` → `fs`, `http` → `http`, `https` → `https`).
  `resolve` adds the URI rule.
- `create` with a URI: `resolve_enabled_scheme` as today, then `operator_from_uri(store_type,
  OperatorUri::new(uri, config_as_string_map()?))`, a match with one cfg-gated arm per service,
  next to `service_arguments`:
  `"s3" => Operator::from_config(S3Config::from_uri(&uri)?)?.finish()`. A service whose
  `from_uri` returns `Unsupported` → AC-7.

## Sync vs Async

Sync throughout, like the rest of store construction: parsing is not I/O and OpenDAL builders do
not touch the network. No `unwrap`, no `Error::new`, no default arm over an enum.

## Integration Points

- `StoreRouterConfig` documents (YAML/JSON/TOML) gain the optional key; nothing else reads it.
- `liquers-web` builds through `StoreRouterBuilder`, so a browser factory may later declare
  `localstorage` in its `StoreTypeInfo` with no further change.
- `store-conformance-check-tool` may accept a URI for `--store` later.

## Error Handling

| Failure | Constructor | Message names |
|---|---|---|
| both `type` and `uri`, or neither | `Error::configuration_error` | the prefix and both fields |
| `uri` without `scheme://` | `Error::configuration_error` | the URI with its query removed |
| unclaimed scheme | `Error::not_supported` | the scheme and the declared schemes |
| service without a URI form | `Error::not_supported` | the type and "write `type` + `config`" |
| OpenDAL parse or config error | `Error::configuration_error` | the type; never the query string |
| `file://` with a host or a relative path | `Error::configuration_error` | the URI |

A message never repeats the query string, which may carry a credential.

## Relevant Commands

None. No `store_config` realm entry and no command signature changes; `specs/command_registry.yaml`
unchanged.

## Documentation Architecture

- `specs/reference/STORE_CONFIG_FSD.md`: "Store entries" gains `uri`; a "URI schemes" table; the
  variable rule.
- `specs/guides/STORE_CONFIG_GUIDE.md`: a task "one store from a URI", secrets in `config:`.
- `specs/guides/STORE_IMPLEMENTATION_GUIDE.md`: "declare a URI scheme" for a factory author.
- `affects_docs` as in `DESIGN.md`; no `specs/README.md` change.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/store_config.rs`, `liquers-core/src/store_factory.rs`, `liquers-store/src/store_factory.rs` |
| Crates and workflows affected | `liquers-core`, `liquers-store`; every consumer of `StoreRouterConfig`, additively |
| Existing tests likely to change | none expected: both fields default and skip when empty; `resolve01`–`resolve04` keep their meaning |
| New validation | AC-1 to AC-7 as unit tests, offline (OpenDAL builders do not touch the network) |
| Compatibility / data / concurrency / performance / security | additive; no persisted state; one parse per entry at startup; **security**: a URI invites a credential in one string that is logged and pasted — messages omit the query, the guide keeps secrets in `config:` |
| Recovery | revert two fields and the resolution rule |
| Certainty and open questions | high: `OperatorUri::new` (percent-decoding, extra options override) and `Configurator::from_uri` read in `opendal-0.55.0`; Q1, Q2, Q6 open |
