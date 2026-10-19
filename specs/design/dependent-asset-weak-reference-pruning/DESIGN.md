---
id: DEPENDENT-ASSET-WEAK-REFERENCE-PRUNING
kind: design
title: The dependency manager prunes dead and duplicate weak dependents as their lists grow
gh_pr: [158]
form: compact
phase: implementation
readiness: ready
autofix: eligible
area: [core/assets]
issues: [DEPENDENT-ASSET-WEAK-REFERENCES-ARE-NEVER-PRUNED]
created: 2026-10-17
---
# The dependency manager prunes dead and duplicate weak dependents as their lists grow

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-17 backlog compaction. Phases 1-4
are written but not approved, and nothing is implemented.

## Phase 1: High-Level Design

### Purpose

`DependencyManager::dependent_assets` (`liquers-core/src/dependencies.rs`) holds, per dependency
key, a `Vec<WeakAssetRef<E>>` of the non-keyed assets that read it. `add_dependent_asset` only
pushes. Entries leave the list only when the key changes or is removed (`take_dependent_assets`,
`remove`). Dead references (the asset was dropped) and duplicates (one asset pushed twice) stay.
Both happen often: `Context` pushes the evaluating asset when it schedules a dependency
(`liquers-core/src/context.rs`, the `add_dependent_asset` call in the dependency scheduling path),
and `track_asset` pushes a non-keyed asset again for each of its records when it finishes. Memory
use should follow the number of live dependents, not the number of queries ever evaluated.

### Problem Example

```rust
for i in 0..1000 {
    let q = format!("-R/data/a.txt/-/append-{i}");      // a distinct query each time
    envref.evaluate(&q).await?.get().await?;             // asset dropped after use
}
// dependent_assets["-R/data/a.txt"] now has >= 1000 entries, all dead.
```

Today the list keeps every entry until `data/a.txt` changes. It should hold a number of entries
proportional to the live dependents: at most about twice the live count.

### Scope and Acceptance Criteria

- **AC-1** Dead entries are pruned as the list grows
  - WHEN many dependents of one key are added and dropped while the key does not change
  - THEN the key's list length stays at most twice the number of live dependents plus a small
    constant (16), not the total number ever added
- **AC-2** One asset is listed once
  - WHEN the same asset is added as a dependent of one key several times
  - THEN after a prune it appears once, and an expiry of the key expires it once
- **AC-3** Expiry is unchanged
  - WHEN the key changes
  - THEN every live dependent is still expired, as today
- **AC-4** Insertion cost stays amortized constant
  - WHEN *n* live dependents are added to one key
  - THEN the total pruning work is O(*n*), not O(*n²*)

Non-goals: pruning lists of keys that are never pushed to again (their size is bounded by the
last prune, at most twice their live count at that time); the keyed edge maps.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — a bug fix inside the private body of
  `DependencyManager::add_dependent_asset` in `liquers-core`, no new structure, no `pub` change,
  one crate.
- **Leading issue:** None
- **Explanation:** `WeakAssetRef` already carries the asset id and its `Weak`, which is everything
  a prune needs; the change is local to one method, and expiry already filters dead entries.
- **Open questions:** None. Prune-on-eviction (the issue's other option) is rejected in Phase 2.

### Design Dependencies

- `overlaps` `replacing-write-drops-stale-dependency-edges` (weak: same file, different map).

### Consolidated Findings

Pruning happens on insert, triggered by the vector's own capacity, so no new field or struct is
needed. Duplicates come from the `Context` push and the `track_asset` push of one asset;
deduplicating by id during the prune removes them without a per-insert scan.

## Phase 2: Architecture

### Solution

In `add_dependent_asset`, while holding the entry: if `len() == capacity()` (the next push would
reallocate), first `retain` entries whose `data.strong_count() > 0` and whose `id()` has not been
seen in this pass (a local `HashSet<u64>`). If after the prune the list still fills more than half
its capacity, `reserve(capacity)`, so the next prune happens only after at least as many inserts as
there are entries. Then push. A minimum capacity of 16 avoids pruning tiny lists.

Rejected: prune on eviction (an `AssetData` drop does not know the dependency manager); a
per-insert duplicate scan (O(*n*) each, quadratic overall); a `HashMap<u64, WeakAssetRef>`
in place of the `Vec` (changes every reader of the field for no gain over amortized pruning).

Known-issue preflight: none open touches `dependent_assets`.

### Changes

- `liquers-core/src/dependencies.rs` `DependencyManager::add_dependent_asset`: the prune; a
  private helper `prune_dependent_assets(list: &mut Vec<WeakAssetRef<E>>)`.
- `#[cfg(test)]` helpers `dependent_asset_count(&self, key) -> usize` and
  `prune_dependents_now(&self, key)` (calls `prune_dependent_assets` on the entry) for the tests.
- `specs/reference/DEPENDENCIES_STATUS.md`: one line on the bound; History row.

No signature, command or serialized format changes. Synchronous code under the existing `scc`
entry guard; no new await points.

### Risks

A prune drops an entry only when its asset is gone or is listed earlier, so no live dependent can
be lost (AC-3). `Weak::strong_count` is a relaxed read; a racing drop is pruned next time.
Certainty: high.

## Phase 3: Examples and Tests

### Examples

The Problem Example; secondary: one asset added three times expires once.

### Tests

In `liquers-core/src/dependencies.rs` `mod tests`, using `AssetRef::new_temporary`:

- `dependent_assets_prune_dead_entries_as_the_list_grows` — add 1 000 temporary assets one by
  one, dropping each; the count stays below 2·live + 16 — AC-1, AC-4
- `dependent_assets_list_one_asset_once` — add one asset 40 times; the count never exceeds the
  minimum capacity (16) plus one, because duplicates are only bounded between prunes; then force a
  prune with the `#[cfg(test)]` `prune_dependents_now(key)` and assert the count is 1 and `expire`
  reports it once — AC-2
- `dependent_assets_keep_every_live_entry` — 100 live assets added with dead ones interleaved;
  `expire` reports all 100 — AC-3

Run: `cargo test -p liquers-core --lib dependencies::tests::dependent_assets`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-core/src/dependencies.rs` — the three failing tests and the `#[cfg(test)]`
  accessor — `cargo test -p liquers-core --lib dependencies::tests::dependent_assets` (AC-1, AC-2 fail)
- [x] 2. `DependencyManager::add_dependent_asset` and `prune_dependent_assets` — the capacity-
  triggered prune — same command, all pass. Rollback: revert the method body.
- [x] 3. `specs/reference/DEPENDENCIES_STATUS.md` — the bound, History row, `reviewed:` bumped;
  close the issue — `python3 scripts/docs_index.py --check`

Implementation note (2026-10-18): duplicates are judged by the shared allocation
(`Weak::as_ptr`), not by `id()`, because every temporary asset (`AssetManager::create_temporary_asset`)
has id 0 and deduplicating by id would drop live dependents. The tests build assets with
`AssetData::new_temporary(..).to_ref()`, because `AssetRef::new_temporary` starts a service-message
task that holds the asset strongly, so a dropped handle would never die.

Review note (2026-10-18, Codex on #158): a prune that leaves the list under a quarter of its
capacity now shrinks it to twice its length (at least 16), so the allocation also follows the
live count down after many dependents drop. Test: `dependent_assets_release_capacity_once_dependents_drop`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests` (search
catalogs are the heaviest users). Final diff review for scope.
