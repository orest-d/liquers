---
id: ASSET-TO-OVERRIDE-SOURCE-INCONSISTENT
kind: issue
title: AssetManager::to_override turns a stored-only Source into a recipe-less Override
status: draft
priority: P3
complexity: S
area: [core/assets]
design:
created: 2026-09-27
github:
---
## Problem

`AssetRef::to_override` leaves a `Source` unchanged (`assets.rs`, "Directory, Source: No
change"). The store-only branch of `AssetManager::to_override` (trait default and
`DefaultAssetManager`) only checks `has_data()` and rewrites the stored status to `Override`, so a
`Source` that is not live becomes an `Override` with no recipe behind it. The same call gives two
different results depending on whether the asset happens to be in memory.

## Impact

Low. An `Override` without a recipe behaves like a `Source` for reads, but status-based decisions
treat it differently: `remove` / `expire` / `set_description` (the latter refuses non-`Source`)
under `axum-assets-endpoints`, and `POST override` exposes the call over HTTP.

## Expected behaviour

Either both branches leave a `Source` unchanged, or both refuse it (`StatusConflict`). Not decided.

## Discovery

Final cross-phase review of `specs/design/axum-assets-endpoints/`, 2026-09-27: its AAE30 test
originally asserted a 200 for `POST override` on a stored `Source`; the test was moved to a
computed value instead.
