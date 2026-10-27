---
id: RUNTIME-DEPENDENCY-RECORD-AGAINST-SUPERSEDED-VERSION-IS-NOT-CHECKED
kind: issue
title: A dependency record added at run time is never compared with a newer registered version
status: closed
priority: P3
complexity: M
area: [core/assets]
design: runtime-dependency-record-superseded-check
created: 2026-10-17
github:
---
## Problem

A command can record a dependency with the version it observed itself, through
`Context::add_dependency` (`liquers-core/src/context.rs`). Nothing compares that version with the
one the dependency manager holds when the record becomes an edge. A non-keyed asset becomes a weak
dependent at `DependencyManager::track_asset` (`add_dependent_asset`, no comparison); a keyed one
gets its edges through `load_from_records` → `add_dependency`, which only records.

So if the dependency is written after the command read its version and before the dependent is
tracked, the write's cascade finds no dependent, the edge is born recording a superseded version,
and the dependent finishes `Ready` with stale content until the dependency changes again. The check
that catches exactly this race exists only for `AssetRef::enter_dependencies`
(`design/dependency-edge-superseded-version/`, complete), which covers dependencies waited for, not
records a command adds.

Inferred from reading the code; not reproduced (the window is narrow).

## Problem Example

A keyed recipe's command reads version `v1` of `data/d.txt` and records it with
`context.add_dependency`. Before the run completes, `d` is written (`v2`). The run finishes `Ready`
with an edge expecting `v1`. It should finish `Expired` with `Direct { StaleDependency }`.

## Impact

Rare stale results for commands that record their own reads: `ns-search` after
`design/search-read-dependencies`, and any future command of that kind. Workaround: the next change
of the same dependency expires the dependent.

## Expected behaviour

When a runtime record with a concrete version becomes an edge (or a weak dependent) and the manager
holds a concrete, different version, the dependent takes the stale-dependency route
(`Direct { StaleDependency }`), as `enter_dependencies` does.

## Discovery

Designing `design/search-read-dependencies` (2026-10-17), whose search commands record a version
read from the store and then read the bytes.

## Triage (2026-10-20)

Still present at HEAD 18732dd by reading (`AssetRef::complete_evaluation` merges the records, and
`track_keyed_asset` / `track_asset` never compare them). Designed in
`design/runtime-dependency-record-superseded-check/`.

## Resolution

Fixed 2026-10-27 by `design/runtime-dependency-record-superseded-check/`. `AssetRef::complete_evaluation` now compares the run's dependency records with the dependency
manager's versions (`DependencyManager::superseded_record`): before finalization, a superseded
record takes the stale route (`AssetData::note_stale_dependency`), and after `track_asset` a write
that landed in between expires the asset (`expire_if_superseded`). Tests:
`added_record_superseded_before_completion_marks_stale`,
`added_record_with_current_version_stays_ready`, `added_record_with_unknown_version_stays_ready`,
`query_asset_with_superseded_added_record_is_expired` (`liquers-core/tests/dependency_manager_integration.rs`),
`superseded_after_tracking_expires_the_asset` (`assets.rs`) and
`superseded_record_skips_unknown_and_absent` (`dependencies.rs`).
