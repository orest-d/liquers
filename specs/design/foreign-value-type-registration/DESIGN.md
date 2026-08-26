---
id: FOREIGN-VALUE-TYPE-REGISTRATION
kind: design
title: Foreign and Python value types in the type registry
workflow: liquers-project
status: in_review
phase: high-level
area: [core/value, lib/value, web, py]
gh_pr: []
issues: [FOREIGN-VALUE-TYPES-NOT-REGISTERED, PY-VALUE-TYPE-DESCRIPTIONS-MISSING]
affects_docs: []
created: 2026-08-26
superseded_by:
---
# foreign-value-type-registration Design Tracking

**Created:** 2026-08-26

## Phase Status

- [x] Phase 1: High-Level Design (in review)
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

2026-08-26 — Phase 1 written. The refusal was reproduced natively with a mock `ForeignValue`
(`set_state` -> `[General] Type identifier 'js.Value' is not registered in this build`), settling
the issue's "not verified against a build" caveat. Filed `PY-VALUE-TYPE-DESCRIPTIONS-MISSING`
for the adjacent liquers-py gap found while reading the write path.

2026-08-26 — Phase 1 revised on user direction. `PY-VALUE-TYPE-DESCRIPTIONS-MISSING` is **in
scope**. The open question on registration shape is settled by the governing rule: a type
identifier corresponds one-to-one with a value variant, so the single `ExtValue::Foreign` variant
carries a single `TypeInfo` (`js.Value`) and there is no provider-family entry. Reviewing the docs
for that rule found it enforced by a test but never stated, and contradicted outright by the
`ValueInterface::identifier` doc comment (`liquers-core/src/value.rs:230`, "Several types can be
linked to the same identifier"); correcting the formulation is now a deliverable. Declaring
`liquers-py`'s `value`/`context` modules was measured: four compile errors in `value.rs`, so
repairing that file is inside scope.

2026-08-26 — Registry lifecycle settled by the user: the registry stays essentially constant and is
fixed once the environment is constructed. An integration extends the *existing* core/lib registry
and passes the finished one to the environment constructor; the `Environment` trait gains nothing
and there is no post-construction registration point. Six constructors take the new parameter, all
additively.

2026-08-26 — An unregistered identifier stays a **hard refusal**; the pre-`value-type-system`
degrade-to-metadata behaviour is not restored. Realm interaction (both sides holding a registry
complete for both realms, and identifying types no realm-crossing can carry — a JavaScript closure
has no transfer at all) is recorded as a forward constraint this design must not obstruct, and
`TYPE-REGISTRY-NOT-REALM-AWARE` was updated with it. One open question remains: how the static and
instance spellings of a foreign type's identifier are kept in agreement.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
