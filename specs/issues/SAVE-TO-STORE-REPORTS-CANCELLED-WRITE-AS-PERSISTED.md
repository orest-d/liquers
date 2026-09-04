---
id: SAVE-TO-STORE-REPORTS-CANCELLED-WRITE-AS-PERSISTED
kind: issue
title: A write skipped because the asset was cancelled is recorded as a successful persist
status: draft
priority: P2
complexity: S
area: [core/assets]
design:
created: 2026-09-04
github:
---

## Problem

`AssetRef::save_to_store` (`liquers-core/src/assets.rs:2604`) short-circuits when the asset has
been cancelled, and returns success:

```rust
if self.is_cancelled().await {
    eprintln!("Asset {} cancelled, skipping store write in save_to_store", self.id());
    return Ok(());          // nothing was written
}
```

There is a second such check after serialization (`:2631`), with the same `return Ok(())`.

The caller cannot distinguish this from a completed write. `persist_with_status_tracking` passes the
result to `record_persistence_result`, which reads `Ok(())` as success and records
`PersistenceStatus::Persisted` — for a key that has nothing under it.

`AssetManager::to_override` (`:4078`) then trusts that status when deciding whether a value needs
re-serializing before promotion.

## Impact

An asset whose write was skipped claims to be persisted. A later reader that trusts
`PersistenceStatus` — rather than asking the store — can conclude a value is durable when the store
has never held it. Recovery flows built on `to_override` are the concrete consumer.

P2 rather than P1: it needs a cancellation racing a persist, cancellation already implies the
asset's result is being discarded, and no wrong *value* is produced — the wrong thing is a status
about a value's durability.

## Expected behaviour

The cancelled short-circuit is distinguishable from a successful write. `persist_with_status_tracking`
already has the vocabulary for it: it sets `PersistenceStatus::None` for its own `cancelled`
argument before ever calling `save_to_store`. The same outcome should be reachable when
cancellation is observed *inside* `save_to_store`, so a skipped write records `None` rather than
`Persisted`.

Note that the two checks differ in what has already happened — the second runs after serialization,
so bytes exist but were not written — and a fix should not conflate them with a genuine failure,
which is an `Err` and correctly recorded as one.

## Discovery

Found on 2026-09-04 during the cross-document review of
`specs/design/stale-dependency-status-finalization/` Phase 4, while establishing what the
persistence path reports when a write does not happen.
