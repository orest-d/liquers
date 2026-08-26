---
id: PREDECESSOR-CUT-EQUIVALENCE
kind: design
title: Make cutting a predecessor boundary observably equivalent to expanding it
workflow: liquers-project
status:
phase: documentation
area: [core/plan, core/assets, core/context]
issues: [PREDECESSOR-CUT-NOT-YET-EQUIVALENT, PLAN-SPLIT-DROPS-PREDECESSOR-FIELDS, RECIPE-TO-PLAN-IGNORES-RECIPE-LEVEL-VOLATILE-AND-EXPIRES, CORE-PLAN-POLICY-AND-DEFAULTS]
affects_docs: [specs/reference/api/DOC_08_RECIPES_PLANS.md]
gh_pr: [43]
created: 2026-08-26
superseded_by:
---
# Predecessor Cut Equivalence Design Tracking

**Created:** 2026-08-26

Follow-on to `plan-cwd-freeze`, which built the boundary machinery (`Plan::freeze_cwd`,
`Plan::predecessor`, `Plan::cut_predecessor`) and left it switched off because cutting still
changes observable behaviour.

## Phase Status

- [x] Phase 1: High-Level Design — **approved 2026-08-26**
- [x] Phase 2: Solution & Architecture — **approved 2026-08-26**
- [x] Phase 3: Examples & Testing — **approved 2026-08-26**
- [x] Phase 4: Implementation Plan — **approved 2026-08-26**
- [x] Implementation: steps 1-10 landed
- [ ] Phase 5: Documentation — **written, awaiting the approval gate**
- [ ] Implementation Complete

## Notes

Phase 1 was established by measurement rather than by reading, in the manner `plan-cwd-freeze`
used: `cut_predecessor` has no production caller, so the divergences are only visible when it is
forced on. Three lines in `finalize_plan`, then
`LQ_FORCE_CUT=1 cargo test -p liquers-core --tests --no-fail-fast`. The probe is a measurement
tool and is not part of any change set.

**Verified during Phase 1** (all at `d1bd02e`, each by running rather than reading):

- **4 divergences from 3 causes**, matching the issue's table exactly: 2 in
  `recipe_cwd_resolution`, 1 in `injection`, 1 in `--lib`; the other 16 suites green.
- **The two CWD divergences are one defect**, and not the one the issue guessed at.
  `freeze_cwd_with` resolves the recorded predecessor from the cursor's *entry* state, but
  `Recipe::to_plan` prepends a `SetCwd` the builder never emitted. The step count is compensated
  (`predecessor_steps += 1`); the cursor is not. So the boundary query — the only thing a cut
  carries — is frozen one CWD short. Symptoms measured: `KeyNotFound: 'input.txt'` where
  expansion returns `"programmatic"`, and `"child|a/c"` where expansion returns `"a/c/child|a/c"`.
  A prototype fix (recording the prologue length and advancing over it) clears both, keeps
  `liquers-core` green with the cut off, and `liquers-lib --lib --tests` exits 0 with it on.
- **The `injection` divergence is a mis-declared command**, not a code defect: `first_cmd` and
  `third_cmd` read the payload through injected parameters and declare no `payload: required`.
- **The `--lib` divergence is a shape assertion.** With the two `steps[1]` assertions relaxed the
  test passes under the cut, same value and same context CWD.
- **A fifth cause, found by reading and then measured.** A recipe-level `volatile:` is not in the
  query text, so it does not reach a boundary. Counting prefix executions over two evaluations:
  command-level `volatile: true` runs 2 both ways; recipe-level runs 2 expanded and **1** cut.
- **`PlanBuilder` already walks every candidate prefix and discards all but the last.** Measured
  by instrumenting the recording point in `process_query`: it recurses into the predecessor
  first, so on the way back up it visits each prefix in order, shortest to longest, and at each
  one already holds the promoted prefix query, that prefix's exact step count, and the
  *cumulative* `is_volatile` / `payload_required` **for that prefix** — the remainder has not been
  processed yet. It then overwrites `plan.predecessor` and keeps only the longest.

  ```
  prefix/vol/tail/render      steps  volatile  payload   remainder_is_action
    prefix                      1      false     none       true
    prefix/vol                  2      true      none       true      <- volatility enters here
    prefix/vol/tail             3      true      none       true
  a/b/c/d/out.txt
    a, a/b, a/b/c               1,2,3  false     none       true
    a/b/c/d                     4      false     none       false     <- filename remainder
  -R/x.csv/-/a/b
    -R/x.csv                    1      false     none       true      <- a resource is a candidate
    -R/x.csv/-/a                2      false     none       true
  ```

  So the flags are per-prefix and monotone, the candidate set is every action boundary (a
  resource fetch included), and it is complete — a boundary must be a query, so there is no finer
  granularity. `remainder_is_action` marks the one candidate that must be excluded: cutting where
  the remainder is a trailing filename leaves the parent nothing but a `Filename` step, and a
  recipe's overrides nothing to patch.
- **`split_index == predecessor_steps`** on every shape tried, prologue included — so the first
  half of a `Plan::split` *is* the predecessor's steps.
- **The `v` instruction exists** and is builder-intercepted like `q` and `ns`, takes no
  parameters and emits no step, so it is an identity on the value — but it marks the **whole**
  plan volatile regardless of position.

**Decided in discussion** (to be formalised in Phase 2):

- The cut is placed at the last candidate boundary that can be **cached**; a candidate cannot be
  cached if its own plan requires a payload or is volatile. Whole-plan flags are the wrong
  granularity in both directions.
- A payload need is declared on command metadata and must **not** be inferred from `injected`,
  which may be satisfied from the environment alone.
- A volatile recipe is volatile **from its first action**. The flag carries no position, so it
  cannot mark where a non-volatile part ends; the positional instrument is `v`.
- Every level passed over, and the decline, says why — an `init_info` naming the command.
- `Plan::split` dropping the coupled predecessor fields is in scope, because the field list is
  the shape of every defect in this lineage, two of which shipped.
- **Cutting at the outermost cacheable predecessor is the intended default**, not a policy left
  open. It is what lets the `AssetManager` cache, share, expire and schedule an intermediate; it
  is not the default today only because it does not work. This supersedes `DOC_08`'s closing
  paragraph under "Why the default should make the predecessor available", which defers the
  decision to `CORE-PLAN-POLICY-AND-DEFAULTS`. That issue's other three markers — cache,
  volatile flags, inline flag — are untouched.
- **Complete decomposition is a non-goal.** A boundary at every action is interesting mainly
  because it is possible, and the case for it is a volatile plan, where nothing is cacheable so
  an asset per step buys the dependency graph and parallel scheduling rather than caching. Not
  foreclosed, not built.
- Whether `PlanBuilder` keeps the candidate list it already computes is an implementation
  detail, not a design question — it may keep one if that produces a correct plan. What matters
  is identifying the cut point.
- **A recipe-level `expires:` does not block a cut.** It bounds how long the resulting asset
  stays valid, not the purity of the computation. Only volatility makes a plan uncuttable, which
  against `assets.rs:1610` reads `recipe.volatile || recipe.expires.is_volatile()` — and
  `Expires::is_volatile` is true only for `Immediately` (or a `Combination` containing one), so a
  plain finite expiration is unaffected. The rule reuses the predicate the asset layer already
  applies rather than inventing a second one.
- **The expanded plan is the oracle, not a co-equal shipping form.** The suite is a correctness
  verification of the *cut* plan against it. Its remaining role is explanation and analysis —
  `liquers-validate` calls `PlanBuilder::build` directly (`validate/mod.rs:72`) and never
  finalizes, so it keeps the expanded form for free whatever the evaluation default is. Its other
  apparent role, lower memory through less caching, belongs to a future asset-manager retention
  policy; `CORE-ASSET-GC` already owns that and no new issue is warranted.

### Implementation, 2026-08-26

Steps 1-10 landed, one commit each. Everything measured during the phases held when built:

- **Step 1** — `prologue_steps`. `freeze_resolves_predecessor_after_the_recipe_prologue`
  confirmed to fail without the walk (`-R/input.txt/-/identity` where
  `-R/a/c/input.txt/-/identity` was required), with no cut involved.
- **Step 2** — `VolatilitySource`. The trap Phase 4 predicted is real: with the scope upgrade
  inside `mark_volatile`'s early-out, `vol_cmd/v/tail` records `Positional` where it must record
  `Declared`. Confirmed by reverting it.
- **Step 3** — the recipe fold. `RECIPE-TO-PLAN-IGNORES-RECIPE-LEVEL-VOLATILE-AND-EXPIRES` closed.
- **Step 4** — `check_consistent` and `split` from `self.clone()`.
  `PLAN-SPLIT-DROPS-PREDECESSOR-FIELDS` closed.
- **Steps 5-6** — the walk. All nine unit tests passed first run; the four call sites were exactly
  those predicted, and one of them constructed the degenerate whole-plan shape the `>=` guard
  exists for.
- **Step 7** — the suite. 13 shapes × 3 CWD conditions agree, plus the payload shapes and the
  corner cases, 17 tests in `plan_cwd_freeze`.
- **Steps 8-9** — the declaration fix, and **the flip**. One predicted failure, the shape
  assertion, moved onto an explicitly un-cut plan.
- **Step 10** — `liquers-core` 19 suites green, `liquers-lib --lib --tests` exit 0, both with the
  cut on.

**Confirmed at the Phase 1 gate:** the default flip ships in this design, not a follow-on.

**Filed during Phase 2:** `RECIPE-TO-PLAN-IGNORES-RECIPE-LEVEL-VOLATILE-AND-EXPIRES` (P2) —
`Recipe::to_plan` reads neither `volatile:` nor `expires:`, measured, so a recipe preview
under-reports both.

**Filed during Phase 1:** `V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL` (P3),
`RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING` (P3);
`PLAN-SPLIT-DROPS-PREDECESSOR-FIELDS` raised P3 → P2 and taken into scope;
`PAYLOAD-SOURCED-INJECTION-NOT-DECLARED` filed and rejected the same day, its premise answered by
command metadata.

**Process note.** Phases 2-4 were drafted before this workflow was applied, then withdrawn: the
approval gates had not been run, and content written ahead of a gate anchors the phase it
pre-empts. The measurements above survive because they are facts about HEAD, not design
decisions. Everything else is re-derived at its own gate.

**Agent orchestration.** The host does not permit spawning review agents, so each phase's review
passes run sequentially against the same briefs and are recorded in the phase document, per this
skill's host-compatibility clause. `plan-cwd-freeze` recorded the same limitation.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
- Predecessor design: [`plan-cwd-freeze`](../plan-cwd-freeze/DESIGN.md)
- Reference: `specs/reference/api/DOC_08_RECIPES_PLANS.md`, "Predecessor boundaries"
