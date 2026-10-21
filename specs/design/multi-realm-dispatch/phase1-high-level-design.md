# Phase 1: High-Level Design — dispatching a realm to another environment or host

## Purpose

Make the *client/server* shape of `VISION.md` ("the server holds the store and heavy commands; the
browser evaluates its own queries … computation on whichever side it belongs") work inside one
query: a transform segment whose realm names another host runs there, and the rest runs locally.
"Hosts differ in which commands their realm provides, never in what the language means" (Vision 2).

## Problem Example

A browser page runs a `liquers-web` environment. The server (`liquers-axum`) has a command
`heavy_stats` that needs Polars; the page calls the server's realm `server`:

```
-R/data/sales.csv/-server/heavy_stats/-/to_text
```

The query is valid if the page knows the server's command metadata (validated with an overlay
registry declaring `server` / `heavy_stats`; it plans as `GetResource[data, sales.csv]`,
`Action{server/heavy_stats}`, `Action{to_text}`).

- **Today:** the page fails at plan time with `Action 'heavy_stats' not registered; command keys
  tried: 'server--heavy_stats'`. If the page imported the server's metadata
  (`registerCommands` accepts `command_registry.yaml`, `liquers-web/src/command/import.rs`) it
  would plan, then fail in `CommandRegistry::execute` with `unknown_command_executor`: nothing
  routes a realm anywhere but the local registry (`REALMS.md` "How a realm is executed").
- **Should:** `heavy_stats` runs on the server, its result crosses back, and `to_text` runs in the
  page — one query, one asset, with the remote part's provenance and dependencies kept.

## Scope and Acceptance Criteria

- **AC-1** A remote realm runs remotely
  WHEN a query's transform segment names a realm bound to a remote host and the plan reaches it
  THEN that segment is evaluated by the remote host and its value becomes the input of the next local step
- **AC-2** Local realms are unchanged
  WHEN no realm in a query is bound to a remote host
  THEN plans, evaluation and every existing test in `plan.rs` and `interpreter.rs` behave as at HEAD
- **AC-3** An unbound realm is an ordinary planning error
  WHEN a segment names a realm that is neither registered locally nor bound remotely
  THEN planning fails with today's `ActionNotRegistered` error naming the keys tried
- **AC-4** Remote failure is reported as the step's failure
  WHEN the remote evaluation fails or the host is unreachable
  THEN the local asset ends `Error` with an error naming the realm, the remote query and the remote error type
- **AC-5** Provenance survives the crossing
  WHEN a remotely evaluated part succeeds
  THEN the local asset's dependency records name the remote part so that its expiration can expire the local asset

Non-goals: splitting one action across hosts; automatic choice of a host for a realm-less command;
moving computation for load balancing (`VISION.md` non-goal: not a distributed orchestrator);
foreign realms such as `store_config`, which are never executable.

## Core Interactions

Query/plan (`Step::Action` already carries `realm`); commands (executor lookup by `CommandKey`);
context/environment (where a realm binding lives); assets (dependency on a remote part); web/axum
(the transport); value types (what may cross — `realm-type-registry`).

## Crate Placement

`liquers-core`: the realm-binding seam and its error, transport-free. `liquers-web` and a native
client crate: the transports. `liquers-axum`: nothing new for the recommended route below, which
reuses `q/entry`.

## Documentation Intent

- Reference: extend `specs/reference/REALMS.md` ("How a realm is executed"; the open question
  "Realm-aware dispatch") once a route is chosen.
- Guide: none until a transport exists.
- Other documents: `PROJECT_OVERVIEW.md` §realms; `VISION.md` is not changed.
- Documents to update: `REALMS.md`, `WEB_API_SPECIFICATION.md` Appendix B if a route is added.

## Design Readiness

- **Readiness:** phase2-blocked
- **Automatic fixing:** not-eligible — `XL`, a new seam in `liquers-core` and two transports
  (rules 1, 3, 4).
- **Leading issue:** the dispatch unit (question 1).
- **Explanation:** every in-process prerequisite exists (realm-aware planning, `CommandKey`
  lookup, `Environment::CommandExecutor` as an associated type), so the seam itself is small; but
  the two candidate units produce incompatible caching, data movement and query identity, and the
  transport and identity they need are undesigned elsewhere.
- **Open questions:**
  1. **Blocking - dispatch unit:** does a remote realm receive *one action with its input state*
     (per-action RPC) or *the query prefix ending in that segment* (prefix delegation)? On the
     problem example, per-action downloads `data/sales.csv` to the page and uploads it back with
     the call, and the server caches nothing; prefix delegation sends
     `-R/data/sales.csv/-/heavy_stats`, which the server caches and dependency-tracks, with no
     upload. But for `-R/local/notes.csv/-server/heavy_stats`, where `local/` is a browser store,
     prefix delegation cannot work at all (the server cannot read the page's store) and per-action
     can. Recommended: prefix delegation, refusing at plan time a prefix the remote cannot resolve,
     because it keeps the remote part an addressable, cached asset (Vision 1, 3) and rides on the
     existing `q/entry` route; per-action as a later, explicit extension.
  2. **Blocking - realm name on the wire:** the server knows `heavy_stats` in its realm `""`; the
     page calls it `server`. Delegation must rewrite the header (`-server/` → `-/`), so the same
     command has two encodings and two cache keys. Recommended: the binding owns the rename
     (local realm name → remote realm name), and the local asset's query keeps the local spelling.
  3. **Open design - where a binding is declared:** environment configuration
     (`ENVIRONMENT_CONFIG.md`, beside stores) or code (`EnvironmentBuilder`). Recommended: both,
     config delegating to the builder, as stores do.
  4. **Open design - remote metadata:** the planner needs the remote's command metadata to plan
     `heavy_stats`; nothing publishes it (filed `REGISTRIES-NOT-PUBLISHED-ACROSS-REALMS`).
     Recommended: import it under the bound realm name when the binding is made.

## Design Dependencies

- `remote-store-and-asset-manager` (`NO-REMOTE-STORE-OR-ASSET-MANAGER`) — **requires**: prefix
  delegation is "evaluate this query on that asset manager"; its remote asset manager and status
  notifications are what make AC-5 (remote expiration) possible.
- `realm-type-registry` (`TYPE-REGISTRY-NOT-REALM-AWARE`) — **required-by**: types per realm
  attach to dispatch; its first slice is independent of this design and goes first.
  `CROSS-REALM-TRANSFER-NOT-PLANNED` (filed) is where the two meet.
- `access-policy-at-entry-points` (`CORE-SESSION-AND-KEY-ACL`) — **requires**: a realm call made
  on a user's behalf needs the identity that design gives.
- `UNMANAGED-ASSET-NOT-FOLLOWABLE-ACROSS-CONNECTIONS` (issue) — **overlaps**: a delegated prefix
  with argument overrides is unmanaged on the server and can only be awaited, not followed.
- `WEB-CANCELLATION-INERT` (issue) — **overlaps**: in the browser a remote step is evaluated
  inline, so cancelling it needs that issue's deferred manager.
- `environment-orderly-shutdown` (`ENVIRONMENT-ORDERLY-SHUTDOWN`) — **overlaps**: open remote
  connections are a service that needs asynchronous cleanup.
- `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` (issue) — **overlaps**: a large value
  crosses whole until that lands.
- Overlap judgement: `NO-REMOTE-STORE-OR-ASSET-MANAGER` was the close call. T2 (shared contract)
  was rejected because only the *remote* binding needs its protocol, while an in-process binding
  (another environment in the same process) does not; the relation is a shared prerequisite, so
  weak overlap, recorded as `requires`.
