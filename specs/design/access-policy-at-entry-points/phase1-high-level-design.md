# Phase 1: High-Level Design — Principals and an access policy at the entry points

## Purpose

A `liquers-axum` server today acts with the environment's full rights for every caller, so a
deployment serving more than one person cannot let one read `public/` while only another writes
`notes/alice/`. Give every request a **principal** and check it against a **key-pattern policy**
where the request enters (HTTP routes, the WebSocket), without letting identity into evaluation,
so queries stay stateless and their cached assets stay shared (VISION non-goal "No stateful query
language"; `context.rs` module doc: "Asset evaluation should not depend on the user. Only the
access rights will"). It is the security gap of "Server / data service" and of a hosted
playground (`POSITIONING.md` open question 6).

## Problem Example

A server mounts the Store and Assets APIs over a store with `public/` and `notes/alice/`, behind
an authentication layer that knows the caller is `bob`:

```text
PUT  /liquer/api/store/data/notes/alice/todo.md        (as bob)  → 200, alice's file overwritten
GET  /liquer/q/-R/notes/alice/todo.md/-/to_text          (as bob)  → 200, alice's file served
```

There is nothing to configure: no handler sees who called, and no policy exists. Should be: with
the policy below, both answer **403** `AccessDenied` naming the key, while `GET
/liquer/q/-R/public/readme.md/-/to_text` still answers 200 for anyone.

```yaml
default: deny
rules:
  - { principals: ["*"],          operations: [read],        keys: ["public"] }
  - { principals: ["user:alice"], operations: [read, write], keys: ["notes/alice"] }
  - { principals: ["*"],          operations: [evaluate],    commands: ["*"] }
```

## Scope and Acceptance Criteria

- **AC-1** Write refused
  WHEN a principal without a matching `write` rule sends any key mutation (Store API `PUT`/`DELETE`/`upload`/`makedir`/`removedir`, Assets API key mutations)
  THEN the answer is 403 with `error.type` `AccessDenied` naming the key, and nothing is written
- **AC-2** Query reads checked
  WHEN a query (Query API, Assets `q/` routes, `ws/q`) names a key in a resource or asset step, a link, a `_link_` option or a nested query, and the principal may not `read` it
  THEN the request is refused with 403 before anything is evaluated
- **AC-3** Commands checked
  WHEN a query runs a command no `evaluate` rule of the principal matches
  THEN it is refused with 403 naming the command, before evaluation
- **AC-4** Allowed requests unchanged
  WHEN every check passes
  THEN the answer is byte-for-byte what it is without a policy, and the asset is the one every other principal gets for the same query (one cached asset, no per-principal copy)
- **AC-5** No policy, no change
  WHEN no policy is configured
  THEN every route behaves as at HEAD (all existing axum tests pass unchanged)
- **AC-6** Principal from the request
  WHEN an outer layer inserts a `User` request extension
  THEN it is the principal; without one the principal is `User::Anonymous`
- **AC-7** Listings filtered
  WHEN a principal lists a directory it may read (Store `listdir`/`keys`, Assets `key/listdir`, Recipes `listdir`)
  THEN entries it may not read are omitted
- **AC-8** Admin gated
  WHEN a principal without an `admin` rule calls an `admin/` route
  THEN 403
- **AC-9** Error type round-trips
  WHEN `AccessDenied` crosses a binding (`liquers-py` exception class, `liquers-web` error string) or the axum `error.type` parser
  THEN it maps to itself, and HTTP 403

Non-goals (slice 1): authentication (an embedding layer's job; AC-6 is the seam); principal
groups or roles; authorizing a command's own store writes (allowed by allowing the command, as the
MCP allow-list does); recipe transitivity (Q5); `liquers-mcp` enforcement (filed:
`MCP-SERVER-APPLIES-NO-ACCESS-POLICY`); a session store, expiry or sessions as assets (issue routes
1 and 3); per-principal rate limits.

## Core Interactions

- Context: `User` becomes the principal (derives only); `Session`, `SessionType` unchanged and
  still not on `Context`.
- Plan: a read-only walk of a built plan collects the keys and commands it names.
- Error: a new `ErrorType::AccessDenied`.
- Web/API: every route checks its operation; the WebSocket checks each subscribe.
- Bindings: the new error variant in `liquers-py` and `liquers-web`.

## Crate Placement

`liquers-core` (new `access.rs`: `AccessPolicy` trait, `RulePolicy`, the plan walk — shared by every
entry crate, and what `liquers-mcp` will call); `liquers-axum` (extraction and enforcement);
`liquers-py`, `liquers-web` (error mapping only). `liquers-lib` unchanged.

## Documentation Intent

- Reference: `WEB_API_SPECIFICATION.md` §9 rewritten (principal, policy, 403, what is checked per
  route), §3.1 row, Appendix B line removed; `PROJECT_OVERVIEW.md` stateless section gains "who may
  ask is checked at the boundary"; `DOC_04` Session/User paragraph.
- Guide: `WEB_API_GUIDE.md` new section "Restricting access" (policy YAML, an auth layer inserting
  `User`).
- Other documents: `POSITIONING.md` caveat line.
- Documents to update: the source issue; `specs/README.md` capability line (planned → built).

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — complexity `L`, a new `ErrorType` variant and a new `pub`
  trait reaching four crates (rules 1, 3, 4, 6).
- **Leading issue:** Q1, where identity is enforced.
- **Explanation:** under the recommended answers the slice is concrete: every entry point already
  parses a key or builds a plan, and `liquers-mcp` `check_allowed` proves the plan walk. The
  decisions change the contract, so they stay visible.
- **Open questions:**
  1. **Open design - enforcement point:** at the entry points, identity never on `Context`
     (recommended), or on `Context`/the store as the issue's "Expected behaviour" says. Example of
     the difference: alice evaluates `-R/notes/alice/x/-/summary`; its asset is cached under that
     query. At the boundary, bob asking the same query is refused before the cache is consulted.
     With identity in `Context`, the cached asset (computed with alice's rights) is handed to bob
     unless the cache key includes the principal — which ends shared caching and statelessness.
  2. **Proposed resolution - principal type:** reuse `User` (`System`, `Anonymous`, `Named`) with
     `Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize` (recommended), not a new type.
  3. **Open design - refusal:** new `ErrorType::AccessDenied` → 403 (recommended), or masking
     denied reads as 404 `KeyNotFound` so existence does not leak (stronger, but a client cannot
     tell "missing" from "forbidden").
  4. **Proposed resolution - configuration:** one `Arc<dyn AccessPolicy>` given to each builder
     (`with_access_policy`) (recommended: the policy belongs to the deployment, not to evaluation),
     not a new `Environment` method.
  5. **Proposed resolution - recipes:** check what the request names, not what a key's recipe
     reads (recommended: a recipe is server-side configuration; reading `public/summary` grants its
     value even if its recipe reads `notes/`). Documented as the publishing mechanism.
  6. **Proposed resolution - defaults:** no policy = allow all (AC-5); a policy with no matching
     rule = its `default` (`deny` in examples).

## Design Dependencies

- `unmanaged-asset-registry` — **required-by**: binds a registry id to the principal that
  created it (AC-6's `User`); can ship earlier as a bearer capability.
- `NO-REMOTE-STORE-OR-ASSET-MANAGER` (issue, no design) — **required-by**: its trusted system API
  takes peer identity from here ("should not invent its own"); a peer is a `User` with `admin`.
- `assets-api-admin-operations`, `axum-websocket-connection-limits` — **overlaps**: new routes get
  an operation (`write`, admin read); per-principal limits would build on AC-6. No order.
- `agent-memory-mvp` (complete) — **overlaps**: its query-tool allow-list "should give way to
  permissions checked by core"; the core walk here is that, wiring is the filed follow-up.
- `STORE-COMMAND-NAMESPACE-MISSING` (issue) — **overlaps**: a command reading a key given as a
  parameter is authorized as a command (`evaluate`), not per key; recorded there.
- `store-conditional-write` — **overlaps**: authorization half of multi-writer use; no order.

## Consolidated Findings

- At HEAD: `User`/`Session`/`create_session` exist (`context.rs`), are used by nothing but
  `SimpleEnvironment` and a test, and no handler sees a caller; `WEB_API_SPECIFICATION.md` §9 says
  so. The issue persists unchanged.
- The issue's three routes all put identity in or next to evaluation; route 2 (payload) cannot
  reach keyed assets and route 1 breaks shared caching (Q1 example). This design is a fourth route
  the issue did not list, and the one `context.rs` already states as intent.
- Every entry point already parses a key or plans a query, so enforcement is one call per handler
  after parsing; overrides must be included (a `_link_` option names another query).
- `AccessDenied` follows the `configuration-error-kind` precedent: `error.rs`, axum `error.rs`
  (two functions and `keyabs15b`), `liquers-py` `error.rs` and `_errors.py` (generated class),
  `liquers-web` `error.rs`, `assets.rs` persistence match.
- Slice size is `L` at its limit; MCP wiring, groups, and per-key listing filters beyond the
  four listing routes are follow-ups.
- `WEB_API_SPECIFICATION.md` §9 also cites `STORE-NO-READ-ONLY-ADAPTER` as open; it is closed
  (`ReadOnlyStore`, mount `read_only`) — filed as `WEB-API-SPEC-SAYS-A-DIRECTORY-MOUNT-CANNOT-BE-READ-ONLY`.
