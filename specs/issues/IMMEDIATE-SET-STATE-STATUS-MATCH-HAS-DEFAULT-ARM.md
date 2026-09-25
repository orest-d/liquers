---
id: IMMEDIATE-SET-STATE-STATUS-MATCH-HAS-DEFAULT-ARM
kind: issue
title: ImmediateAssetManager::set_state matches Status with a default arm
status: draft
priority: P3
complexity: S
area: [core/assets]
design: 
created: 2026-09-25
github:
---
## Problem

`ImmediateAssetManager::set_state` (`liquers-core/src/assets.rs:6776–6781`) decides the stored
status with

```rust
let final_status = match state.metadata.status() {
    Status::Expired => Status::Expired,
    Status::Error => Status::Error,
    _ if self.recipe_opt(key).await?.is_some() => Status::Override,
    _ => Status::Source,
};
```

`CLAUDE.md` requires explicit arms on enum matches so that a new `Status` variant is a compile
error. `DefaultAssetManager::set_state` (`:5630–5652`) makes the same decision with every variant
listed; the two managers can therefore drift apart silently when a status is added.

## Impact

Low today — the two matches agree. The risk is future divergence between the queued and inline
managers, which the asset-lifecycle tests would catch only for the variant they exercise.

## Expected behaviour

The inline manager lists every `Status` variant, as the queued one does, or both call one shared
function.

## Discovery

Noticed in passing during the record-streams Phase 4 final review (2026-09-25), while checking
the store-write sites that `stored: false` must skip.
