# Phase 5: Documentation - Documenting `payload:`, `expires:` and `version:`

**Status: plan.** Written on 2026-10-05; executed after implementation. For a documentation-only
design, implementation *is* most of Phase 5: this plan fixes what is reviewed and recorded.

## Completion Preconditions

- [ ] Phase 4 steps 1-4 complete
- [ ] User and review comments answered or incorporated
- [ ] Documentation consistent with the macro as implemented

## Documentation Plan

### New Reference Documents

None.

### New Guide Documents

None.

### Existing Documents to Update (`affects_docs`)

| Document | Planned change |
|---|---|
| `reference/REGISTER_COMMAND_FSD.md` | three table rows, example block, §Injected Parameters link, "Implementation versions" subsection; History row + `reviewed:` |
| `guides/COMMAND_REGISTRATION_GUIDE.md` | DSL bullet, two task recipes; History row + `reviewed:` |

`CLAUDE.md` is updated too but is outside `specs/` and carries no History table.

### Candidates Considered and Discarded

By area (`macro`, `core/commands`, `docs`): `COMMAND_DECLARATION.md` (owns `metadata_version`, says
`impl_version` is supplied at registration — still true), `PAYLOAD_GUIDE.md` (already correct),
`LANGUAGE-INTEGRATION_GUIDE.md`, `DOC_08_RECIPES_PLANS.md` (linked, not changed).

### Links and Capability Map

None needed: the FSD is already the map's target for the macro.

### Issues to Close

`REGISTER-COMMAND-PAYLOAD-STATEMENT-UNDOCUMENTED` (note that `PAYLOAD_GUIDE.md` already covered it)
and `REGISTER-COMMAND-EXPIRES-AND-VERSION-STATEMENTS-UNDOCUMENTED` → `status: closed` with resolutions.

## Implementation Summary

*Pending.*

## Documentation Delivered

*Pending.*

## Issues Filed

*Pending.* None expected.

## Important Learning

*Pending.*

## Conformance and Remaining Work

*Pending.*

## Validation

*Pending.* Planned: `python3 scripts/docs_index.py --check`.
