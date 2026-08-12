---
id: CORE-ASSET-GC
kind: issue
title: Assets are never garbage collected
status: accepted
priority: P3
complexity: L
area: [core/assets, core/store]
design: 
created: 2026-08-08
github:
---
## Problem

Nothing removes assets that are no longer referenced. `WeakAssetRef` exists and the expiration
monitor evicts on expiry, but an asset that simply stops being wanted persists in the cache and in
the store.

## Impact

Unbounded growth in any long-running deployment. Expiration bounds *stale* data, not *unwanted*
data.

## Expected behaviour

A collection policy — reachability, or age plus a low-water mark — with explicit semantics for what
happens to a stored representation when its in-memory asset is collected.

Wants a design: what makes an asset unreachable is not obvious once recipes can name it by key.

## Discovery

Migration triage, 2026-08-08. Source: work packages WP-18/19. Verified against HEAD: no GC mechanism exists. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.
