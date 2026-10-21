# Phase 1: High-Level Design — Configuring a store from a URI

## Feature Name

Configuring a store from a URI (`uri:` on a store entry).

## Purpose

Let a store entry be written as one URI — `s3://my-bucket/datasets?region=eu-central-1` — instead of
a `type` plus a `config` map, for the cases where a URI is the natural unit: a command-line flag, a
single-store quick start, an environment variable. Serves `VISION.md` priority area 3 (more storage
backends through the store factory model) by making the backends OpenDAL already provides reachable
in the form people copy from cloud tooling.

## Problem Example

```yaml
stores:
  - uri: s3://my-bucket/datasets?region=eu-central-1
    prefix: remote
```

Today: `StoreConfig` (`liquers-core/src/store_config.rs`) has no `uri` field, and it is not
`deny_unknown_fields`, so the key is **silently dropped**; the entry then has an empty `type`, no
factory resolves it, and the build fails with "A store entry has no type" (`unknown_store_type_error`). Should be: the entry builds
the same store as

```yaml
  - type: s3
    prefix: remote
    config: { bucket: my-bucket, root: datasets, region: eu-central-1 }
```

The capability exists one layer down: OpenDAL 0.55's `OperatorUri::new` plus a service
`Configurator::from_uri` build an identical operator (`s3_01_arguments_and_uri_agree`,
`liquers-store/src/store_factory.rs`, already proves the equality through `Operator::from_uri`).

## Scope and Acceptance Criteria

- **AC-1** Equivalent spellings
  WHEN an entry has `uri: s3://probe-bucket/data?region=eu-central-1` and a prefix
  THEN it builds a store with the same prefix, bucket name and root as the equivalent `type: s3`
  entry with a `config:` map
- **AC-2** Exactly one of `type` and `uri`
  WHEN an entry has both `type` and `uri`, or neither
  THEN the build fails with a configuration error naming the entry's prefix and the two fields
- **AC-3** Resolution through the factory chain
  WHEN two factories in a chain both declare a scheme
  THEN the first in the chain builds the entry, exactly as for a `type:` entry, and OpenDAL's own
  operator registry is never consulted
- **AC-4** Unclaimed scheme
  WHEN no factory in the chain resolves the URI's scheme
  THEN the build fails with an error naming the scheme and listing the schemes the chain declares
- **AC-5** Core types have schemes
  WHEN an entry has `uri: file:///srv/data` or `uri: memory://`
  THEN core's `filesystem` (rooted at `/srv/data`) or `memory` store is built; `fs://` still means
  OpenDAL's `fs` service
- **AC-6** Variables and secrets
  WHEN an entry has `uri: s3://${BUCKET}/data?region=${REGION}` with `REGION` set to a value
  containing `&` or `=`, and `config: { secret_access_key: ${SECRET} }`
  THEN the substituted values reach the service unchanged, and the `config:` values are applied as
  well, overriding a query parameter of the same name
- **AC-7** Service without a URI form
  WHEN the URI names an OpenDAL service whose configuration cannot be read from a URI
  THEN the build fails with a not-supported error that tells the user to write `type` + `config`

Non-goals: replacing `type` + `config` (still the primary form; S3 has 26 arguments); exposing
`Operator::from_uri`; a URI for the whole router or for `prefix`; schemes for the browser store
types (`localstorage://`), which `liquers-web` can declare later without a design change;
credentials inside a URI as a recommended practice.

## Core Interactions

- **Store configuration:** `StoreConfig` gains `uri`; `StoreTypeInfo` gains `uri_schemes`; the
  router builder checks exclusivity and expands variables in the URI.
- **Store factories:** the default `StoreFactory::resolve` and `StoreTypeMap::resolve` also match a
  URI's scheme against declared schemes; `OpendalStoreFactory` resolves and builds from a URI;
  `ChainedStoreFactory` reports an unclaimed scheme.
- **Store backends:** core's `filesystem` constructor reads its root from a `file://` URI.
- Query, commands, assets, value types, bindings: untouched (`liquers-web` consumes
  `StoreRouterConfig` and gains the field without change).

## Crate Placement

`liquers-core` (`store_config.rs`, `store_factory.rs`) for the field, the scheme declaration, the
default resolution, exclusivity, variable expansion and the two core schemes: configuration and the
factory model live in core. `liquers-store` (`store_factory.rs`) for the OpenDAL half: interpreting a
URI is backend knowledge — the authority means a bucket for `s3`, a host for `ftp`, nothing for
`fs` — so it belongs in the factory that owns the backend.

## Documentation Intent

- Reference: extend `specs/reference/STORE_CONFIG_FSD.md` — the `uri` field, the exclusivity rule,
  the scheme table (generated from `uri_schemes`), variable expansion in a URI.
- Guide: extend `specs/guides/STORE_CONFIG_GUIDE.md` with a short "one store from a URI" task that
  keeps secrets in `config:`, and `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` with "declare a URI
  scheme" for a factory author.
- Other documents: none.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — a new public field on two structs, a new configuration form
  and a change to the default trait method (rules 3 and 4).
- **Leading issue:** **Open design - variables in a URI:** where `${VAR}` is expanded.
- **Explanation:** the 2026-08-29 audit's one hard requirement — `StoreFactory::resolve(&StoreConfig)`
  in place of `claims(&str)` — has landed with `store-factories-in-core`, as has `#[serde(default)]`
  on `store_type`. What remains is additive. Re-reading OpenDAL showed that the earlier
  recommendation for variables (expand per value after parsing) cannot be implemented without
  re-implementing every service's `from_uri`, so it is re-opened. Phases 3-4 assume the
  recommendations below.
- **Open questions:** six, listed under Open Questions below.

## Design Dependencies

- `store-factories-in-core` (complete) — **requires**, satisfied: `resolve`, `ChainedStoreFactory`
  first-wins and the defaultable `store_type` are at HEAD.
- `store-conformance-check-tool` — **overlaps**, weak: its `--store` picks a mount by name today; a
  later `--store s3://…` would use this form.
- `store-conditional-write`, `metadata-application-attributes` — no overlap.

## Open Questions

1. **Open design - variables in a URI:** expand `${VAR}` on the URI string, percent-encoding each
  substituted value (recommended: `OperatorUri::new` percent-decodes the path and query, so any
  value round-trips, and expansion stays in the builder), or forbid `${VAR}` in `uri` and require
  `config:` for anything variable.
  2. **Open design - `config:` beside `uri`:** allowed, applied as OpenDAL's extra options that
  override a query parameter of the same name (recommended: it is the documented home for
  secrets and for arguments too long for a query string), or forbidden.
  3. **Proposed resolution - filesystem scheme:** core's `filesystem` declares `file://`, OpenDAL's
  `fs` keeps `fs://`; `memory://` is core's `memory` by chain order.
  4. **Proposed resolution - `type` with `uri`:** mutually exclusive (the scheme is the type); can be
  relaxed later additively if a URI ever needs disambiguating.
  5. **Proposed resolution - dispatch:** `uri_schemes` feeds the default `resolve` (one mechanism,
  overridable), instead of being documentation only as the 2026-08-29 draft had it.
  6. **Open design - parsing in core:** a narrow hand-written `scheme://authority/path?query` parser
  with percent-decoding for the two core schemes (recommended: core stays dependency-light), or
  the `url` crate, already in `Cargo.lock` through OpenDAL.
