---
id: REPLACING-WRITE-KEEPS-DROPPED-DEPENDENCY-EDGES
kind: issue
title: A write that replaces a keyed asset keeps the dependency edges its new metadata no longer lists
status: closed
priority: P3
complexity: S
area: [core/assets]
design: replacing-write-drops-stale-dependency-edges
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

## Problem Example

`set_state(k)` with metadata recording `-R/data/d.txt`, then `set_state(k)` again with metadata
recording nothing, then a write of `data/d.txt`. Today `k` is expired (`Cascaded`, root
`-R/data/d.txt`). It should stay `Override`.

## Expected

Registering a keyed asset's records replaces the key's incoming edges with the ones its metadata
lists: clear the key's previous incoming edges (each dependency's `keyed_dependents` entry for this
key) before loading the new records.

## Notes

The reverse index needed to find "every dependency this key is an edge of" may not exist;
`keyed_dependents` is indexed by dependency. Check whether a scan is acceptable or a reverse map is
needed.

## Resolution

2026-10-18. A write now replaces the key's incoming dependency edges instead of adding to them
(`design/replacing-write-drops-stale-dependency-edges/`). `DependencyManager::track_replacing_asset`
(`liquers-core/src/dependencies.rs`) clears the key's incoming edges with the existing
`remove_dependent_edges` (which also takes the key out of the attribution sets of the expressions its
previous run scheduled) before loading the new records; both built-in `set_state`s and the default
`AssetManager::track_installed` call it. The evaluation path still adds edges. A run still in flight
when its key is replaced cannot put an edge back: after a schedule-time registration,
`Context::schedule_dependency_asset_with_key` (`liquers-core/src/context.rs`) checks for
cancellation, undoes what it registered with the new
`DependencyManager::unregister_scheduled_dependency` (keeping the edges the replacement's records
name), and fails the request as cancelled. `set_binary` still loads no records and is unchanged.

Tests: `dependencies::tests::remove_dependent_edges_removes_only_that_dependent`,
`dependencies::tests::unregister_scheduled_dependency_undoes_a_keyed_and_an_expression_request`,
`dependency_manager_integration::replacing_set_state_drops_unlisted_dependency_edges`,
`dependency_manager_integration::replaced_run_cannot_restore_a_cleared_edge`,
`external_asset_manager::external_manager_replacing_install_drops_unlisted_edges`.
