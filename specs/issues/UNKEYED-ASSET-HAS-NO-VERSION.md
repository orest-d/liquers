---
id: UNKEYED-ASSET-HAS-NO-VERSION
kind: feature
title: A keyless query asset has no version, so nothing can tell whether it changed without re-evaluating it
status: draft
priority: P3
complexity: M
area: [core/assets, records]
design: unkeyed-asset-derived-version
created: 2026-10-26
github:
---

## Problem

`AssetRef::prepare_version` (`liquers-core/src/assets.rs`) returns `None` for a non-keyed asset,
deliberately: such an asset is not a dependency-graph node, and serializing every query result to
hash it would tax the commonest path. So `metadata.version()` is `None` for every keyless query
result, and a dependency on one is recorded with an unknown version.

The analysis in `design/external-index-sync/indexation-policy.md` §4-§5 proposes a version that
needs no serialization: a hash over the plan's dependency versions and command implementation
versions, decidable before evaluation. Nothing computes it.

## Problem Example

A manifest whose chunks are unkeyed queries (`ChunkId::Query`, e.g.
`-R/data/sales.csv/-/ns-pl/eq-region-EU`) is reconciled against an external index
(`design/external-index-reconciliation/`). `describe_chunk` returns metadata with no version for
every such chunk, so the reconciliation cannot tell a changed chunk from an unchanged one and falls
back to re-feeding it on a schedule. Should be: the chunk's version changes exactly when one of its
inputs' versions (or a command implementation version) changes, so an unchanged chunk is skipped.

## Impact

External-index reconciliation, and any consumer comparing versions, treats every unkeyed chunk as
time-based: more evaluation and more writes than necessary, and staleness bounded only by the
schedule. Workaround: give chunks keys (a manifest with keyed chunks, or `recipes.yaml` entries).

## Expected behaviour

A derived version for a keyless, non-volatile asset, computed from its recorded dependencies'
versions and the command versions in its plan, without serializing the value; `None` stays the
answer for a volatile asset and for one with an unversioned dependency. Open: whether it is
computed before evaluation (offline, from the plan, as `liquers-validate` plans) or after (from the
dependency records), and whether it must be distinguishable from a content version (bit 127 marks
content hashes today).

Complexity corrected from `L` to `M` by the bulk design (2026-10-26): every input the version needs
is already in the asset's metadata at finalization, so the change stays in `liquers-core`
(`prepare_version` plus a guard keeping dependency-manager edges to queries at
`Version::unknown()`, as `DEPENDENCIES_STATUS.md` requires).

## Discovery

Found on 2026-10-26 while designing `external-index-reconciliation` (bulk design, cluster I): the
reconciliation's version diff reads `describe_chunk(...).metadata.version()`, which is `None` for
every `ChunkId::Query` chunk at HEAD cd6d5da. The need was first written down in
`indexation-policy.md` §5 ("What is missing at HEAD"), where no issue was filed.
