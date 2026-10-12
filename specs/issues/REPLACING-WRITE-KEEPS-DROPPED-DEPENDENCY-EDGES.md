---
id: REPLACING-WRITE-KEEPS-DROPPED-DEPENDENCY-EDGES
kind: issue
title: A write that replaces a keyed asset keeps the dependency edges its new metadata no longer lists
status: draft
priority: P3
complexity: S
area: [core/assets]
created: 2026-10-11
github:
---

## Problem

`DependencyManager::track_keyed_asset` (`liquers-core/src/dependencies.rs`) registers the
dependency records in the new metadata through `load_from_records`, which only adds or updates
edges. It never removes an edge the key held before. So when `set_state` (built-in
`DefaultAssetManager` / `ImmediateAssetManager`, or an external manager through
`AssetManager::track_installed`) replaces an `Override` whose old metadata listed dependency `d`
with metadata that does not, the edge `d → key` survives. A later write to `d` still expires the
key, although its current value no longer depends on `d`.

The effect is a spurious expiry, never a stale value: the asset is recomputed or re-read when it
did not need to be.

Found by an automated review of PR #118, which added `track_installed`; the built-in managers
behave the same way.

## Expected

Registering a keyed asset's records replaces the key's incoming edges with the ones its metadata
lists: clear the key's previous incoming edges (each dependency's `keyed_dependents` entry for this
key) before loading the new records.

## Notes

The reverse index needed to find "every dependency this key is an edge of" may not exist;
`keyed_dependents` is indexed by dependency. Check whether a scan is acceptable or a reverse map is
needed.
