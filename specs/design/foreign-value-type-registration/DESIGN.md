---
id: FOREIGN-VALUE-TYPE-REGISTRATION
kind: design
title: Foreign and Python value types in the type registry
workflow: liquers-project
status: in_review
phase: examples
area: [core/value, lib/value, web, py]
gh_pr: []
issues: [FOREIGN-VALUE-TYPES-NOT-REGISTERED, PY-VALUE-TYPE-DESCRIPTIONS-MISSING, WEB-VALUE04-BYTES-IDENTIFIER-CASE-MISMATCH]
affects_docs: [VALUE-TYPE-SYSTEM, LANGUAGE-INTEGRATION_GUIDE, TYPE_SYSTEM_GUIDE]
created: 2026-08-26
superseded_by:
---
# foreign-value-type-registration Design Tracking

**Created:** 2026-08-26

## Phase Status

- [x] Phase 1: High-Level Design (in review — all questions decided)
- [x] Phase 2: Solution & Architecture (approved)
- [x] Phase 3: Examples & Testing (in review)
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

2026-08-26 — Last open question settled: a **string constant** is the single source of truth for a
type identifier, with a unit test asserting the static and instance spellings agree. No trait
machinery and no `debug_assert`. Rationale from the user: a few tens of types, each fixed once its
variant is implemented, so a correct implementation stays correct and a compile-time guarantee would
buy little. Follows the existing `ERROR_TYPE_IDENTIFIER` / `ORIGIN_JAVASCRIPT` practice.
**Phase 1 approved by the user.**

2026-08-26 — Phase 2 written. Known-issue preflight: 39 open records matched by area, eight
relevant, **no unresolved blocker**. `PY-MODULES-NOT-DECLARED-IN-LIB` is a prerequisite absorbed in
part (declare two modules, repair `value.rs`) rather than waited on. Architecture narrowed one
Phase 1 assumption — `liquers-py` needs no new constructor, because `Value::Py` is statically
describable, so five constructors take the registry rather than six — and widened another: the
`liquers-py` repair must add an `AssetInfo` variant, forced by a trait signature whose current
implementation is `todo!()`. Two items for the user: whether to fix the stale `bytes`/`Bytes`
assertion that leaves the liquers-web suite red at HEAD, and whether to add a diagnostic
"list known types" command.

2026-08-26 — Phase 2 approved. User confirmed the `bytes`/`Bytes` assertion fix is in scope; **no
diagnostic command** — this design stays at zero commands.

2026-08-26 — Phase 3 written: 13 unit tests, 5 integration tests in one new file, and two new
`liquers-web` `ENVIRON` checks. Three of the tests fail (do not compile) before the change; one
records the hard-refusal decision; `fvt5.1` **is** the constant-plus-test guarantee. Examples are
conceptual, with the integration test standing in for an `examples/` demo — this change has no
user-facing surface to demonstrate. Finding: `WEB-VALUE04` is understated — four stale assertions,
not one, and `second_value_type.rs:336` passes vacuously. Recorded in that issue.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
