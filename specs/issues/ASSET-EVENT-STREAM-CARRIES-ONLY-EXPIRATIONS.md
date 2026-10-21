---
id: ASSET-EVENT-STREAM-CARRIES-ONLY-EXPIRATIONS
kind: feature
title: The manager event stream announces expirations only, not new versions, additions or removals
status: draft
priority: P3
complexity: M
area: [core/assets]
design:
created: 2026-10-21
github:
---

## Problem

The manager-level event stream designed in `external-index-sync` sends `AssetEvent::Expired` only.
A write that publishes a new version (`AssetManager::publish_version`, `track_installed`, an
evaluation registering its version), a file added to a folder (`refresh_listing_version`) and a
removal (`AssetManager::remove`) are not announced. An observer learns about them only indirectly,
when something that depended on them expires. A folder with no dependents produces no event when a
document is added.

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
