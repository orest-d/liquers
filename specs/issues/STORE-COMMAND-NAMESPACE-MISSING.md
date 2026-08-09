---
id: STORE-COMMAND-NAMESPACE-MISSING
kind: feature
title: Store contents cannot be read or written from a query
status: draft
priority: P3
complexity: M
area: [lib/commands, core/store]
design:
created: 2026-08-09
github:
---

## Problem

No crate registers a command that touches the store. `liquers-lib/src/commands.rs` registers none,
and there is no `store` namespace anywhere. The store is reachable only through the evaluation
machinery — a `-R/` resource query becomes a `GetAsset` step — and through whatever API the host
exposes (`liquers-axum` routes, or the `Store` wasm class added by
`specs/design/liquers-web-store/`).

Consequences a query cannot express today:

- Listing a directory. `AsyncStore::listdir` exists and nothing surfaces it to a query.
- Reading a key chosen at evaluation time rather than written into the query text.
- Writing a result to a key, so a pipeline cannot persist an intermediate under a name of its own
  choosing.
- Asking whether a key exists, to branch on it.

## Impact

Every store interaction has to happen outside the query language, so a workflow that would
naturally be one query becomes a query plus host code. It also means each host re-implements the
same four operations: `liquers-axum` has HTTP routes for them, and the browser integration is
about to add a JavaScript class for them, with no shared command layer underneath.

Low priority because both existing hosts do have a way to do it — the gap is expressiveness, not
capability.

## Expected behaviour

A `store` namespace in `liquers-lib`, so that every target gets it rather than one host at a time.
Roughly `store_get`, `store_metadata`, `store_list`, `store_contains`, and — separately, see below
— `store_set`.

Two things need deciding before this is built, and neither is obvious:

1. **Writes need `volatile` and probably need more than that.** A command that writes to the store
   makes evaluation non-deterministic in the way Liquers' dependency tracking assumes it is not.
   At minimum such commands are `volatile`; whether they should exist at all in a query, rather
   than only in a recipe, is the real question.
2. **Reads from a query are an authorization surface.** A `store_get` whose key comes from a
   parameter lets a query read any key the environment's store can reach, which is a different
   posture from `-R/`, where the key is part of the query text and visible to any policy inspecting
   it. This wants resolving together with `CORE-SESSION-AND-KEY-ACL`.

## Discovery

Raised as Phase 2 open question Q8 of `specs/design/liquers-web-store/` (2026-08-09) and closed by
the user as out of scope for that design: the capability belongs in `liquers-lib` so every target
gets it, and folding it into a browser-store design would have turned "the browser can have a
store" into "queries can mutate stores" without the security discussion that deserves.
