# Phase 5: Documentation - Compiler-Checked `ErrorType` List for OBJECT06

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

By area (`web`): `WEB_API_SPECIFICATION.md` ≈138 concerns the axum `ErrorType` → HTTP mapping, not this test; `LANGUAGE-INTEGRATION_GUIDE.md`, `STORE_SEMANTICS.md`, `STORE_IMPLEMENTATION_GUIDE.md`, `RECORD_STREAMS.md`, `COMMAND_DECLARATION.md` do not mention the list. `liquers-web/README.md` (outside `specs/`) does not mention a variant count. The frozen `design/liquers-web/` is not edited. So `affects_docs` is empty: a test-only change.

### Links and Capability Map

None needed in `specs/README.md`: the capability is reached through the documents above.

### Issues to Close

`WEB-OBJECT06-EXPECTS-A-STALE-ERROR-TYPE-COUNT` → `closed`, resolution noting the missing `KeyNotAbsolute`.

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

*Pending.* Planned: `python3 scripts/docs_index.py --check`; the wasm Node loop result (or a statement that the toolchain was unavailable).
