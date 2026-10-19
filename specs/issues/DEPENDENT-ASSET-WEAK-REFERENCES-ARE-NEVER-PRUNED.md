---
id: DEPENDENT-ASSET-WEAK-REFERENCES-ARE-NEVER-PRUNED
kind: issue
title: The dependency manager never prunes dead weak dependents, so its lists grow with every distinct query
status: closed
priority: P3
complexity: S
area: [core/assets]
design: dependent-asset-weak-reference-pruning
created: 2026-10-17
github:
---
## Problem

`DependencyManager::track_asset` (`liquers-core/src/dependencies.rs`) registers every finished
non-keyed asset as a weak dependent of each of its recorded dependencies
(`add_dependent_asset`, a `Vec<WeakAssetRef>` per dependency key). The list is emptied only when
that dependency changes (`take_dependent_assets`) or is removed. Nothing removes a reference whose
asset has been dropped, and nothing deduplicates, so a list grows with every distinct query that
read the key, for as long as the key does not change.

## Problem Example

Evaluate 1 000 distinct queries that each read `-R/data/a.txt` and drop each asset. Today
`dependent_assets["-R/data/a.txt"]` holds at least 1 000 entries, all dead, until `data/a.txt`
changes. It should hold about as many entries as there are live dependents.

## Impact

Small today, because query assets record few dependencies. It becomes significant once an asset
records one dependency per file it read, as `design/search-read-dependencies` makes `ns-search` do.
*Q* distinct searches over *N* files hold about *Q·N* references per search step: 1 000 searches
over 5 000 files is about 10 M (`design/search-read-dependencies/performance.md`, finding 5).
Workaround: none short of restarting the process.

## Expected behaviour

Dead weak references are pruned, either amortized on insert (drop dead entries when a list grows
past a threshold) or when the dependent asset is evicted. The cost stays proportional to live
dependents.

## Discovery

Performance assessment of `design/search-read-dependencies` (2026-10-17).

## Resolution

2026-10-18: fixed by `design/dependent-asset-weak-reference-pruning/`. `DependencyManager::add_dependent_asset`
(`liquers-core/src/dependencies.rs`) now prunes a key's weak-dependent list on insert, when a push would
reallocate it (from 16 entries up): dead references and duplicates of one asset (judged by the shared
allocation, since temporary assets all have id 0) are dropped, and the list is grown when the prune leaves
it more than half full, so it holds at most about twice its live dependents with amortized constant
pruning work. Tests: `dependencies::tests::dependent_assets_prune_dead_entries_as_the_list_grows`,
`dependent_assets_list_one_asset_once`, `dependent_assets_keep_every_live_entry`.
