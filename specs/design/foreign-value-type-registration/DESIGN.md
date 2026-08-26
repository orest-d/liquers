---
id: FOREIGN-VALUE-TYPE-REGISTRATION
kind: design
title: Foreign and Python value types in the type registry
workflow: liquers-project
# No `status:` — `gh_pr` is set, so the implementation-related status is GitHub's to derive (§5.5).
phase: documentation
area: [core/value, lib/value, web, py]
gh_pr: [42]
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
- [x] Phase 3: Examples & Testing (approved)
- [x] Phase 4: Implementation Plan (in review)
- [x] Phase 5: Documentation (in review)
- [x] Implementation Complete (steps 1-9; Phase 5 outstanding)

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

2026-08-26 — Phase 3 approved. Phase 4 written: nine steps, one commit each, native first (steps
1-6) and `liquers-web` last (step 8) so the wasm toolchain and `cargo clean` are needed once.
Step 6 proves the fix; step 8 begins with a baseline run, since Phase 3's four-stale-assertions
count is derived from reading. Measured while planning: `liquers-py`'s test harness links and runs,
but pyo3 carries `extension-module` without `auto-initialize`, so `fvt6` must be GIL-free — Phase 3
had assumed ordinary unit tests. Natural stopping points recorded at steps 6 and 7; mid-step-7 is
not one, because a declared module that does not compile is worse than an undeclared one.

2026-08-26 — Phase 4 approved and **executed, steps 1-9**. All validations green: liquers-lib
302 + 14 suites, liquers-core 613, liquers-py 5 (newly testable), liquers-web 141 across 16
targets, `check-build-matrix.sh` 11/11, `registry_export` unmoved and
`specs/command_registry.yaml` unchanged in the diff.

Three things the plan did not predict, each recorded in its commit:

1. **`liquers-py` tests could not link.** Phase 4 measured an *empty* harness and concluded
   ordinary tests work; with real test code the linker fails on `_Py_Dealloc`, because pyo3's
   `extension-module` omits the Python symbols. Fixed with the recipe from the PyO3 guide —
   `extension-module` behind a default feature, `rlib` added to `crate-type` — so every ordinary
   build and the maturin wheel are unchanged while `--no-default-features --features async_store`
   links and runs. Without it the Python half would have shipped verified by nothing but
   `cargo check`.
2. **`ERROR-STATE-FROM-ERROR-NOT-STORABLE` (P1), filed not fixed** — and later deleted, see the
   entry below. `fvt7.5` failed for the wrong
   reason, which exposed that `Metadata::with_error` sets `type_identifier` to `error` but never
   sets `type_name`, and `sync_metadata_with_value` returns early for error states — so
   `State::from_error` produces a state no store accepts, with a message naming `type_name` rather
   than the error. Verified against a build.
3. **The wasm baseline was three failures, not one.** The issue's run aborted at the first failing
   binary and never reached `value_bridge_VALUE`. Phase 3's reading-derived prediction of four was
   right about all four, including that `second_value_type.rs:336` passed vacuously.

2026-08-26 — Mid-implementation the user directed a model correction: **there is no error type
identifier**. An errored state holds `V::none()` and is typed accordingly; the failure lives in the
metadata. Applied across `type_system`, `metadata`, `state` and `assets`, with eight test sites
updated — four of them written earlier in this branch. This dissolved
`ERROR-STATE-FROM-ERROR-NOT-STORABLE`, which was deleted rather than fixed: it described a symptom
of the error type, and the one-line fix would have entrenched it.

2026-08-26 — Phase 5 written. Reference and both guides updated, `CLAUDE.md` amended, three issues
closed, `PY-MODULES-NOT-DECLARED-IN-LIB` annotated. No new issue outstanding.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
