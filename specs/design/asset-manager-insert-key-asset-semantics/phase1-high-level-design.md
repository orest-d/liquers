# Phase 1: High-Level Design - Keyed Asset Registration Semantics

## Feature Name

Keyed Asset Registration Semantics

## Purpose

Make the AssetManager's keyed-map registration operations explicit and consistent across queued and
immediate managers. This resolves a silent queued-manager no-op without making a low-level map
replacement implicitly cancel, expire, notify, or cascade through a superseded AssetRef.

## Core Interactions

### Query System

No parsing or encoding change; keyed recipe ownership continues to use the existing Key map.

### Store System

No new store operation; external `set_state` remains responsible for persisting its replacement.

### Command System

None.

### Asset System

Clarifies cache registration: `set_state` replaces a deliberately cancelled old keyed asset, while
`to_override` only re-establishes reachability of its same AssetRef after eviction. Lifecycle
invalidation remains an explicit caller-level operation, not a side effect of map mutation.

### Value Types

None.

### Web/API (if applicable)

None.

### UI (if applicable)

None.

## Crate Placement

`liquers-core/src/assets.rs` only; the trait and both manager implementations own keyed-map and
asset-lifecycle behavior, so no dependency edge changes.

## Documentation Intent

**Reference:** Extend `specs/reference/ASSETS.md` with the authoritative keyed-registration and
lifecycle boundary, because future core callers need its present behavior without reading a design.

**Guide:** Neither; this is an internal trait contract, not a repeatable contributor workflow.

**Other documents to create:** None; reconsider if implementation reveals a reusable cache/lifecycle pattern.

**Specific documents to update:** `specs/issues/ASSET-MANAGER-INSERT-KEY-ASSET-NO-OVERWRITE.md`
(resolution) and `specs/README.md` (new in-flight design); Phase 2 will set `affects_docs`.

Audience: core maintainers and coding agents; they must know which operation may displace a map
entry and that replacing map reachability alone does not expire the displaced runtime asset.

## Open Questions

1. Should the trait expose separate replace and insert-if-absent primitives, or one operation
   returning the displaced AssetRef so each caller chooses its lifecycle policy?
2. What atomic guard prevents a delayed `to_override` re-registration from overwriting a newer
   `set_state` result for the same key?
3. Which explicit lifecycle path, if any, should expire a displaced asset and cascade dependents?

## References

- `specs/issues/ASSET-MANAGER-INSERT-KEY-ASSET-NO-OVERWRITE.md`
- `specs/reference/ASSETS.md` (expiration, ownership, and set-state contracts)
- `liquers-core/src/assets.rs` (`set_state`, `to_override`, both AssetManager implementations)
