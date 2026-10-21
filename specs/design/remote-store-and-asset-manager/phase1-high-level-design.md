# Phase 1: High-Level Design — a server's store and asset manager as a client's own

## Purpose

Serve the *client/server* shape of `VISION.md`: a client environment (a CLI, a desktop app, later a
browser page) mounts a Liquers server's store as one of its own stores, and later delegates
evaluation to the server's asset manager, instead of either duplicating the server's data or being
a thin viewer over REST.

## Problem Example

A desktop tool wants the team server's `shared/` folder as part of its own key space:

```yaml
# the client's store configuration (ENVIRONMENT_CONFIG.md / STORE_CONFIG_FSD.md)
stores:
  - type: liquers_api
    prefix: shared
    config:
      url: "https://data.example.org/liquer/api/store/"
      headers: { Authorization: "Bearer ${LIQUERS_TOKEN}" }
```

```
-R/shared/sales.csv/-/ns-pl/head-10/-/to_text
```

(validated with `liquers-validate`: `GetResource[shared, sales.csv]`, then two actions.)

- **Today:** no store type speaks the Store API, so the configuration is refused ("unknown store
  type"); the only HTTP store is OpenDAL's read-only `http` service, or `liquers-web`'s read-only
  `FetchStore` with a fixed key list. Neither lists, writes, removes or reads the server's metadata.
- **Should:** the query reads `shared/sales.csv` from the server through `GET entry/sales.csv`,
  writes go to `PUT entry/…`, listing to `GET listdir/…`, and server errors keep their type.

## Scope and Acceptance Criteria

Slice 1 (designed through Phase 4):

- **AC-1** Read and write through the server
  WHEN a client `set`s a key on a `liquers_api` store and then `get`s it
  THEN the server's store holds the data and metadata, and `get` returns them
- **AC-2** The store passes the conformance suite
  WHEN the suite runs against a `liquers_api` store whose server serves a memory store
  THEN every rule its declared `StoreCapabilities` select passes
- **AC-3** Errors keep their type
  WHEN the server answers with an error body (`KeyNotFound`, `NotSupported`, …)
  THEN the client returns an `Error` of the same `ErrorType` whose message names the key
- **AC-4** An unreachable server is a read or write error
  WHEN the server cannot be reached or answers without an error body
  THEN a read returns `KeyReadError`, a write `KeyWriteError`, naming the URL
- **AC-5** Configuration is checked when the store is built
  WHEN a `liquers_api` entry has no `url`, or a `metadata` section
  THEN building the router fails with a configuration error naming the entry
- **AC-6** Keys map under the mount prefix
  WHEN the store is mounted at `shared` and asked for `shared/a/b.txt`
  THEN it requests `{url}entry/a/b.txt`, and a key outside `shared` is `KeyNotSupported`

Later slices (follow-up issues, not planned here): a client resolves a query through the server's
asset manager, seeing its status and progress (`REMOTE-ASSET-MANAGER-AND-SYSTEM-API`); a browser
page mounts a server's store read/write (`WEB-HAS-NO-READ-WRITE-REMOTE-STORE`).

Non-goals (slice 1): caching remote data locally; data-only access (the Store API applies the
server's own metadata handling); conditional writes (`store-conditional-write` adds `set_if`, which
needs an API route); streaming large values (`VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE`).

## Core Interactions

Store (a new `AsyncStore` and its factory); web/API (a client of the Store API); assets only in the
later slice.

## Crate Placement

`liquers-axum`, behind a new optional feature `client`: the wire types it needs (`DataEntry`,
`ErrorDetail`, `ApiResponse`) already live there, `reqwest` is already its dev-dependency, and
`liquers-store` cannot depend on `liquers-axum` (the edge runs the other way). See question 1.

## Documentation Intent

- Reference: extend `WEB_API_SPECIFICATION.md` (Appendix B loses "a remote store"; a short "Client"
  section) and `STORE_CONFIG_FSD.md` (store type `liquers_api`).
- Guide: none; `STORE_IMPLEMENTATION_GUIDE.md` §5 lists the new conformance suite in its table.
- Other documents: `liquers-axum` crate docs.
- Documents to update: the three above.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — `XL` source; a new `pub` store type, feature and dependency
  (rules 1, 3, 4).
- **Leading issue:** where the client lives (question 1).
- **Explanation:** slice 1 maps every `AsyncStore` method onto an existing Store API route and is
  provable with the conformance suite in-process. The remote asset manager is not designed here:
  its authority and conflict rules are open and it needs peer identity first.
- **Open questions:**
  1. **Open design - placement:** a `client` feature of `liquers-axum` (recommended: reuses the
     wire types, no new crate, tests next to the server) or a new `liquers-client` crate (cleaner
     name, but the wire types would have to move to it or to core).
  2. **Open design - authentication:** static `headers` with `${VAR}` expansion now (recommended),
     replaced by the session mechanism of `access-policy-at-entry-points` when it lands; or wait.
  3. **Proposed resolution - store type name:** `liquers_api` (no clash with OpenDAL's `http`).
  4. **Blocking for the later slice, not this one - authority:** for the remote asset manager,
     which side owns a key (the issue's open questions: authority, conflicts, local caching,
     split evaluation, reconnection). Recorded in `REMOTE-ASSET-MANAGER-AND-SYSTEM-API`.

## Design Dependencies

- `access-policy-at-entry-points` (`CORE-SESSION-AND-KEY-ACL`) — **requires**: peer identity for
  the system API, and the replacement for slice 1's static headers (that design records this one
  as `required-by`).
- `store-conditional-write` (`STORE-WRITE-HAS-NO-PRECONDITION`) — **overlaps**: a remote store
  must forward `set_if` once a route carries it; slice 1 inherits the trait default.
- `multi-realm-dispatch` (`CORE-MULTI-REALM-INTERPRETER`) — **required-by**: delegating a realm's
  query prefix is evaluation on a remote asset manager.
- `UNMANAGED-ASSET-NOT-FOLLOWABLE-ACROSS-CONNECTIONS` (issue) — **overlaps**: a remote asset
  manager following an unmanaged server asset needs its server-wide id.
- `environment-orderly-shutdown` (`ENVIRONMENT-ORDERLY-SHUTDOWN`) — **overlaps**: an HTTP client and
  a WebSocket are services needing asynchronous close.
- `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` (issue) — **overlaps**: whole-body transfer.
- `axum-assets-endpoints` — origin of the user API / system API split; not extended.

## Consolidated Findings

- Every `AsyncStore` method of slice 1 has a route (`WEB_API_SPECIFICATION.md` §4); `get` and `set`
  use `entry` so data and metadata travel together, in CBOR (the API's default).
- The remote store reports **no** data-only access: the server already applies its own metadata
  handling, and a second, client-side `metadata` section would double it. AC-5 makes that explicit.
- `is_supported` is synchronous, so it decides by the mount prefix only, as `FetchStore` does.
- `ErrorType` derives `Deserialize`, and the server's `error.type` is the variant name, so error
  types round-trip without the server-side `parse_error_type`.
- The remote asset manager remains blocked on authority and identity; slice 1 does not
  depend on any of its answers.
- Queries in this design were validated with `liquers-validate`.
