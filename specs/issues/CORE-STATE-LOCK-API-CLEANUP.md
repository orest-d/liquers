---
id: CORE-STATE-LOCK-API-CLEANUP
kind: issue
title: "`State` holds an `RwLock` that may not be needed"
status: closed
priority: P3
complexity: S
area: [core/value]
design: state-lock-marker-cleanup
created: 2026-08-08
github:
---
## Problem

`liquers-core/src/state.rs:15` — `// TODO: try to remove rwlock`.

## Problem Example

```rust
// liquers-core/src/state.rs:14-21 at HEAD
pub struct State<V: ValueInterface> {
    // TODO: try to remove rwlock
    data: Arc<V>,
    pub metadata: Arc<Metadata>,
}
```

Today: the marker asks for a lock to be removed that the struct no longer has. Should be: a doc
comment stating that `State` is two shared handles, never locked, changed only by building a new
`State`.

## Impact

A lock on every `State` costs an atomic operation on paths that may not need one, and it shapes the
API: callers work through guards where a plain borrow might do.

## Expected behaviour

Either the lock is removed and `State` becomes a plain value, or the marker is replaced by a
comment explaining what requires it. Worth measuring before changing — `BENCHMARK-SUITE` is the
prerequisite.

## Discovery

Migration triage, 2026-08-08. Source: `todo20260219.md` #13. Verified against HEAD: marker present at `state.rs:15`. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.

## Update 2026-10-20: the lock is gone; only the marker remains

Re-checked at HEAD (18732dd) during bulk design. `State` holds `data: Arc<V>` and
`pub metadata: Arc<Metadata>`; `state.rs` imports only `std::{borrow::Cow, sync::Arc}`, and
`grep -n RwLock liquers-core/src/state.rs` finds nothing but the comment at line 15. The first
expected outcome above — the lock removed and `State` a plain value — is therefore already true.
The archived audit's companion marker on the public surface (line 16 in February) is gone too:
`data` became private behind guarded accessors.

What is still wrong is the comment itself: it sends readers looking for a lock that does not exist.
Re-sized `M` → `S`; the remainder (replace the marker with the ownership model, and say what
`set_status` costs) is designed in [`state-lock-marker-cleanup`](../design/state-lock-marker-cleanup/DESIGN.md).
No benchmark is needed for it. Copy-on-write metadata belongs to
`SHAREABLE-METADATA-AND-BYTES-ARE-DEEP-COPIED`.

## Resolution

Fixed 2026-10-27 by `design/state-lock-marker-cleanup/`. The `// TODO: try to remove rwlock` marker in `liquers-core/src/state.rs` is gone (there has been no lock for a long time). `State`'s doc comment now describes the model: an immutable pair of `Arc` handles, cloned by two reference-count increments, with no interior mutability and nothing to lock, changed only by building new handles; `set_status` says what it costs (a clone of the whole `Metadata`). Comment-only change.
