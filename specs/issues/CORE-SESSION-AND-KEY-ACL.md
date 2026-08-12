---
id: CORE-SESSION-AND-KEY-ACL
kind: issue
title: No session model, and no way to authorize writes per key
status: accepted
priority: P2
complexity: L
area: [core/context, core/store, axum]
design: 
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

## Discovery

Migration triage, 2026-08-08. Source: `todo20260219.md` #10, work package WP-12, and the `KEY-LEVEL-ACL` feature brief. Verified against HEAD: the context.rs statement is current. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.
