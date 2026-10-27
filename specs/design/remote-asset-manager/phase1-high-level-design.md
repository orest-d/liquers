# Phase 1: High-Level Design — delegating evaluation of a key prefix to a server's asset manager

## Purpose

Serve the *client/server* shape of `VISION.md` one step past the remote store: a client
environment (CLI, desktop app, later a browser page) treats a key prefix as *the server's*, so
asking for `shared/report.html` makes the **server** evaluate it, with its status, progress and
expiration visible locally as an ordinary `AssetRef`, instead of either downloading every input and
recomputing locally or being a thin REST viewer.

## Problem Example

The team server holds `shared/sales.csv` and a recipe `shared/report.html` that needs Polars and
an hour of CPU. A desktop client mounts the server's store at `shared` (slice 1 of
`remote-store-and-asset-manager`, `liquers_api` store type) and asks for the report:

```rust
let asset = client_env.get_asset_manager().get(&parse_key("shared/report.html")?).await?;
let mut rx = asset.subscribe_to_notifications().await;   // progress, then Ready
```

- **Today (HEAD `cd6d5da`):** the client has only local managers (`DefaultAssetManager`,
  `ImmediateAssetManager`, `liquers-core/src/assets.rs`). With slice 1 landed, the client's store
  serves `shared/report.html` if the server has *stored* it; otherwise the client's own recipe
  provider (reading `shared/recipes.yaml` through the remote store) makes the client evaluate the
  recipe locally — downloading `shared/sales.csv`, needing Polars, and writing the result back to
  the server's store as a `Source`/`Override` written by a client, not as a server-computed `Ready`.
  The server's progress, expiration and dependency records of the report are invisible.
- **Should:** the client asks the server's asset manager; the local `AssetRef` mirrors the server's
  status and progress (`Submitted` … `Processing` 40 % … `Ready`), `get()` then returns the value,
  and when the server expires the report (a new `sales.csv`), the client's asset and everything the
  client derived from it expire too.

A query mixing both sides, `-R/shared/report.html/-/to_text` (validated with `liquers-validate`:
`GetAsset[shared, report.html]`, `Action{to_text}`), should evaluate `to_text` locally on the
remote value.

## Scope and Acceptance Criteria

- **AC-1** A delegated key is evaluated by the server
  WHEN a client whose manager delegates prefix `shared` to a server requests `shared/report.html`, and the server has a recipe for it
  THEN the server's asset manager evaluates it, the client does not run the recipe, and the client's `get` returns the server's value with the server's version
- **AC-2** Status and progress are mirrored
  WHEN the server-side asset reports status changes and progress
  THEN the client's `AssetRef` for that key reports the same status sequence (coalesced as a `watch` channel is) and the same progress entries
- **AC-3** Server expiration reaches client dependents
  WHEN the server expires a delegated key on which a client-side asset depends
  THEN the client's mirror of the key becomes `Expired` and the client's dependent expires with a `Cascaded` reason rooted at the delegated key
- **AC-4** Local keys are unchanged
  WHEN a key outside every delegated prefix is requested
  THEN evaluation, storage and every existing manager test behave as at HEAD
- **AC-5** Writes to a delegated key go to the server
  WHEN the client `set_state`s or `remove`s a delegated key
  THEN the server's manager performs the write or removal (with the server's status rules), and the client mirror follows
- **AC-6** Disconnection is visible and recovers
  WHEN the connection to the server drops and later returns
  THEN in-flight mirrors end `Error` naming the server, and after reconnection the client reconciles every mirrored key's version, expiring those that changed, before trusting events again
- **AC-7** Identity
  WHEN the server requires a principal for the routes the proxy uses
  THEN the proxy authenticates as that principal and a refusal surfaces as the server's error type

Non-goals: split evaluation of one query across hosts by realm (`multi-realm-dispatch`); the
client computing a delegated key and pushing manager-owned metadata (a trusted system API, see the
Design Readiness); conflict resolution between two writers of one key; the browser variant (needs
slice 2, `WEB-HAS-NO-READ-WRITE-REMOTE-STORE`, first); offline operation.

## Core Interactions

- Assets: a new manager (or a delegating layer over a local manager) and a way to build an
  `AssetRef` whose state is driven from outside (no public primitive exists for status/progress).
- Dependencies: delegated keys enter the client's dependency graph as externally versioned roots.
- Store: the delegated prefix's store is slice 1's `liquers_api` store.
- Web/API: the client uses the user Assets API (`key/info`, `key/entry`, `key/version`,
  `key/submit`, `key/cancel`) and WebSockets (`ws/key` for one asset, `ws/events` for expiry).

## Crate Placement

The proxy needs an HTTP and WebSocket client, so it lives with slice 1's client: a `client`
feature of `liquers-axum` (slice 1's recommended answer to its own question 1) — `liquers-core`
gains only the generic primitive to mirror an asset's state. Native only at first; the browser
variant would live in `liquers-web`.

## Documentation Intent

- Reference: extend `WEB_API_SPECIFICATION.md` (client section: the delegated-manager contract);
  `ASSETS.md` (mirrored assets, externally versioned dependency roots).
- Guide: `ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` (the mirror primitive for external managers).
- Other documents: `STORE_CONFIG_FSD.md` or `ENVIRONMENT_CONFIG.md` for the delegation setting.
- Documents to update: the above, once Phases 3-4 exist.

## Design Readiness

- **Readiness:** phase2-blocked
- **Automatic fixing:** not-eligible — `XL`; new public core primitive, a new manager, new
  configuration (rules 1, 3, 4).
- **Leading issue:** **Blocking - authority:** which side owns a delegated key.
- **Explanation:** Phase 2 shows a working shape exists under the recommended authority rule, and
  that its first slice needs **no** trusted system API. But the rule is a system-level decision the
  issue itself marks as blocking, and the proxy would be planned against four designs that are
  unimplemented and three of which still carry open decisions (client placement, identity, event
  kinds), so Phase 3 tests and Phase 4 file-level steps would be speculative.
- **Open questions:**
  1. **Blocking - authority.** Two plausible answers give incompatible behaviour. *Prefix
     authority (recommended):* every key under a delegated prefix is the server's; the client
     never evaluates or stores it itself, and a client recipe for such a key is ignored. *Per-key
     authority* (whoever has a recipe, or whoever wrote last): the client and the server can both
     hold `shared/report.html`, with versions `v_c` and `v_s`; a client dependent records `v_c`, a
     server dependent `v_s`, and each side's next write publishes a version the other's
     dependents contradict, so they expire each other's results on every round. Prefix authority
     is what slice 1 already assumes for the store ("the server's store stays authoritative for
     every key it serves").
  2. **Open design - is a system API needed now?** Recommended: no. Under prefix authority the
     client never writes manager-owned metadata; every read it needs (value, version, status,
     progress, expiration) is on the user Assets API and the WebSockets. A trusted system API
     (whole entries, dependency records, expiration written by a peer) is needed only for
     server-to-server replication or split evaluation, and should be refiled as its own issue
     when one of those is designed.
  3. **Open design - local caching.** Recommended: the mirror caches the value in memory only, as
     any asset does, and the client never writes delegated keys to a local store; invalidation is
     by `ws/events` expiry (and `Updated`/`Removed` from `asset-event-kinds`) plus reconciliation
     after `Lagged` or reconnection.
  4. **Open design - unit of delegation.** Recommended: keys only (prefix delegation). Queries
     whose head is a delegated key evaluate their actions locally on the mirrored value;
     delegating an action is `multi-realm-dispatch`'s question.
  5. **Proposed resolution - reconnection:** on reconnect, compare each live mirror's version with
     `key/version` and expire those that changed (cause `UpdatedInStore`-like, see Phase 2), then
     resubscribe; an in-flight mirror whose server asset is gone ends `Error`.

## Design Dependencies

- `remote-store-and-asset-manager` — **requires**: slice 1 provides the `liquers_api` store, the
  HTTP client, its configuration and the client placement (its question 1, open). Not implemented.
- `access-policy-at-entry-points` (`CORE-SESSION-AND-KEY-ACL`) — **requires**: peer identity
  (AC-7). `needs-decision`, not implemented.
- `external-index-sync` — **requires**: the manager-level expiry stream the server forwards.
- `asset-events-over-websocket` — **requires**: `ws/events`, the out-of-process form of that
  stream (AC-3, AC-6).
- `asset-event-kinds` — **requires** (for AC-5's mirror following writes and for cache
  invalidation by `Updated`/`Removed`).
- `multi-realm-dispatch` — **overlaps**: it delegates actions by realm; this delegates keys by
  prefix. Its transport question would reuse this proxy's client. Both are `phase2-blocked`.
- `unmanaged-asset-registry` (`UNMANAGED-ASSET-NOT-FOLLOWABLE-ACROSS-CONNECTIONS`) — **overlaps**:
  following a server's unmanaged evaluation across a reconnect needs its server-wide id; out of
  scope while only keys are delegated.
- `axum-websocket-connection-limits` — **overlaps**: a proxy holds long-lived connections that the
  server's caps and idle timeouts must allow (its pings keep them alive).
- `environment-orderly-shutdown` — **overlaps**: the proxy's sockets are services needing an
  asynchronous close.

No source in this cluster strongly overlaps this one: the two event designs are prerequisites
(weak overlap, `requires`), not the same root cause, contract or change site.
