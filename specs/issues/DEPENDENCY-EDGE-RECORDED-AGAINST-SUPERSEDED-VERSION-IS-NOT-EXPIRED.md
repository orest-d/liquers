---
id: DEPENDENCY-EDGE-RECORDED-AGAINST-SUPERSEDED-VERSION-IS-NOT-EXPIRED
kind: issue
title: A dependent that records an edge against a version the map has already replaced stays Ready
status: closed
priority: P3
complexity: M
area: [core/assets]
design: dependency-edge-superseded-version
created: 2026-10-04
github:
---

## Problem

`DependencyManager::add_dependency` records the version the dependent observed and never compares
it with the version the map currently holds for the dependency. Only a later *change* of the
dependency (`register_version`, `register_written_version`, `audit_version`) compares edges. So if
the dependency changes after the dependent read it but before the dependent's edge is recorded, the
cascade of that change finds no edge to expire. The edge is then recorded against a version that is
already superseded, and nothing in this process notices.

Example, with a folder listing (`-R-dir/data`) and an index built from it:

1. The directory step reads `data` and registers listing version `L1`. Since
   orest-d/liquers#75 it also re-reads once to catch a write that landed before registration.
2. `set_binary(data/late.txt)` runs after that last read. The listing is registered, so
   `refresh_listing_version` registers `L2` and cascades. The listing asset is still evaluating, so
   it is not yet a tracked dependent, and the index has no edge yet: nothing expires.
3. The listing finishes with a value built for `L1`. The index records `index -> -R-dir/data @ L1`
   and finishes `Ready`, without `late.txt`.
4. A later request for the index is served from the live asset. Only a fast track after a restart
   (which checks edges against the map) or an explicit audit finds it stale.

The window is narrow, the time between the dependency's last read and the dependent recording its
edge. A keyed dependency mostly closes it another way: the write replaces the dependency's asset,
so a dependent still waiting on the old one sees it `Expired` and takes the stale-dependency path.
A query asset such as the listing has no such replacement.

## Impact

A stale derived value can be served in-process after a concurrent change. It is not persisted as
fresh across a restart: an audit or `OnLoad` check catches it there.

## Expected behaviour

When an edge is recorded, compare its version with the map's current version for the dependency.
If the map holds a different concrete version, the dependent is stale at birth: expire it (cause
`Updated`, root the dependency), or return it so the caller can. Care is needed for the evaluation
path, where a dependent that is still running must take the stale-dependency route rather than be
expired from under itself (see `design/stale-dependency-status-finalization/`).

## Discovery

Found 2026-10-04 while fixing a Codex review finding on orest-d/liquers#75 (the directory step
versioned a different `listdir` call than the one that produced its value). Re-reading the listing
after registration closed the window before registration. This window remains after it.

## Resolution (2026-10-07)

Fixed by design `dependency-edge-superseded-version`. When an evaluating asset records its edge
(`AssetRef::record_dependency_on_asset`), it now compares the version it observed with the map's
current version for the dependency. If both are concrete and differ, the dependent takes the
stale-dependency route and finishes `Expired` with `Direct { StaleDependency }`, to be recomputed on
next access. `DependencyManager::add_dependency` stays a pure recorder.

Evidence: `edge_against_superseded_version_marks_dependent_stale`,
`edge_with_unknown_version_marks_nothing`, `edge_with_current_version_marks_nothing`
(`liquers-core/src/assets.rs`).
