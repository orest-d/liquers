# Phase 5: Documentation - Markdown as a `Text` Data Format

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
| `guides/TYPE_SYSTEM_GUIDE.md` | §Choosing a data format at write time (≈198): one line noting that `md` on a `Text` writes the text as is (plain markdown), while `md` on a `RecordView` renders a markdown table (≈138) — the same extension is valid on several types, each with its own meaning |

Each updated `reference/` or `guides/` document gets a `## History` row (source `phase-5`) and a
`reviewed:` bump in the same commit.

### Candidates Considered and Discarded

By area (`core/value`, `lib/value`): `VALUE_TYPE_SYSTEM.md` enumerates per-type formats only for the record identifiers (≈103), not for base types, so it makes no claim this changes; `PROJECT_OVERVIEW.md` lists modules only.

### Links and Capability Map

None needed in `specs/README.md`: the capability is reached through the documents above.

### Issues to Close

`TEXT-VALUE-CANNOT-BE-STORED-AS-MARKDOWN` → `closed`, resolution naming the tests.

## Implementation Summary

*Pending — written after implementation.*

## Documentation Delivered

*Pending.*

## Issues Filed

*Pending.*

## Important Learning

*Pending.*

## Conformance and Remaining Work

*Pending.*

## Validation

*Pending.* Planned: `python3 scripts/docs_index.py --check`.
