---
id: REMOVED-OR-REPLACED-KEY-REVIVED-BY-A-LATE-BACKGROUND-SAVE
kind: issue
title: A background save still in flight after a key is removed or replaced writes the old value back to the store
status: draft
priority: P1
complexity: M
area: [core/assets, core/store]
design:
created: 2026-10-21
github:
---

## Problem

On the queued manager a keyed asset's result is persisted in a spawned task
(`AssetRef::persist_with_status_tracking`, `save_in_background && !inline` → `tokio::spawn` →
`save_to_store`). The asset is already `Ready` when the task starts. `AssetManager::remove`,
`set_state`, `set_binary` and `to_override` take `key_mutation_lock` and call
`AssetRef::cancel_for_replacement`, which leaves a *finished* asset alone ("A finished asset is left
as it is"), and `save_to_store` neither takes `key_mutation_lock` nor re-checks anything but
`Status::Cancelled`. So a save that lands after the removal or replacement writes the old value
back with status `Ready`:

- `remove` of a computed key: the store entry is rewritten as `Recipe` (or nothing is written when
  the save has not landed yet), then the save writes `Ready` — the removed value is served again by
  fast track;
- `set_state` / `set_binary` replacing a computed key: the user's new value is overwritten in the
  store by the old computed value; the in-memory asset holds the new value, so the loss shows only
  after a restart or eviction.

## Problem Example

Observed, not constructed: before runtime volatility landed, `liquers-core/tests/evaluation_session.rs`
`volatile_dependency_has_one_value_per_evaluation` evaluated `-R/data/c.txt`, called
`manager.remove(data/c.txt)` and evaluated it again; in 16 of 20 runs (binary built from
`2bc4228`) the second evaluation logged "Fast track successful for asset with key data/c.txt",
i.e. it loaded a `Ready` copy that the first evaluation's background save wrote after the removal.
At HEAD the test passes only because `c` is now volatile. A non-volatile key in the same sequence
is expected to behave as before (not re-run at HEAD with a dedicated test).

## Impact

Removed values reappear, and a user-supplied value can be silently replaced by a stale computed one
in the store (data loss after restart). Only the queued manager (native) saves in the background.
Workaround: wait for `AssetRef::persistence_status()` to leave `None` before removing or replacing.

## Expected behaviour

A write-back of an asset that is no longer the key's current owner (removed, replaced, cancelled)
does not reach the store. Possible directions, none chosen: `save_to_store` takes
`key_mutation_lock` and checks that the asset is still the registered owner (or, for an uncached
key, that no removal or replacement happened since); `remove` / `set_*` wait for an in-flight save
of the old asset; `cancel_for_replacement` closes a "may persist" gate on finished assets too, which
the save checks under the lock; a conditional store write (`store-conditional-write`).

## Discovery

2026-10-21, bulk design of cluster C1b, while diagnosing `EVALUATION-SESSION-VOLATILE-TEST-INTERMITTENT`
(design `evaluation-session-volatile-test-flake`, hypothesis H1). Related:
`UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION` (another late write-back race, different
trigger and fix site).
