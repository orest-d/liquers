---
id: EVALUATE-DOES-NOT-CLEAR-CACHED-BINARY
kind: issue
title: The evaluation body installs a new value without invalidating the cached binary
status: draft
priority: P2
complexity: S
area: [core/assets]
design:
created: 2026-09-04
github:
---

## Problem

`AssetRef::evaluate` (`liquers-core/src/assets.rs:2528`) installs a freshly produced value under
the write lock:

```rust
lock.metadata.with_type_identifier(...).with_type_name(...);
for dep in dependencies { let _ = lock.metadata.add_dependency(dep); }
lock.data = Some(value);
```

It does not clear `lock.binary`. Every other value-installing path does:
`set_value` (`:3338`) and `set_state` (`:3379`) both set `lock.binary = None` with the comment
"Invalidate binary".

`save_to_store` (`:2604`) reads `binary_unchecked()` first and only serializes when that is `None`.
So an asset that holds a cached binary from an earlier evaluation and is then re-evaluated **in
place** would persist the *old* bytes alongside the *new* metadata.

## Impact

Not reachable today, as far as can be established: the manager evicts and reconstructs an asset
rather than re-evaluating one in place, and an asset arrives at `evaluate` with `binary` unset. So
this is latent rather than live, and priority is P2 on that basis.

It is worth recording because the invariant is asymmetric between three paths that all install a
value, and because the failure mode is silent and maximally confusing: correct metadata describing
stale bytes, with no error anywhere. Anything that makes in-place re-evaluation reachable — a
retry, a refresh, an asset reused across runs — turns it live immediately.

## Expected behaviour

`evaluate` clears `lock.binary` when it installs a value, in the same locked block, matching
`set_value` and `set_state`. Alternatively, establish and document that `evaluate` is only ever
reached with `binary == None` and assert it, so the asymmetry is a stated precondition rather than
an accident.

## Discovery

Found on 2026-09-04 during the cross-document review of
`specs/design/stale-dependency-status-finalization/` Phase 4, while tracing the persistence path
to establish which reads `save_to_store` depends on.
