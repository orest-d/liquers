# Phase 4: Implementation Plan - Predecessor Cut Equivalence

## Overview

Ten steps. Steps 1-4 are inert with respect to shipped behaviour and each lands green on its own.
Step 5 changes a `pub` signature. Steps 6-8 build the suite that guards the flip. **Step 9 is the
flip.** Step 10 is the gate.

The ordering is chosen so that the switch is thrown last, against a suite that already passes —
not so that the suite chases a behaviour change already in flight.

Two couplings to respect:

- **Step 5 must land in one commit with its call-site updates**, or `liquers-core/tests` will not
  compile.
- **Step 6 depends on steps 1 and 3**: the walk reuses the prologue cursor and reads
  `volatility_source`.

Step 1 is already verified against the tree; the probes for steps 3 and 4 were applied, measured
green and reverted.

## Implementation Steps

### Step 1 — `Plan::prologue_steps`, recorded and used (VERIFIED)

**Files:** `liquers-core/src/plan.rs`, `liquers-core/src/recipes.rs`

Add the field with `#[serde(default)]`. `Recipe::to_plan` sets it beside the existing
`predecessor_steps` bump. In `freeze_cwd_with`, advance a scoped cursor over the prologue's
`SetCwd` steps before resolving `self.predecessor`:

```rust
if let Some(predecessor) = &mut self.predecessor {
    let mut scoped = cursor.clone();
    // Advance over steps the builder did not emit for `query` — a recipe's CWD prefix — so the
    // boundary query resolves from the working key its own steps start under.
    for step in self.steps.iter().take(self.prologue_steps) {
        if let Step::SetCwd(key) = step {
            scoped.set_cwd_from(key);
        }
    }
    *predecessor = scoped.resolve_query_scoped(predecessor);
}
```

`&mut self.predecessor` alongside `self.steps.iter()` is a disjoint-field borrow and compiles;
this exact form was built and run.

Add the three freeze units from Phase 3. `freeze_resolves_predecessor_after_the_recipe_prologue`
**must fail on `main`** — confirm that before keeping it.

**Validation**
```bash
cargo test -p liquers-core --lib freeze_
cargo test -p liquers-core --test recipe_cwd_resolution
```
**Already measured:** both `recipe_cwd_resolution` divergences clear under a forced cut; 19 suites
green with the cut off; `liquers-lib` green with it on.

**Agent:** sonnet · rust-best-practices · Phase 2 §"Recording the prologue"; `plan.rs` freeze
region; `recipes.rs::to_plan`

### Step 2 — `VolatilitySource`, and the scope the builder records

**Files:** `liquers-core/src/plan.rs`

Add the enum and `Plan::volatility_source`. Give the builder a scope-aware marker.

**The trap, and the reason this is its own step.** `mark_volatile` (`plan.rs:1239`) records only
when the plan is *not already* volatile:

```rust
fn mark_volatile(&mut self, reason: &str) {
    if !self.is_volatile { self.is_volatile = true; self.plan.init_info(...); }
}
```

So on `vol_cmd/v/tail` the `v` would be swallowed and the scope left `Positional` when it must
upgrade to `Declared`. **The scope upgrade must sit outside that guard.** Shape:

```rust
fn mark_volatile(&mut self, reason: &str, scope: VolatilitySource) {
    if !self.is_volatile {
        self.is_volatile = true;
        self.plan.init_info(reason.to_string());
    }
    // Outside the guard: `Declared` must win even when volatility was already set.
    self.upgrade_volatility_source(scope);
}
```

`upgrade_volatility_source` matches both variants explicitly — no `_ =>`. Call sites: the volatile
command path passes `Positional`; the `v` interception (`plan.rs:1486`) passes `Declared`.

**Validation**
```bash
cargo test -p liquers-core --lib volatil
```
Add `candidate_flags_are_per_prefix` and a `vol_cmd` + `v` combination test that pins the trap.

**Agent:** sonnet · rust-best-practices · Phase 2 §"The `v` instruction, and volatility scope"

### Step 3 — `Recipe::to_plan` folds its own declarations (PROBED)

**Files:** `liquers-core/src/recipes.rs`

Closes `RECIPE-TO-PLAN-IGNORES-RECIPE-LEVEL-VOLATILE-AND-EXPIRES`.

```rust
if self.volatile || self.expires.is_volatile() {
    plan.is_volatile = true;
    plan.upgrade_volatility_source(VolatilitySource::Declared);
}
if !self.expires.is_never() {
    plan.expires = plan.expires.clone().combine(self.expires.clone());
}
```

`Expires::combine` takes `self` by value, hence the clones. Add the three fold units from Phase 3,
including `finite_expiration_does_not_block_a_cut`.

**Validation**
```bash
cargo test -p liquers-core --lib to_plan
cargo test -p liquers-core --tests --no-fail-fast
```
**Already measured:** applied as a probe, all 19 `liquers-core` suites and the
`liquers-lib --lib --tests` loop stayed green. Green is not proof here — step 7 owes the test that
would have caught a regression.

**Agent:** sonnet · rust-best-practices · the issue; Phase 2 §"Folding the recipe's declarations"

### Step 4 — `check_consistent` and `Plan::split`

**Files:** `liquers-core/src/plan.rs`

Closes `PLAN-SPLIT-DROPS-PREDECESSOR-FIELDS`.

`check_consistent(&self) -> Result<(), Error>`: `prologue_steps <= steps.len()`, and
`prologue_steps <= predecessor_steps <= steps.len()` when `predecessor.is_some()`. Returns an
`Error::general_error` with the query attached — not a panic. Called after `build`, after
`to_plan`'s insert, after `split` and after `cut_predecessor`.

`split` builds both halves from `self.clone()` and replaces only what differs. Per Phase 2:
`frozen_cwd` and `volatility_source` carried on both; `predecessor` and `predecessor_steps`
cleared on both; `prologue_steps` carried clamped on the first, `0` on the second.

Add the three split/consistency units, including `split_index_equals_predecessor_steps`, which
pins the coincidence Phase 2 declined to rely on.

**Validation**
```bash
cargo test -p liquers-core --lib split check_consistent
```

**Agent:** sonnet · rust-best-practices · `PLAN-SPLIT-DROPS-PREDECESSOR-FIELDS`; Phase 2
§"Coupled fields"

### Step 5 — `cut_predecessor` takes the registry

**Files:** `liquers-core/src/plan.rs`, and every call site in `liquers-core/tests/` and
`liquers-core/src/interpreter.rs`'s test module

Signature only, no behaviour change: `cut_predecessor(&mut self, cmr: &CommandMetadataRegistry)`.
**Lands in one commit with the call sites**, from `grep -rn cut_predecessor`.

**Validation**
```bash
cargo check -p liquers-core --all-targets
```

**Agent:** haiku · — · the call-site list

### Step 6 — the placement walk

**Files:** `liquers-core/src/plan.rs`

The substance. In order:

1. **Declared decline**, before anything else: `volatility_source == Some(Declared)` →
   `init_info` + `Ok(false)`.
2. **Degenerate guard**: change `predecessor_steps > steps.len()` to `>=`. Equality means an empty
   tail — the whole plan replaced by a boundary recomputing itself. Unreachable once step 1
   handles `v`, but pinned independently because a positional `v` would reopen it.
3. **The prologue cursor**, built once from `frozen_cwd` and the prologue's `SetCwd` steps.
4. **The walk**: build each candidate with `with_placeholders_allowed()` (measured — without it
   the cut fails where expansion proceeds); accept the first with no payload requirement and not
   volatile; otherwise `init_info` the reason, freeze the candidate against a **named clone** of
   the prologue cursor, take its own `predecessor`, and continue. No candidate left → `Ok(false)`.
5. **The step-count cross-check**: `candidate.steps.len() != cut_at - prologue_steps` →
   `Ok(false)`, decline rather than mis-split.

**Borrow-checker note.** `self.init_info(…)` takes `&mut self`, so no borrow of `self.steps` or
`self.predecessor` may be live across it. Clone `boundary` out of `self.predecessor` before the
loop, and let the prologue-cursor loop's borrow end before the first `init_info`.

Add the five walk units from Phase 3.

**Validation**
```bash
cargo test -p liquers-core --lib cut_ walk_ candidate_
```

**Agent:** sonnet · rust-best-practices · Phase 2 §"Where a boundary goes"; `PlanBuilder`;
`CwdCursor`

### Step 7 — the equivalence suite

**Files:** `liquers-core/tests/plan_cwd_freeze.rs`, `liquers-core/src/interpreter.rs`

Move `evaluate_both_ways` out of `interpreter.rs`'s `#[cfg(test)] mod` into the existing
`plan_cwd_freeze.rs` (8 tests, `where_am_i` fixture already there). Widen it: four compared
properties, the three-way CWD axis, per-shape result rows rather than fail-fast. Header comment
states what equivalence covers and what differs by design.

Fill E1-E16, then the five corner cases — **including `volatile_recipe_dependency_records`**, the
debt step 3 incurred.

Environments per Phase 3: `ImmediateEnvironment` for plan/CWD shapes, `SimpleEnvironment` +
`AsyncMemoryStore` for `-R/` and keyed recipes, `SimpleEnvironmentWithPayload<Value, String>` for
E7, E8, E13, E14.

**Validation**
```bash
cargo test -p liquers-core --test plan_cwd_freeze
```
Expect E8 to assert an inequivalence, and every other shape to agree, **with the cut still off** —
the suite drives `cut_predecessor` directly on a clone, so it does not need step 9.

**Agent:** sonnet · liquers-unittest, liquers-validate · Phase 3; `plan-cwd-freeze/phase3-examples.md`

### Step 8 — the two existing-test corrections

**Files:** `liquers-core/tests/injection.rs`, `liquers-core/src/interpreter.rs`

`payload: required` on `first_cmd` and `third_cmd`. The two `steps[1]` shape assertions in
`absolute_outer_resource_keeps_relative_link_on_live_cwd` made policy-explicit — measured, with
them relaxed the test passes under the cut with the same value and CWD.

**Validation**
```bash
cargo test -p liquers-core --test injection
cargo test -p liquers-core --lib absolute_outer
```

**Agent:** haiku · — · Phase 2 causes 2 and 3

### Step 9 — the flip

**Files:** `liquers-core/src/interpreter.rs`

`finalize_plan` calls `plan.cut_predecessor(envref.get_command_metadata_registry())?` after the
volatility and expiration passes — after freezing, so the boundary query is absolute.

**This is the only step that changes shipped behaviour.** Everything before it is inert. It is one
statement, and reverting it returns the default to expanded while keeping every fix.

**Validation**
```bash
cargo test -p liquers-core --tests --no-fail-fast
cargo test -p liquers-lib --lib --tests
```

**Agent:** sonnet · — · Phase 1 §Purpose; `finalize_plan`

### Step 10 — full validation

```bash
cargo test -p liquers-core --tests --no-fail-fast
cargo test -p liquers-lib --lib --tests
bash scripts/check-build-matrix.sh
python3 scripts/docs_index.py --check
```

## Testing Plan

Per-step commands above; step 10 is the gate. Three notes:

- **`check-build-matrix.sh`** is included because two `#[serde(default)]` fields and a new public
  enum touch the serialization surface, though no `#[cfg(feature)]` is involved.
- **Steps 1-8 run with the cut off.** The suite exercises `cut_predecessor` directly on a clone,
  so equivalence is proven before the default changes. That is the point of putting step 9 last.
- **`liquers-web` and the browser loops are unaffected** and are not run — no `wasm32` or
  feature-gated code is touched.

Baseline to preserve, measured at `d1bd02e`: 19 `liquers-core` suites green, `liquers-lib --lib
--tests` exit 0 — under the prologue fix with the cut forced on, and again under the recipe fold.

## Agent Assignment

| Step | Tier | Skills | Knowledge |
|---|---|---|---|
| 1 | sonnet | rust-best-practices | Phase 2 §Recording the prologue; `plan.rs` freeze region |
| 2 | sonnet | rust-best-practices | Phase 2 §`v` and volatility scope; `mark_volatile` at `plan.rs:1239` |
| 3 | sonnet | rust-best-practices | `RECIPE-TO-PLAN-IGNORES-…`; `Expires::combine` |
| 4 | sonnet | rust-best-practices | `PLAN-SPLIT-DROPS-…`; Phase 2 §Coupled fields |
| 5 | haiku | — | `grep -rn cut_predecessor` |
| 6 | sonnet | rust-best-practices | Phase 2 §Where a boundary goes; `PlanBuilder`, `CwdCursor` |
| 7 | sonnet | liquers-unittest, liquers-validate | Phase 3; existing `plan_cwd_freeze.rs` |
| 8 | haiku | — | Phase 2 causes 2 and 3 |
| 9 | sonnet | — | Phase 1 §Purpose; `finalize_plan` |
| 10 | sonnet | — | CLAUDE.md build section |

Agents are not spawnable on this host, so these are tier labels for whoever executes, recorded so
Claude and Codex produce the same schema.

## Rollback Plan

Each step is a separate commit and independently revertible, with one coupling: steps 5 and 7
revert together (signature and call sites).

**The design's kill switch is step 9.** Everything before it is inert with respect to shipped
behaviour: `cut_predecessor` has no production caller until then, and the only pre-flip behaviour
change is step 3's fold, which is a display correction plus a dependency-registration change that
`volatile_recipe_dependency_records` guards.

| Failure discovered | Rollback |
|---|---|
| A divergence after the flip | Revert step 9 alone. Default returns to expanded; every fix and the suite stay |
| The fold changes something the suite misses | Revert step 3. Independent of the cut; nothing else depends on `plan.expires` being combined |
| A walk defect | Revert steps 6 and 9. Steps 1-4 stand on their own as fixes |
| A `split` regression | Revert step 4. No production caller |

## Phase 5 Entry Criteria

- [ ] Steps 1-10 landed; step 10's four commands green
- [ ] E1-E16 passing under all three CWD conditions, E8 still pinning its inequivalence
- [ ] `volatile_recipe_dependency_records` present and passing — the debt from step 3
- [ ] `freeze_resolves_predecessor_after_the_recipe_prologue` confirmed to fail on `main`
- [ ] All user and review comments answered
- [ ] Ready to close: `PREDECESSOR-CUT-NOT-YET-EQUIVALENT`, `PLAN-SPLIT-DROPS-PREDECESSOR-FIELDS`,
      `RECIPE-TO-PLAN-IGNORES-RECIPE-LEVEL-VOLATILE-AND-EXPIRES`
- [ ] Ready to update: `CORE-PLAN-POLICY-AND-DEFAULTS` (its `expand_predecessors` half is answered)
- [ ] Documentation per Phase 2 §"Documentation Architecture" — rustdoc, `DOC_08` including the
      superseded closing paragraph, `## History` row and `reviewed:` bump, `specs/README.md`

## Review Record

The host does not permit spawning agents, so the four conformity passes and the final critical
review ran sequentially against the same briefs, per this skill's host-compatibility clause.

**rust-best-practices pass — implementation validation.** Three findings, all folded into the
steps above rather than left as notes:

1. **`mark_volatile`'s early-out swallows a later `Declared`** (step 2). The scope upgrade must sit
   outside `if !self.is_volatile`, or `vol_cmd/v/tail` records `Positional` and gets cut. This is
   the kind of defect the design's own history says analysis misses, so step 2 carries a test for
   it specifically.
2. **`init_info` needs `&mut self`** (step 6), so no borrow of `self.steps` or `self.predecessor`
   may be live across it. Clone the boundary out before the loop.
3. **`Expires::combine` takes `self` by value** (step 3), so the clones are required rather than
   sloppy — noted so nobody "optimises" them into borrows that will not compile.

Confirmed clean: no `unwrap`/`expect` outside tests; no `Error::new`; `check_consistent` returns
`Result` rather than panicking; `upgrade_volatility_source` matches both variants explicitly;
borrowed `&CommandMetadataRegistry` with no lifetime on `Plan`; sync throughout, justified in
Phase 2; no trait signature changed, so `liquers-py` implementors are unaffected.

**Reviewer 1 — Phase 1 conformity.** The default flip is present as step 9, which is Phase 1's
confirmed intent; the non-goal (complete decomposition) is not approached; the expanded plan's
analysis role is untouched, since `liquers-validate` never finalizes.

**Reviewer 2 — Phase 2 conformity.** Every signature and field matches Phase 2. One ordering
correction applied: an earlier draft folded the recipe declarations (step 3) before
`VolatilitySource` existed (step 2), which would not compile — `to_plan` sets `Declared`.

**Reviewer 3 — Phase 3 conformity.** Every test named in Phase 3 has a step that creates it. One
gap closed: Phase 3's `volatile_recipe_dependency_records` had no home; it is now explicit in
step 7 and in the Phase 5 entry criteria, so the debt cannot be quietly dropped.

**Reviewer 4 — codebase compatibility.** Call sites of `cut_predecessor` are test-only, verified
by grep. `plan_cwd_freeze.rs` exists and is extended rather than created. `mark_volatile`,
`Expires::combine`, `Recipe::to_plan` and the `v` interception were read at HEAD, not recalled.

**Final critical review.** Certainty is high on steps 1-5 and 8-10: three of them are already
measured against the tree, and the rest are mechanical. **Step 6 is the one with real
uncertainty** — it is the only step whose logic has not been executed, and it carries the
borrow-checker constraint, the freeze-per-candidate wrinkle and the cross-check. If any step
needs to be broken down further during execution, it is that one; the suite from step 7 is what
will say so.
