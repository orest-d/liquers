# Phase 5: Documentation - Predecessor Cut Equivalence

## Completion Preconditions

All met.

- [x] Steps 1-10 landed, one commit each; step 10's four commands green
- [x] The equivalence suite passing under all three CWD conditions, E8 still pinning its
      inequivalence
- [x] `volatile_recipe_skips_dependency_registration` present and passing — the debt from step 3
- [x] `freeze_resolves_predecessor_after_the_recipe_prologue` confirmed to fail without the fix
- [x] All user comments answered
- [x] Issues ready to close

## Implementation Summary

**Cutting at the outermost cacheable predecessor is now the default.** `finalize_plan` calls
`Plan::cut_predecessor` after freezing and after the analysis passes, so an intermediate becomes
an asset the manager can cache, share, expire and schedule instead of recomputing it inside every
consumer. That was the point of the boundary machinery `plan-cwd-freeze` built and left switched
off.

**What was requested and delivered.** The issue asked for equivalence between cutting and
expanding, and named a twelve-shape suite as its expected behaviour. Both landed, with the suite
grown to sixteen shapes across three CWD conditions.

**What was added beyond the request, and why.**

- *The default flip.* The issue framed this as unblocking `CORE-PLAN-POLICY-AND-DEFAULTS`; Phase 1
  established that cutting is the intended behaviour, not a policy left open, and it ships here.
- *Two adjacent issues, at the author's direction.* `PLAN-SPLIT-DROPS-PREDECESSOR-FIELDS` and
  `RECIPE-TO-PLAN-IGNORES-RECIPE-LEVEL-VOLATILE-AND-EXPIRES` were both found during the design and
  both taken into scope rather than deferred.
- *A volatility scope distinction.* Not anticipated. Checking `v` against the placement walk
  showed that `PlanBuilder` collapsed two different kinds of volatility into one flag; drawing the
  distinction replaced a recipe-only marker and made three measured obstacles unreachable.

**What was omitted.** Nothing from the approved plan. Complete decomposition remains a non-goal,
and a positional `v` remains `V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL`.

**What the measurements narrowed, which the final positions no longer show.** Three claims in this
design were wrong until measured, and the corrections are the substance rather than the trivia:

- The issue's own diagnosis blamed "a nested keyed recipe re-deriving its own working key". It was
  one missing cursor advance.
- The payload rule moved three times — declare-it, a blanket decline, then a per-candidate walk.
  The first two made the plan compensate for a missing declaration, hiding the defect.
- Volatility was overstated as a general divergence. Command-level volatility never diverged
  (2 runs both ways); only recipe-level did (2 → 1). The general claim would have produced a
  guard three times larger than the problem.

## Documentation Delivered

- [x] `Recipe::volatile` — volatile **from the first action**; why the last-action reading fails,
      with the measured 2 → 1; that the positional instrument is `v`
- [x] `Recipe::expires` — bounds the result's validity, is combined into the plan, does **not**
      block a cut unless itself volatile
- [x] `Recipe::to_plan` — the two facts it now folds on, and why neither is recoverable later
- [x] `PlanBuilder` — what it records for later passes and does not act on
- [x] `Plan::cut_predecessor` — the placement rule, in the place a reader lands from a stack trace
      or an `init_info` line
- [x] `Plan::{prologue_steps, volatility_source}`, `VolatilitySource` — field and variant docs
- [x] `DOC_08_RECIPES_PLANS.md` — "Where a boundary goes"; the superseded deferral paragraph
      rewritten; four pitfall rows; the plan-fields table; a paragraph on `v`'s whole-plan scope
- [x] `DOC_08` `## History` row and `reviewed:` bump, same commit
- [x] `specs/README.md` — regenerated

**No new reference and no guide**, as Phase 1 decided and Phase 3 re-tested. The one candidate —
"how do I read the boundary diagnostics" — is answered by the `init_info` at the moment an author
asks.

`affects_docs`: `[specs/reference/api/DOC_08_RECIPES_PLANS.md]`. Rejected candidates re-checked at
this phase and still rejected: `DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` (the evaluation entry
points are unchanged — `Context::evaluate`, `apply` and `schedule_dependency_asset` have the same
signatures and the same behaviour) and `PROJECT_OVERVIEW.md` (no core concept changes; a boundary
is a query becoming an asset, which that document already describes).

## Issues Filed

| Issue | Outcome |
|---|---|
| `PREDECESSOR-CUT-NOT-YET-EQUIVALENT` | **closed** — the design's subject |
| `PLAN-SPLIT-DROPS-PREDECESSOR-FIELDS` | **closed** — step 4 |
| `RECIPE-TO-PLAN-IGNORES-RECIPE-LEVEL-VOLATILE-AND-EXPIRES` | **closed** — step 3 |
| `CORE-PLAN-POLICY-AND-DEFAULTS` | **updated** — its `expand_predecessors` half is answered; three markers remain |
| `V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL` | open (P3), filed here. Nothing depends on it |
| `RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING` | open (P3), filed here. Out of scope |
| `PAYLOAD-SOURCED-INJECTION-NOT-DECLARED` | **rejected** — filed and rejected the same day; the payload need is on command metadata |

Nothing new was found during implementation that is not already filed.

## Important Learning

**Running the *existing* suite under a flipped policy is how this class of defect surfaces.** All
four divergences came from one forced run. The purpose-built equivalence harness found none of
them, because it held fixed the one axis they lived on — it always built a recipe with no `cwd:`.
A harness that cannot vary the condition a defect lives on is worse than no harness, because it
reports confidence.

**Measure before writing the claim down.** Every position in this design that was wrong was
corrected by a measurement costing less than the argument that preceded it. The `v` check is the
clearest case: it was raised as a risk to a finished architecture and returned a better one.

**One defect shape recurred three times** — a plan mutated through a subset of coupled fields.
`Recipe::to_plan`'s stale `predecessor_steps`, `freeze_cwd_with`'s stale cursor, `Plan::split`'s
dropped fields. Two shipped. The response is structural rather than a third fix: build from
`self.clone()`, and `check_consistent` at every point a plan finishes being constructed.

**A predicted trap was worth its own step.** Phase 4's rust-best-practices pass found that
`mark_volatile` records only when the plan is not already volatile, so a `Declared` source
arriving after a `Positional` one would be swallowed. Confirmed by reverting it during
implementation: `vol_cmd/v/tail` records `Positional` where it must record `Declared`, and a plan
declaring nothing is cacheable would have had a boundary cut out of it. Found by review, not by a
failing test — no existing test covered that ordering.

## Conformance and Remaining Work

| Scope | Status |
|---|---|
| Requested — equivalence, and the suite that keeps it | Delivered |
| Requested — unblock the default | Delivered, and the default flipped |
| Added — the two adjacent issues | Delivered, at the author's direction |
| Added — volatility scope | Delivered; not anticipated in Phases 1-2 |
| Not done — positional `v` | `V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL` (P3) |
| Not done — analysis passes rerun for previews | `RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING` (P3) |

Nothing was left partially done. `DOCS_STRUCTURE_GUIDE.md` §5.6 is satisfied: what did not land
is an open issue, not an unfinished phase.

## Validation

```
cargo test -p liquers-core --tests --no-fail-fast    19 suites, 0 failures
cargo test -p liquers-lib  --lib --tests             exit 0
bash scripts/check-build-matrix.sh                   All 11 configurations OK
cargo test -p liquers-lib  --test registry_export    5 passed — no command signature changed
python3 scripts/docs_index.py --check                166 documents · 0 errors
```

The build matrix needed `rustup target add wasm32-unknown-unknown` first; without it two wasm32
rows fail for a missing toolchain rather than a code defect, exactly as `CLAUDE.md` records.
