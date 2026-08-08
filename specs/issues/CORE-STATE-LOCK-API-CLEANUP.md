---
id: CORE-STATE-LOCK-API-CLEANUP
kind: issue
title: `State` holds an `RwLock` that may not be needed
status: draft
priority: P3
complexity: M
area: [core/value]
design: 
created: 2026-08-08
github:
---
## Problem

`liquers-core/src/state.rs:15` — `// TODO: try to remove rwlock`.

## Impact

A lock on every `State` costs an atomic operation on paths that may not need one, and it shapes the
API: callers work through guards where a plain borrow might do.

## Expected behaviour

Either the lock is removed and `State` becomes a plain value, or the marker is replaced by a
comment explaining what requires it. Worth measuring before changing — `BENCHMARK-SUITE` is the
prerequisite.

## Discovery

Migration triage, 2026-08-08. Source: `todo20260219.md` #13. Verified against HEAD: marker present at `state.rs:15`. See `specs/DOCS_MIGRATION_PLAN.md` §4.0c.
