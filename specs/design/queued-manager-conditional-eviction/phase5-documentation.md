# Phase 5: Documentation - Conditional Queued-Manager Cache Eviction

**Status: plan.** Written on 2026-10-05, when the design adopted the five-phase `liquers-project`
contract; executed after implementation, following
`.claude/skills/liquers-project/references/phase5-documentation.md`. The sections the skill requires
after implementation are present and marked *pending*.

## Completion Preconditions

- [ ] Implementation is finished and validated (Phase 4)
- [ ] All user comments are answered or incorporated
- [ ] All review comments are answered or incorporated
- [ ] Documentation is consistent with the implemented and tested behavior
- [ ] Documentation is included in the implementation PR

## Documentation Plan

### New Reference Documents

None: the change extends behaviour that existing documents own.

### New Guide Documents

None.

### Existing Documents to Update (`affects_docs`)

| Document | Planned change |
|---|---|
| `guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` | the `remove_expired_from_maps` row (≈143, "Drop the entry only if it is still the asset with that id"): add *atomically* — compare and remove in one map operation (`remove_if_async`, or one mutex guard) — since a manager outside core would otherwise copy the racy compare-then-remove pattern |

Each updated `reference/` or `guides/` document gets a `## History` row (source `phase-5`) and a
`reviewed:` bump in the same commit.

### Candidates Considered and Discarded

By area (`core/assets`): `ASSETS.md` (≈59 `query_assets`, ≈191 `remove_expired_from_maps`) and `ASSET_LIFECYCLE.md` describe *when* entries are evicted, which is unchanged; `ASSET_SET_OPERATION.md`, `DEPENDENCIES_STATUS.md`, `DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` do not describe eviction mechanics.

### Links and Capability Map

None needed in `specs/README.md`: the capability is reached through the documents above.

### Issues to Close

`QUEUED-MANAGER-EVICTION-RACE` → `closed`, resolution naming the tests and the structural proof (no unconditional stale removal remains).

## Implementation Summary

*Pending — written after implementation.*

## Documentation Delivered

*Pending.*

## Issues Filed

*Pending.*

## Important Learning

*Pending.* Seed: the key map was already serialized by `key_mutation_lock`; only the query map was racy.

## Conformance and Remaining Work

*Pending.*

## Validation

*Pending.* Planned: `python3 scripts/docs_index.py --check`.
