# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** The issue states the expected behaviour, and the codebase already has the route
  it asks for. A dependent that is still evaluating and finds a dependency stale takes the
  stale-dependency path (`AssetRef::note_expired_dependency`, finished as `Expired` with
  `StaleDependency`, recomputed on next access). The check goes where the edge is recorded during
  evaluation. `DependencyManager::add_dependency` stays a pure recorder, as a test deliberately pins
  (`add_dependency_records_a_disagreeing_version_without_expiring`).
- **Open questions:** None

## Problem

When an evaluating asset records its edge to a dependency (`AssetRef::enter_dependencies`,
`liquers-core/src/assets.rs` ≈1930–1990), it records the version the dependency's value carries
(`dependency.metadata.version()`), and nothing compares it with the version the dependency manager
currently holds. If the dependency changed after the dependent read it but before the edge was
recorded, the cascade for that change found no edge, and the edge is born pointing at a superseded
version. The dependent then finishes `Ready` with a stale value. The issue's example is a directory
listing that registers `L1`, a concurrent `set_binary` that registers `L2`, and an index built from
`L1`.

## Expected behaviour and acceptance

1. When `enter_dependencies` records an edge with a concrete version `v` and the map holds a
   concrete, different current version for the dependency, the dependent is marked through the
   stale-dependency route. It finishes `Expired` with reason `Direct{StaleDependency{dependency}}`,
   and the next request recomputes it.
2. If either version is unknown (`Version(0)`), nothing happens (no evidence).
3. Equal versions: nothing happens.
4. The dependent is never expired "from under itself" while running. The route only labels it for
   finalization (the existing behaviour of `note_expired_dependency`).
5. `add_dependency` keeps its recording-only contract, and its pinning test stays.

## Scope

`enter_dependencies` only. The other recording site (`register_plan_dependencies`-style loop at
`assets.rs` ≈6040) records the map's own current version, so it cannot disagree.

## Design Dependencies

- `dependency-chain-analysis-cost` — **overlaps** (same area, independent).
- `stale-dependency-status-finalization` (complete) — **overlaps**. It defines the route used here.

## Documentation assessment

- Reference: `specs/reference/DEPENDENCIES_STATUS.md`, edge recording: "an edge recorded against a
  version the map has already replaced marks the dependent stale (the stale-dependency route)".

## Consolidated Findings

- `note_expired_dependency(dependency)` takes the dependency `AssetRef`, which `enter_dependencies`
  already has. It reads the dependency's provenance key under its own lock and then labels the
  parent. It is reused as is.
- The race window is narrow, so the test makes it deterministic: register a newer version in the
  dependency manager between the dependency's completion and the dependent's `enter_dependencies`
  call (a test-only hook is not needed, because the test can call `register_version` directly on the
  manager before invoking the dependent's wait).
