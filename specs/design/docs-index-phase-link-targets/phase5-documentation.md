# Phase 5: Documentation - Resolvable Design-Phase Links in `index.md`

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
| `DOCS_STRUCTURE_GUIDE.md` | §7.2 item 9 ("What `--check` validates"): the dead-link check also covers the generated `specs/index.md`; History row (this document has no `reviewed:` field) |

Each updated `reference/` or `guides/` document gets a `## History` row (source `phase-5`) and a
`reviewed:` bump in the same commit.

### Candidates Considered and Discarded

By area (`docs`, `build`): `autonomous_bulk_design.md`, `autonomous_issue_fixing.md`, `CONFORMANCE_TERMS.md`, `API_DOCS_GAP_ANALYSIS.md`, `UNITTEST_GUIDE.md` — none describes the index generator or the link check. `DOCS_STRUCTURE_GUIDE.md` lives at `specs/` root, not under `reference/` or `guides/`; it is listed in `affects_docs` relative to `specs/` all the same, because it is the document this change makes wrong.

### Links and Capability Map

None needed in `specs/README.md`: the capability is reached through the documents above.

### Issues to Close

`DOCS-INDEX-EMITS-MACHINE-LOCAL-PATHS` → `closed`, resolution naming both tests and the regenerated phase links.

## Implementation Summary

*Pending — written after implementation.*

## Documentation Delivered

*Pending.*

## Issues Filed

*Pending.*

## Important Learning

*Pending.* Seed: a partial fix chose a different base path and created a second defect of the same kind; the general checker now guards the generated file.

## Conformance and Remaining Work

*Pending.*

## Validation

*Pending.* Planned: `python3 scripts/docs_index.py --check` (now including `index.md`); `python3 -m unittest scripts/test_docs_index.py`.
