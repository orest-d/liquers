---
id: ASSET-EVENT-STREAM-CARRIES-ONLY-EXPIRATIONS
kind: feature
title: The manager event stream announces expirations only, not new versions, additions or removals
status: draft
priority: P3
complexity: M
area: [core/assets]
design: asset-event-kinds
created: 2026-10-21
github:
---

## Problem

The manager-level event stream designed in `external-index-sync` (Phase 4 written, not yet
implemented at `cd6d5da`) sends `AssetEvent::Expired` only. A new version — a write
(`AssetManager::publish_version`, which every manager's `set_state` / `set_binary` calls), a keyed
evaluation entering the dependency graph (`track_asset` in `AssetRef`'s finish path), an outside
change adopted by `apply_external_change` — and a deletion (`AssetManager::remove`'s `Delete` arm,
the corrupted-file deletion in `apply_external_change`) are not announced. An observer learns
about them only indirectly, when something that depended on them expires. A folder with no
dependents produces no event when a document is added (`refresh_listing_version` computes a
listing version only when one is registered).

## Problem Example

```rust
let mut events = m.subscribe_asset_events(AssetEventScope::Under(parse_key("docs")?));
m.set_state(&parse_key("docs/a.txt")?, State::from("first draft")).await?;
m.remove(&parse_key("docs/b.txt")?).await?;   // a Source with no recipe
```

With `external-index-sync` landed, `events.try_recv()` returns `None` after both calls. It should
yield an "updated" event for `docs/a.txt` with its new version, then a "removed" event for
`docs/b.txt`.

## Impact

An index feeder learns about new documents only by reconciling. Workaround: periodic reconciliation,
which is the correctness guarantee anyway.

## Expected behaviour

Further `AssetEvent` variants, sent from the version-publishing, listing-refresh and removal
primitives: for example `Updated { subject, version }`, `ListingChanged { dir, version }` and
`Removed { subject }`. Adding a variant is a breaking change for exhaustive matches, by design
(`external-index-sync` Phase 1, question 5).

## Discovery

Split off on 2026-10-21 during bulk design of `external-index-sync`, which limits its first slice to
the expiry events its source issue asks for.
