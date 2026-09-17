---
id: ASSET-EXPIRATION-EVENTS-CANNOT-BE-OBSERVED-EXCEPT-PER-ASSET
kind: feature
title: Asset expiration events cannot be observed except per asset
status: draft
priority: P1
complexity: M
area: [core/assets]
design: 
created: 2026-09-17
github:
---
## Problem

Expiration *is* notified — and there is still no way to watch for it.

`mark_expired_status` sends `AssetNotificationMessage::Expired` on the asset's notification channel
(`liquers-core/src/assets.rs:3237`), and `liquers-axum`'s assets WebSocket already maps that
variant (`liquers-axum/src/assets/websocket.rs:324`). But the only way to receive it is
`Asset::subscribe_to_notifications` (`assets.rs:1236`) or
`AssetRef::subscribe_to_notifications` (`assets.rs:3039`). Both require **already holding a
reference to the asset**, which inverts what an observer needs: it wants to be told *which* assets
expired, not to be told about assets it already knows about and is already holding.

There is no subscription on `AssetManager`. Three consequences, each independent:

1. **No scope subscription.** Nothing answers "notify me when anything under `some/folder` expires".
   An observer would have to hold an `AssetRef` for every asset it cares about, which pins them all
   in memory and still cannot cover assets that do not exist yet.
2. **Only live assets emit anything.** The cascade expires an asset only where
   `lookup_key_asset` finds one (`assets.rs:4331`). A keyed asset that is stored but not currently
   live has no channel, so its expiration is silent.
3. **The channel coalesces.** `notification_tx` is a `tokio::sync::watch`
   (`assets.rs:518`, `913`), which retains only the most recent value. A subscriber that is not
   continuously polling sees the latest state, not the sequence, so `Expired` followed by
   `Processing` then `Ready` is observed as `Ready` alone. This is correct for a UI that renders
   current status — which is what the channel was built for — and wrong for anything that must not
   miss an event.

## Impact

P1 because it blocks a designed capability rather than breaking an existing one, and because the
workarounds are all bad: poll every key's metadata, hold a reference to everything, or accept
unbounded staleness.

`design/store-and-asset-search/` needs exactly this. Its interoperability layer lets an external
search engine, vector store or SQL mirror be fed from a Liquers query and refreshed when sources
change; the refresh trigger is an expiration event for a scope. The design deliberately makes
reconciliation — not notification — the correctness guarantee, so this gap does not make it
*incorrect*. It makes every external view as stale as its polling interval, which is the difference
between a usable integration and a demonstration.

The same gap limits any out-of-process observer: a cache warmer, an audit log of invalidations, a
metrics exporter, or a second process mirroring a corpus.

## Expected behaviour

A manager-level event stream that an observer can subscribe to without holding the assets:

- **Scoped subscription** — by key prefix at minimum; a query pattern would be better if it can be
  made cheap.
- **Delivery that does not silently drop.** A broadcast channel with a bounded buffer and an
  explicit lag signal is the right shape: a slow observer is told it fell behind and resynchronizes,
  rather than being quietly served a coalesced view. "Resynchronize" is an acceptable answer
  precisely because reconciliation is the guarantee; silence is not.
- **Events for keyed assets that are not live**, so a cascade over stored-only keys is observable.
- **Enough payload to act**: which key or query, what it transitioned to, and ideally the version it
  moved away from.

Questions for the design:

- Whether this is a new manager-level channel or a generalization of the existing per-asset one, and
  whether the two should share a message type. `AssetNotificationMessage` carries per-asset progress
  variants that a scope subscriber has no use for.
- Whether subscription is in-process only, or whether `liquers-axum`'s WebSocket becomes the
  out-of-process form. The existing assets WebSocket already maps the variant and is the obvious
  delivery vehicle.
- What ordering, if any, is promised between events for different assets.
- How this interacts with `DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED`: an
  observer watching a folder for *added* entries depends on a directory-listing dependency that is
  currently recorded and dropped, so additions produce no cascade at all.
- Whether an expiration caused by the expiration monitor's timer is distinguishable from one caused
  by a dependency cascade. An observer may want to treat them differently.

## Discovery

Raised while designing `store-and-asset-search`'s interoperability layer, 2026-09-17. Verified at
HEAD: `AssetNotificationMessage::Expired` is sent at `assets.rs:3237`; the only subscribe methods
are at `assets.rs:1236` and `assets.rs:3039`, both on an asset rather than on a manager; the
channel type is `watch` at `assets.rs:518`; and the cascade's expiry loop is guarded by
`lookup_key_asset` at `assets.rs:4331`.
