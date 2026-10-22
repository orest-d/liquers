---
id: CORE-SESSION-AND-KEY-ACL
kind: issue
title: No session model, and no way to authorize writes per key
status: accepted
priority: P2
complexity: L
area: [core/context, core/store, axum]
design: access-policy-at-entry-points
created: 2026-08-08
github:
---
## Problem

Two halves of one gap.

**Session.** `liquers-core/src/context.rs:26-28` states it plainly: "`Session` and `User` are
currently minimal identity abstractions. `Environment::create_session` constructs a session, but
`Context` does not currently contain or expose a session or user." A command therefore cannot know
who is running it.

**Key-level ACL.** `set()` and `set_state()` accept any key from any caller. `specs/FEATURES/`
carried a `KEY-LEVEL-ACL` brief proposing authorization on key patterns; without an identity on the
context there is nothing to authorize against, which is why these are one issue.

## Impact

Any deployment serving more than one user — which is what `liquers-axum` is for — cannot restrict
writes. This is the security gap in the backlog.

## Expected behaviour

`Context` exposes the session and user; the store consults a policy keyed on key patterns before a
write. WP-12 says design first, and that is right.

## Design routes

Recorded 2026-09-15 as maintainer guidance, not yet chosen between. **Liquers aims to be
stateless in the way HTTP is** — `reference/PROJECT_OVERVIEW.md` §"Queries and Recipes define
stateless executions" puts it as: a query is stateless given stateless commands and constant
stored data, which is what makes it map cleanly onto REST. Any session design has to leave that
property intact, which is the constraint that separates the three routes below.

1. **Extend the existing session mechanism.** `Session` (`context.rs:133`), `User` (:117) and
   `Environment::create_session` (:188) already exist; what is missing is the attachment —
   ":119" states that sessions are not on `Context` and not enforced by evaluation. The smallest
   change, and the one that puts mutable identity closest to the evaluation path.
2. **Carry it in the payload.** `Environment::Payload` plus `InjectedFromContext` already does
   this shape; `reference/PAYLOAD_GUIDE.md` uses a `user_id` field as its worked example. It
   preserves statelessness by construction, because a payload is per-evaluation. The limit is
   built in and is the point to weigh: **keys are a payload boundary**, so a payload-carried
   session cannot reach a keyed asset, and a payload-requiring evaluation is never stored or
   reused. Identity would therefore be visible to ad-hoc evaluation and invisible to exactly the
   keyed writes this issue wants to authorize.
3. **A dedicated session folder, sessions as assets.** Sessions become addressable state in a
   store rather than runtime state — which is the most Liquers-shaped answer, since it keeps
   execution stateless and moves the state into the place the model already says state lives.
   It also makes a session inspectable, expirable and dependency-tracked for free. What it needs
   is a contract for who may address that folder, which is circular with the ACL half of this
   issue unless the folder is privileged by the store router rather than by a command.

Not in scope for `AGENT-MEMORY-SERVICE`; that design assumes a single writer and no identity.

## Discovery

Migration triage, 2026-08-08. Source: `todo20260219.md` #10, work package WP-12, and the `KEY-LEVEL-ACL` feature brief. Verified against HEAD: the context.rs statement is current. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.

## Status at 18732dd (2026-10-20)

Still reproduces. `liquers-core/src/context.rs`: `User`, `Session`, `SimpleSession` and
`Environment::create_session` exist and nothing outside `SimpleEnvironment` and one test uses them;
no `liquers-axum` handler sees a caller (`WEB_API_SPECIFICATION.md` §9 says so).

Problem example: on a server mounting the Store and Query APIs, `PUT
/liquer/api/store/data/notes/alice/todo.md` sent by bob overwrites alice's file, and `GET
/liquer/q/-R/notes/alice/todo.md/-/to_text` serves it to him; nothing can be configured to refuse
either. Expected: with a policy granting bob no access to `notes/alice`, both answer 403.

Designed in `specs/design/access-policy-at-entry-points/` (bulk design, `needs-decision`). It
proposes a fourth route besides the three above, the one `context.rs` already states as intent
("Asset evaluation should not depend on the user. Only the access rights will"): a principal
(`User`) per request and a key-pattern policy checked **where requests enter** (HTTP routes,
WebSocket), never on `Context` — so queries stay stateless and cached assets stay shared. Its
leading decision is exactly that choice versus this issue's "`Context` exposes the session and
user". The first slice is core + `liquers-axum`; MCP enforcement is filed as
`MCP-SERVER-APPLIES-NO-ACCESS-POLICY`.

