# Phase 5: Documentation - Plan CWD Freeze

## Implementation Summary

**Requested:** diagnose `CORE-RECIPES-EXPAND-PREDECESSORS-CRASH` (P0) — a commented-out
`disable_expand_predecessors()` call in `recipes.rs` with a TODO saying it crashed a test.

**Found:** nothing crashed. Enabling the option produced 11 test failures from four causes, of
which the named one was the least serious: the test's `word` command omitted `payload: required`,
which is the documented "declare it, or lose it" rule. The real defects were structural, and the
root problem was not the option at all — CWD-relative operands were resolved independently by three
passes, each with its own cursor that had to agree with the others.

**Implemented:** the design was rescoped around that. `Plan::freeze_cwd` resolves every relative
operand once, in execution order, before dependency analysis; the predecessor cut became a
post-freeze policy rather than a builder option.

| Area | Change |
|---|---|
| `plan.rs` | `Plan::{frozen_cwd, predecessor, predecessor_steps}`; `freeze_cwd`, `freeze_cwd_with`, `cut_predecessor`; `ResolvedParameterValues`/`ParameterValue::freeze_cwd`; `promote_relative_default_links`; `expand_predecessors` and its two methods removed |
| `query.rs` | `Query::has_relative_operand`, `first_relative_operand_position`; `CwdCursor::{consumed_cwd, take_consumed_cwd, absorb_diagnostics}`; `is_relative` made crate-visible |
| `interpreter.rs` | `finalize_plan` freezes before analysis, warning only when the root fallback is used |
| `context.rs` | `get_cwd_key`/`set_cwd_key` are `pub(crate)`; `evaluate`/`apply`/`get_dependency_state` reject relative queries |
| `value.rs`, `commands.rs` | `TryFrom<Value> for Key`, `FromParameterValue<Key>` — what makes `-R-key/.` consumable |
| `assets.rs` | `AssetRef::stored_error`; a dependency's cause is surfaced instead of "did not produce a value" |
| `recipes.rs` | `predecessor_steps` bumped across the CWD prefix; the dead marker deleted |

Tests: `tests/plan_cwd_freeze.rs` (7), five freeze unit tests, three equivalence tests with the
`evaluate_both_ways` harness, one promotion-slot test, six migrated call sites. **No test deleted.**

Verification: `liquers-core` all 16 suites green, `liquers-lib` green, `docs_index.py --check`
0 errors, CI green on PR #35.

## Deviations from the approved design

| Deviation | Reason |
|---|---|
| `freeze_cwd(Option<Key>) -> Result<(Key, bool), Error>`, not `(&Key) -> Result<Key, Error>` | Phase 2's signature forced `finalize_plan` to install logical root eagerly, which warned even for plans with no relative operand and broke `test_to_override_skips_store_write_when_nonserializable`. The caller must own the warning. |
| A dependency's error is surfaced unchanged, not rebuilt | Phase 4 specified `Error::from_error` plus context carry-over. That double-wraps, because `from_error` stores the cause's rendered form which already carries command and position. Found in review. |
| Promotion skips at an argument gap | Phase 2 assumed appending was sufficient. It is only correct when every earlier slot is written; at a gap the link binds to the wrong argument. Materializing earlier defaults is not always possible (a placeholder or injected argument has nothing to write), so the query is recorded unpromoted. Found in review. |
| Phase 3's E1-E12 suite is partial | Three shapes and the harness landed. Cutting is not yet equivalent, so the remaining shapes would encode current behaviour rather than intended behaviour. Tracked as an issue. |

## Documentation Delivered

### New Reference Documents

None. Phases 1 and 2 both decided freezing belongs beside the existing plan contract; nothing in
implementation changed that.

### New Guide Documents

None. The one candidate — "how do I write a command that needs its directory?" — is a section in
the existing registration guide, not a guide of its own.

### Existing Documents Reviewed or Updated

| Path | Change |
|---|---|
| `specs/reference/api/DOC_08_RECIPES_PLANS.md` | New "Freezing" and "Predecessor boundaries" sections: what freezing is, the three-cursor problem, when it runs, mechanics and scope rules, how cutting differs, the dependency/caching/parallelism case for making a predecessor available, and five observed pitfalls. `disable_expand_predecessors` removed from the planning contract; the serialization section updated. |
| `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` | Why the working key is crate-private — identity, not resolution — and that `evaluate`/`apply`/`get_dependency_state` refuse relative queries. |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | "Passing the working directory (or any relative query) into a command": `-R-key/.` as a default link, building absolute queries from it, and the argument-order pitfall. |
| `specs/reference/PAYLOAD_GUIDE.md` | Reviewed; no change needed. Its "declare it, or lose it" note already states the rule, and DOC-08's pitfall table now points at where a boundary makes it bite. |

All four queries quoted in the guide were checked with `liquers-validate`: 4 ok, 0 warnings,
0 errors.

## Issues Filed

| Issue | Priority | State |
|---|---|---|
| `CWD-KEY-LINK-NOT-CONSUMABLE-BY-COMMAND` | P1 | **closed** — fixed in this work. Its original analysis was itself partly wrong (it blamed `try_into_string`, which the link path never uses); the correction is recorded in the issue. |
| `PREDECESSOR-CUT-NOT-YET-EQUIVALENT` | P1 | draft — the remaining worklist before the boundary default can be flipped |

## Important Learning

**Measurement found every defect; analysis found none.** Four equivalence differences were found by
running the suite with cutting forced on, and Phase 2's analysis had predicted that all such
differences would reduce to declaration defects:

1. A stale `predecessor_steps` across the recipe CWD prefix made a cut plan run the predecessor's
   action twice.
2. `Query::predecessor` splits a trailing filename off as the remainder, and recording at every
   recursion level let the outermost overwrite the inner one — so a cut swallowed the last action.
   Phase 4 had claimed moving the cut after building made this unreachable. It did not.
3. A dependency's error was replaced by "did not produce a value", hiding the cause behind a
   boundary.
4. Two more remain, tracked in the issue.

**Two of my own tests were weaker than the property they protected.** The error-chaining test
asserted the cause *appeared*, which passes with or without double-wrapping; it now asserts the
command prefix and position appear exactly once. This is the failure mode to watch for in the
remaining equivalence work.

**A preflight finding went stale inside a phase.** `PARAMETER-ESCAPING-INCOMPLETE` was P0 and
measured as breaking `parse(encode(q))` during Phase 2; it closed on `main` mid-phase, and
re-measuring after the rebase showed all probes passing. A constraint recorded as load-bearing
reverted to ordinary good practice. Re-measure rather than carry forward.

**The prior design anticipated this pass.** `plan-relative-resolution` §"Future Plan Normalization
and Optimization" blessed rewriting operands and blocked only *removing* `SetCwd`. Reading it
settled a Phase 2 decision without re-deriving the constraint.

**Existing as a value is not the same as being consumable as an argument.** Phase 2 recorded "no new
value types — `-R-key/.` yields `Value::Key`, which already exists". `Value::Key` had
`try_into_key` and `as_bytes` but no `TryFrom<Value>` and no `FromParameterValue`, so the mechanism
the whole design rested on could not reach a command.

## Conformance and Remaining Work

Delivered as approved: freezing, the builder change, the `Context` surface, error chaining, the
`-R-key/.` mechanism, the migration of all six call sites, and the documentation above.

Not delivered, deliberately:

- **Boundary cutting is off.** No caller invokes `cut_predecessor`. Divergences are down from 11 to
  4 (one of which asserts the expanded plan shape and is not a defect), tracked in
  `PREDECESSOR-CUT-NOT-YET-EQUIVALENT`.
- **The equivalence suite is partial**, for the same reason.
- **The runtime cursors remain in place**, per the approved migration decision: land freeze, assert
  the cursors go idle, remove them on evidence.
- **Non-keyed `Step::Evaluate` pre-scheduling** was filed separately in Phase 2 as a throughput
  matter wanting a benchmark.

## Validation

```bash
cargo test -p liquers-core --tests --no-fail-fast   # all 16 suites green
cargo test -p liquers-lib --lib --tests             # green
python3 scripts/docs_index.py --check               # 128 documents · 0 errors
cargo run -p liquers-core --features cli --bin liquers-validate -- ...   # 4 ok, 0 errors
```

## Completion Preconditions

Before this design moves to `status: complete` and its `phase` is removed:

- [x] Implementation finished and validated; CI green on PR #35
- [x] Review feedback addressed — both Codex findings reproduced, fixed and explained
- [x] Reference and guide documents updated, with `reviewed:` bumped and `## History` rows added
- [x] `CORE-RECIPES-EXPAND-PREDECESSORS-CRASH` closed with a resolution note
- [x] `CORE-PLAN-POLICY-AND-DEFAULTS` updated: its blocker is gone, its decision now depends on
      `PREDECESSOR-CUT-NOT-YET-EQUIVALENT`
- [x] Issues filed for omitted work
- [x] `specs/index.csv` and `specs/README.md` regenerated
- [ ] User approval at the Phase 5 gate
- [ ] PR #35 merged — the design carries `gh_pr`, so no derived status is written until then

## Review Checklist

- [x] Start criteria met: implementation complete and validated, review feedback addressed
- [x] Summary distinguishes requested, implemented, omitted and added scope
- [x] New issues and learning recorded
- [x] Approved reference and guide work explains present behaviour without the design folder
- [x] `affects_docs`, `reviewed:` and `## History` updated per `DOCS_STRUCTURE_GUIDE.md` §9
- [ ] Capability-map links and issue statuses — done in the same commit as this summary
