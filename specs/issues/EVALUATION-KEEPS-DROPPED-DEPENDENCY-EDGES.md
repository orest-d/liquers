---
id: EVALUATION-KEEPS-DROPPED-DEPENDENCY-EDGES
kind: issue
title: Re-evaluating an asset adds dependency edges but never removes the ones it no longer has
status: draft
priority: P3
complexity: M
area: [core/assets]
design: keyed-run-replaces-dependency-edges
created: 2026-10-17
github:
---
# Re-evaluating an asset adds dependency edges but never removes the ones it no longer has

## Problem

`replacing-write-drops-stale-dependency-edges` fixes the write path: a replacing `set_state` clears
the asset's incoming `keyed_dependents` edges before recording the new ones. The evaluation path has
the same additive gap: when a keyed asset is re-evaluated and its new dependency records no longer
name a dependency, the old edge in `keyed_dependents` (`liquers-core/src/assets.rs`) stays, so a
later change to the dropped dependency still expires the asset. The write-path fix was scoped
narrowly because schedule-time attribution edges (recorded before the command runs) may not appear
in the asset's final dependency records, so clearing by records alone could drop a live edge.

## Problem Example

Recipe `r` reads `-R/a` on its first run and `-R/b` on its second (the switch is data-driven).
After the second evaluation, writing `-R/a` still expires `r`; it should not, because `r` no
longer depends on `a`.

Note (2026-10-20): the re-evaluation must have a cause other than a change of `a`. A change of `a`
removes `a`'s edge map in its own cascade. In the design's example, a third input `c` selects
which of `a` and `b` is read, and changing `c` triggers the re-run.

## Expected behaviour

After an evaluation, the asset's incoming edges match its current dependency records plus any
schedule-time edges that are still live.

## Discovery

Found on 2026-10-17 while designing `REPLACING-WRITE-KEEPS-DROPPED-DEPENDENCY-EDGES` in the backlog
compaction (`specs/design/replacing-write-drops-stale-dependency-edges/`, Phase 2 scope note).

## Triage (2026-10-20)

Still reproduces at HEAD 18732dd by reading: `track_keyed_asset` / `load_from_records` only add, and
`remove_dependent_edges` is called only from `track_replacing_asset` and `AssetRef::set_volatile`.
Designed together with `EXPRESSION-ATTRIBUTION-MAPS-ARE-NEVER-CLEANED` in
`design/keyed-run-replaces-dependency-edges/` (strong overlap: T2 and T3).
