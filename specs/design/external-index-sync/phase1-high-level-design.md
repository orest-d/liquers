# Phase 1: High-Level Design — Asset expiration events observable by scope

## Purpose

An observer (an external index feeder, a cache warmer, an audit log, a metrics exporter) must be
able to learn *which* assets expired under a scope without holding those assets, without missing
an event silently, and including keys that are stored but not live. This is the push path of the
external-index interoperability layer. It reduces latency only; reconciliation remains the
correctness guarantee (`interoperability-layer.md` §5).

## Scope Changes

- **2026-10-21, narrowed (bulk design).** This folder was created on 2026-10-10 for the whole
  external-index layer (feed, reconciliation diff, projection identity, sinks, indexation policy),
  with this issue as its only source. That is a multi-crate project with seven open questions and
  no first consumer. The design now covers its source issue only: the manager-level expiration
  event stream in `liquers-core`. The rest is filed as `EXTERNAL-INDEX-HAS-NO-RECONCILIATION-CONTRACT`,
  which takes over the open questions. `interoperability-layer.md` and `indexation-policy.md` stay
  in this folder as that issue's input, unchanged. The slug is kept because other documents link
  it.

## Problem Example

`data/b.txt`'s recipe reads `-R/data/a.txt` (both validate as `GetAsset[data, …]`). An observer
wants "tell me when anything under `data/` expires":

```rust
let m = envref.get_asset_manager();
m.set_state(&parse_key("data/a.txt")?, new_state).await?;   // cascade: data/b.txt expires
```

Today, checked at HEAD (`liquers-core/src/assets.rs`):

- the only subscriptions are `AssetData::subscribe_to_notifications` and
  `AssetRef::subscribe_to_notifications`, both per asset, so the observer must already hold
  `data/b.txt`, and cannot hold assets that do not exist yet;
- if `data/b.txt` is not live, `expire_dependencies_result` calls `expire_stored_copy`, which
  rewrites its stored metadata to `Expired` and notifies nobody;
- the per-asset channel is a `watch`, so `Expired` followed by a recomputation is observed as
  `Ready` alone.

Should be: `m.subscribe_asset_events(AssetEventScope::Under(parse_key("data")?))` yields
`AssetEvent::Expired { subject: Key(data/b.txt), reason: Cascaded { cause: Updated { .. }, root:
-R/data/a.txt, via: -R/data/a.txt }, version: Some(old), live: false }`. A subscriber that falls
behind gets `Lagged(n)`, never a silent gap.

## Scope and Acceptance Criteria

- **AC-1** Scoped subscription without holding assets
  WHEN an observer subscribes to `Under(data)` and a write to `data/a.txt` cascades to a live
  `data/b.txt` the observer never requested
  THEN it receives `Expired` for `data/b.txt` with the `Cascaded` reason the metadata records
- **AC-2** Stored-only keys are announced
  WHEN the cascade expires a key that is stored but not live (`expire_stored_copy`), or
  `AssetManager::expire` expires a stored-only key
  THEN an `Expired` event with `live: false` is delivered
- **AC-3** No silent loss
  WHEN more events are sent than the buffer holds while the subscriber is not receiving
  THEN its next receive returns `Lagged(n)`, n > 0, and later receives continue in order
- **AC-4** A sequence, not a snapshot
  WHEN two keys expire one after the other, and the first is recomputed in between
  THEN both events are received, in send order, and a root's event comes before its dependents'
- **AC-5** Scope filter
  WHEN events occur under `data/` and under `other/`, and for a non-keyed query asset
  THEN `Under(data)` receives only the `data/` ones; `All` receives all, queries included
- **AC-6** Cause and version
  WHEN an asset expires by deadline, explicitly, or by cascade
  THEN the event's `reason` is the `ExpiryReason` recorded in its metadata, and `version` is the
  version it held before expiring
- **AC-7** Every manager
  WHEN the same scenario runs on the queued, the inline and the external test manager
  THEN the same events are delivered, with no change to the external manager's code
- **AC-8** No subscriber, no effect
  WHEN nobody subscribes
  THEN expiry behaves exactly as today and sending costs one failed `broadcast::send`

Non-goals: delivery over the WebSocket or to another process
(`ASSET-EVENTS-ARE-NOT-DELIVERED-OUT-OF-PROCESS`); events for written versions, additions and
removals (`ASSET-EVENT-STREAM-CARRIES-ONLY-EXPIRATIONS`); the reconciliation layer
(`EXTERNAL-INDEX-HAS-NO-RECONCILIATION-CONTRACT`); changing the per-asset `watch` channel, which
stays right for UIs.

## Core Interactions

- **Assets:** the three places an expiry is recorded; a provided `AssetManager` method.
- **Dependency manager:** owns the broadcast sender, one per manager.

## Crate Placement

`liquers-core` only. No binding, command or other crate changes.

## Documentation Intent

- Reference: extend `reference/ASSETS.md` (§Communication Channels: the manager event stream)
  and `reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` (subscription API).
- Guide: extend `guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` §8 (`record_expiry` overrides no
  longer need to forward events; events are sent regardless).
- Other documents: `interoperability-layer.md` §5 "The push path does not exist yet" gets a
  dated note at implementation time.
- Documents to update: the three above.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. `L`; new public types and a new `AssetManager` method
  (rules 1, 3, 4).
- **Leading issue:** Proposed resolution, channel shape: a separate manager-level broadcast with its
  own event type.
- **Explanation:** every expiry already passes through three recording sites, and every manager
  already owns a `DependencyManager`. Emitting there needs no new required trait method and covers
  live and stored-only keys. The remaining items are contract choices with recommended answers.
- **Open questions:**
  1. **Proposed resolution, channel shape:** a new `tokio::sync::broadcast` per manager with its
     own `AssetEvent` type, not a generalisation of `AssetNotificationMessage`, whose progress
     variants a scope observer has no use for. The issue's question 1.
  2. **Proposed resolution, delivery:** in-process only in this slice. The assets WebSocket is the
     natural out-of-process form, filed as `ASSET-EVENTS-ARE-NOT-DELIVERED-OUT-OF-PROCESS`.
  3. **Proposed resolution, ordering:** one total order per manager (send order). Within one
     cascade, the root's event precedes its dependents'. No promise across concurrent cascades or
     across processes.
  4. **Proposed resolution, event kinds:** expirations only. Written versions (which also cover
     additions to a folder, through `refresh_listing_version`) are a follow-up. The issue's
     directory-listing concern is otherwise resolved: `DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-
     REGISTERED-OR-CHECKED` is closed.
  5. **Proposed resolution, enum evolution:** `AssetEvent` is not `#[non_exhaustive]`, so callers
     match explicitly (`CLAUDE.md` match rule), and adding a kind is a visible breaking change.
     Alternative: `#[non_exhaustive]` and a forced `_` arm outside the crate.
  6. **Implementation details (resolved in Phase 2):** buffer of 1024 events; an anonymous asset
     (neither key nor query) is not announced; the issue's "monitor versus cascade" question is
     answered by `ExpiryReason`, which exists.

## Design Dependencies

- `asset-manager-store-sync`: **overlaps**. Its expirations are announced through the same sites.
- `asset-garbage-collection`: **overlaps**. A collection is not an expiry and sends no event.
- `store-and-asset-search` (the origin of the folder): its interoperability layer is now
  `EXTERNAL-INDEX-HAS-NO-RECONCILIATION-CONTRACT`, **required-by** in the sense that its push path
  is this design.
- `axum-assets-endpoints`: **overlaps** via the out-of-process follow-up only.

## Consolidated Findings

- The three emission sites are the complete set of `record_expiry` callers: `AssetData::record_expiry`
  (every live route: monitor, lazy deadline, explicit, cascade, stale dependency),
  `expire_stored_copy` (cascade to a stored-only key) and the stored branch of
  `AssetManager::expire`. Emitting beside each call, not inside the overridable `record_expiry`,
  keeps events working for managers that override it.
- The live site runs under the asset's `data` write lock. `broadcast::Sender::send` is synchronous
  and never awaits, so it is allowed there (`ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` §8's rule).
- The event is sent when the manager's status changes. For a live keyed asset, the store follows
  a moment later (`mark_expired_status` writes after dropping the lock). An observer re-reading
  through the manager sees `Expired`, because the manager is the authority.
- Putting the sender in `DependencyManager`, which every `AssetManager` must own, gives external
  managers events with no code change (AC-7).
