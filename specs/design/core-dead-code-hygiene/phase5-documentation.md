# Phase 5: Documentation - Account for `entities.rs` and `cache.rs`

**Status: plan.** Written on 2026-10-05; executed after implementation.

## Completion Preconditions

- [ ] Phase 4 steps 1-3 complete and the Phase 3 checks pass
- [ ] User and review comments answered or incorporated

## Documentation Plan

### New Reference Documents

None.

### New Guide Documents

None.

### Existing Documents to Update (`affects_docs`)

| Document | Planned change |
|---|---|
| `reference/PROJECT_OVERVIEW.md` | `cache.rs` row (≈103) describes the module as legacy and names the removal issue; History row + `reviewed:` |

### Candidates Considered and Discarded

By area (`build`): `UNITTEST_GUIDE.md`, `autonomous_bulk_design.md`, `autonomous_issue_fixing.md` —
none mentions these modules. By content: `LANGUAGE-INTEGRATION_GUIDE.md` ≈244 already states the
module is legacy (correct, unchanged); `DOC_02_QUERY_LANGUAGE_REFERENCE.md` ≈98 describes
`entities` correctly (unchanged).

### Links and Capability Map

None.

### Issues to Close

`REPO-DEAD-CODE-HYGIENE` → `status: closed`, resolution carrying the Phase 3 evidence table and
naming `CORE-SYNC-STORE-TRAIT-OBSOLETE` as the owner of the deletion.

## Implementation Summary

*Pending.*

## Documentation Delivered

*Pending.*

## Issues Filed

*Pending.* None expected (the follow-up is recorded in the existing `CORE-SYNC-STORE-TRAIT-OBSOLETE`).

## Important Learning

*Pending.*

## Conformance and Remaining Work

*Pending.*

## Validation

*Pending.* Planned: `python3 scripts/docs_index.py --check`.
