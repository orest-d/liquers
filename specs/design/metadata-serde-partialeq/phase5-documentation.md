# Phase 5: Documentation - Serde and Equality for `Metadata`

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

None — see the candidates below for why. `affects_docs` is `[]`.

### Candidates Considered and Discarded

By area (`core/value`): `VALUE_TYPE_SYSTEM.md` and `TYPE_SYSTEM_GUIDE.md` describe value types and data formats, not `Metadata`'s Rust traits; `PROJECT_OVERVIEW.md` lists modules only. By content: `WEB_API_SPECIFICATION.md`, `ASSET_SET_OPERATION.md` and `ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` mention `LegacyMetadata` and the stored JSON form, which this change keeps byte-identical. So `affects_docs` is empty: no current document makes a claim this change alters. Reconsider at execution if a document gained such a claim.

### Links and Capability Map

None needed in `specs/README.md`: the capability is reached through the documents above.

### Issues to Close

`METADATA-LACKS-SERIALIZE-DESERIALIZE-AND-PARTIALEQ` → `closed`, resolution naming the five tests.

## Implementation Summary

*Pending — written after implementation.*

## Documentation Delivered

*Pending.*

## Issues Filed

*Pending.* Candidate: `ChunkDescriptor` serialization (needs a `Query` wire-form decision) — file only when a consumer needs it.

## Important Learning

*Pending.*

## Conformance and Remaining Work

*Pending.*

## Validation

*Pending.* Planned: `python3 scripts/docs_index.py --check`.
