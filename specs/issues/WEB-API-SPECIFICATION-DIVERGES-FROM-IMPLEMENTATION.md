---
id: WEB-API-SPECIFICATION-DIVERGES-FROM-IMPLEMENTATION
kind: issue
title: The web API specification diverges from the implementation in three further ways
status: draft
priority: P1
complexity: S
area: [axum, docs]
design: 
created: 2026-09-16
github:
---
## Problem

`reference/WEB_API_SPECIFICATION.md` is a `reference/` document, so by `DOCS_STRUCTURE_GUIDE.md`
§2 it must be true at HEAD. Three claims in it are not, each of which makes a reader's first
attempt fail:

**1. The store entry write is `PUT`, not `POST`.** The specification says `POST` at line 99 and
again in §4.1.14, with worked CBOR and JSON examples. `StoreApiBuilder::build`
(`liquers-axum/src/store/builder.rs:56-61`) wires `/entry/{*key}` as `get` + `put` + `delete`.
A client following the specification gets **405 Method Not Allowed** on every write.

**2. `FullApiBuilder` does not exist.** The specification uses it five times as the assembly entry
point — lines 2026, 2028, 2071, 2278, 2288, including the authorization example in §9.1 and a
`main()` in §10. A repo-wide search finds no definition and no export. `liquers-axum` exposes
`QueryApiBuilder`, `StoreApiBuilder`, `AssetsApiBuilder` and `RecipesApiBuilder`, which a caller
merges itself. Code written from §10 does not compile. Two of the five references also name crates
that do not exist under those names (`liquers_web`, `liquers_web_axum`).

**3. The assets WebSocket path is not `/ws/assets`.** The specification shows
`/liquer/ws/assets/{*query}` (§5.2.1, and the comparison table at line 884).
`AssetsApiBuilder::new` defaults `websocket_path` to `format!("{}/ws", base_path)`
(`liquers-axum/src/assets/builder.rs:31`), so with the base path the specification itself uses the
route is `/liquer/api/assets/ws/{*query}`. A client following the documented path gets **404**, and
`with_websocket_path` is the only way to obtain the documented one.

## Impact

Each of the three is a first-contact failure: a 405, a compile error, and a 404. Together with
`AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED` — six documented endpoints returning 501 — the
specification cannot be used as written for either reading or writing, and there is no way to tell
which parts are true except by reading the handlers.

Filed at P1 rather than P0 because each has an obvious correct form that a reader will find by
inspection, and no data is at risk. What is at risk is the document's authority: a reference that
is wrong in four known places stops being consulted.

## Expected behaviour

For each, decide which side is right and change the other in the same commit:

- **Entry write.** `PUT` is the better verb for a write to a named key and is what the code does;
  the likely fix is the specification. But §4.1.14's request format is detailed enough that it may
  have been written against an intended `POST` handler — check before assuming.
- **`FullApiBuilder`.** Either write the aggregate builder the specification describes, which is a
  genuine convenience the four separate builders do not provide, or rewrite §9.1 and §10 to
  compose the existing builders. The second is much cheaper and equally honest.
- **WebSocket path.** Either change the default to match the specification or document the actual
  default. Changing the default is a breaking change for anyone already using it; documenting it
  is not.

A test asserting the routed methods and paths against the specification's table would stop the
next divergence. `AXUM-HANDLER-TEST-COVERAGE` is the place that belongs.

## Discovery

Reported by Codex review on PR #70 (three separate comments, 2026-09-15) against an early draft of
`design/agent-memory-mvp/`, which had copied all three forms out of the specification in good
faith. Each was verified independently against HEAD on 2026-09-16 before filing: the builder
sources, a repo-wide search, and the specification's own line numbers.
