# Solution: the change set

Six steps, in landing order. Steps 1–3 remove the divergences; step 4 is the suite the issue
names as its expected behaviour; steps 5–6 close it out. Step 1 is verified; the rest are
designed.

## 1. `Plan` records its prologue explicitly — **verified**

A plan currently has no way to say "these leading steps did not come from my query", and
three places infer it independently: `Recipe::to_plan` bumps `predecessor_steps` by a
hard-coded 1, `absolute_query_resource_step_index` back-matches source resource segments
against steps to keep the prefix distinct, and `freeze_cwd_with` assumes there is no prefix
at all. The third is Cause 1.

Record the number once.

```rust
// plan.rs, in `struct Plan`
/// Number of leading [`Self::steps`] that were *not* emitted by the builder for
/// [`Self::query`] — a recipe's CWD prefix.
#[serde(default)]
pub prologue_steps: usize,
```

```rust
// recipes.rs, in `Recipe::to_plan`, beside the existing predecessor_steps bump
plan.prologue_steps += 1;
```

```rust
// plan.rs, in `Plan::freeze_cwd_with`
if let Some(predecessor) = &mut self.predecessor {
    let mut scoped = cursor.clone();
    // Advance over the steps the builder did not emit for `query` — a recipe's CWD
    // prefix — so the boundary query resolves from the working key its own steps start
    // under, not from the entry one.
    for step in self.steps.iter().take(self.prologue_steps) {
        if let Step::SetCwd(key) = step {
            scoped.set_cwd_from(key);
        }
    }
    *predecessor = scoped.resolve_query_scoped(predecessor);
}
```

**Measured:** both `recipe_cwd_resolution` divergences disappear
(`programmatic_and_provider_cwd_select_their_own_inputs` and
`recursive_links_and_multiple_parameters_use_active_cwd` pass under the cut), and
`cargo test -p liquers-core --tests` stays green with the cut off — 19 suites, no failures.

Why a recorded field rather than inference: a prologue longer than one step, or a second
producer of one, breaks every one of the three current guesses silently and in a different
way each time. The field also gives `predecessor_steps += 1` a stated meaning instead of a
coincidence, and `absolute_query_resource_step_index` a cheaper basis than back-matching —
worth converting in a follow-up, not in this change.

`#[serde(default)]` matches the three fields `plan-cwd-freeze` added. A plan serialized
before this change deserializes with `prologue_steps: 0`, i.e. the pre-change behaviour;
plans are not persisted across versions, so this is a compatibility formality rather than a
migration.

Unit test to add, in `plan.rs`'s `mod tests`:
`freeze_resolves_predecessor_after_the_recipe_prologue` — build a plan from a recipe with
`cwd: "a/c"` and a relative predecessor, freeze against no entry CWD, and assert the frozen
`plan.predecessor` names `a/c/...`. It fails on `main` and passes with the change, without
needing the cut at all — which is the point: the defect is in freezing, and cutting only
exposes it.

## 2. The cut walks back to the last payload-free boundary

Cause 2 is not a defect in cutting. A boundary is a cache entry; a payload is deliberately
not part of a cache key; so a value computed from a payload must never end up behind one. The
correct behaviour is to leave the payload-sensitive part of the plan **expanded** and cut, if
at all, in front of it.

Two facts settle how:

- **The payload need is on command metadata.** `CommandMetadata::payload_required` is the
  declaration, `PlanBuilder::action_payload_requirement` reads it, and the builder ORs it up
  into `Plan::payload_required`. Nothing has to be approximated from `injected` — which would
  be wrong anyway, since injection may be from the environment alone.
- **The decision is per candidate boundary, not per plan.** `Plan::payload_required` answers
  "does this query need a payload anywhere", which is the wrong question in both directions:
  used to decline, it throws away the boundary in `fetch/expensive/render_with_payload`, where
  everything behind the only candidate is payload-free; used to permit, it cuts straight
  across `fetch/personalize/render`. What matters is whether *the steps behind this particular
  boundary* need a payload — which is exactly `payload_required` of the boundary query's own
  plan.

### The rule

Build the candidate boundary's plan and cut only if that plan requires no payload. If it
does, step back one level and try the shorter candidate; if none is payload-free, do not cut.

### Mechanism

`PlanBuilder` records one predecessor per plan (`plan.rs:1716`), so each level supplies the
next: building the recorded predecessor query yields a plan that records *its* predecessor.
Recursion over the builder is therefore all that is needed, and no new plan field is:

```rust
pub fn cut_predecessor(
    &mut self,
    cmr: &CommandMetadataRegistry,
) -> Result<bool, Error> {
    // ... existing frozen / predecessor / range guards ...

    // The working key the query's own steps begin under — §1's prologue walk, reused so a
    // candidate query deeper than the recorded one can be frozen the same way.
    let mut base = CwdCursor::new(self.frozen_cwd.clone());
    for step in self.steps.iter().take(self.prologue_steps) {
        if let Step::SetCwd(key) = step {
            base.set_cwd_from(key);
        }
    }

    let mut boundary = self.predecessor.clone();          // frozen by freeze_cwd
    let mut cut_at = self.predecessor_steps;

    loop {
        let mut candidate = PlanBuilder::new(boundary.clone(), cmr)
            .with_placeholders_allowed()
            .build()?;
        // The candidate's own steps are what the parent's prefix is made of; a mismatch means
        // a recorded range went stale, and splitting on it would run an action twice.
        if candidate.steps.len() != cut_at - self.prologue_steps {
            return Ok(false);
        }
        if candidate.payload_required.is_none() {
            break;                                        // safe: nothing behind it reads a payload
        }
        candidate.freeze_cwd_with(&mut base.clone())?;    // resolve the next candidate's operands
        let Some(inner) = candidate.predecessor.clone() else {
            return Ok(false);                             // payload need reaches the head
        };
        cut_at = self.prologue_steps + candidate.predecessor_steps;
        boundary = inner;
    }

    // ... existing split at `cut_at`, emitting Step::Evaluate(boundary) ...
}
```

The signature gains the registry. Every call site has one: `finalize_plan` holds `envref`, and
`EnvRef::get_command_metadata_registry` returns `&CommandMetadataRegistry`.

Termination is structural — each level is strictly shorter than the last, and the innermost
has `predecessor: None`.

### Measured

Behaviour of the recursion, run against a registry where only `personalize` declares
`payload: required`:

| Query | Level 0 candidate | Payload | Outcome |
|---|---|---|---|
| `fetch/expensive/render` | `fetch/expensive` (2 steps) | none | cut at 2 — unchanged |
| `fetch/expensive/render/out.txt` | `fetch/expensive` (2 steps) | none | cut at 2 — a filename is not an action |
| `fetch/personalize/render` | `fetch/personalize` (2 steps) | required | step back → `fetch` (1 step), none → **cut at 1** |
| `personalize/fetch/render` | `personalize/fetch` (2 steps) | required | step back → `personalize`, required, no predecessor → **no cut** |

The third row is the case a plan-level flag cannot express: `fetch` is cached and shared while
`personalize/render` runs inline per payload. The fourth is the head case, where the payload
need reaches the first action and no boundary at any position is safe.

Step counts line up at every level — the parent's `predecessor_steps` equals the candidate
plan's `steps.len()`, and the candidate's `predecessor_steps` equals the next one's — which is
what makes `cut_at` derivable from the recursion rather than needing to be recorded.

### The freeze wrinkle

`freeze_cwd` resolves `plan.predecessor`, so the level-0 candidate arrives frozen. A candidate
found by stepping back is built fresh from source and is **not**: its operands are still
CWD-relative, and cutting on it would produce exactly the boundary query
`plan-cwd-freeze` exists to prevent. Hence the `freeze_cwd_with` call on each candidate before
its predecessor is read, against a clone of the prologue-advanced cursor from §1 — the same
operation, applied one level deeper. This is the part of the mechanism most likely to be got
wrong, and E13 below pins it.

### What this does not change

No command declaration changes, and `payload: required` keeps exactly its present meaning. A
command that reads the payload must declare it — the existing "declare it, or lose it" rule.
`injected` is left alone: it means `InjectedFromContext`, which may be satisfied from the
environment, and it is not evidence of a payload read in either direction.

### Consequence for E8, and for the injection test

E8 stands as `plan-cwd-freeze` Phase 3 wrote it: an **inequivalence** test, pinning an
undeclared payload command as a case where the two forms differ. I proposed replacing it in
two earlier drafts of this document — first with an opt-in, then with a blanket decline — and
both were wrong: they made the plan compensate for a declaration that is missing, which hides
the defect instead of surfacing it.

`injection::test_chained_commands_with_payload` is therefore a **test fix, not a code fix**:
`first_cmd` and `third_cmd` read the payload through injected parameters and declare nothing,
so they are mis-declared. Adding `payload: required` to both is the change, and
`plan-cwd-freeze` already measured that it makes the test pass under the cut.

### Alternative considered

Record the payload requirement of every candidate level during the single build, as
`Vec<(Query, usize, PayloadRequirement)>`, and let `cut_predecessor` read it without a
registry or a rebuild. Cheaper at cut time and keeps the signature. Not chosen: it moves state
into `Plan` that has to be kept correct across the recipe prologue and serde, which is the
exact class of staleness that produced the level-0 bug in §1 and the double-execution bug
before it. The rebuild is bounded by the number of actions in the chain, happens only when a
plan is actually cut, and produces the very plan the boundary asset would build anyway.
Revisit if profiling ever says so.

## 3. Make the shape assertions policy-explicit

`absolute_outer_resource_keeps_relative_link_on_live_cwd` asserts the expanded step shape
twice. Both assertions are correct for the default and neither is what the test is about —
its subject is that an absolute query's own resource resolves against logical root while a
relative link follows the live CWD, which the value assertion already covers.

Assert the shape against the plan the test itself produces: keep the `Step::GetAsset("data")`
assertions on an explicitly un-cut plan, and let the value assertions run on whichever form
the policy yields. Measured: with the shape assertions relaxed the test passes under the cut,
producing `"root-data|linked"` with the context CWD still `a/c`.

## 4. The equivalence suite — E1 to E12, with a CWD axis

The issue's expected behaviour. Move `evaluate_both_ways` out of `interpreter.rs`'s
`#[cfg(test)] mod` into `liquers-core/tests/plan_cwd_freeze.rs`, where Phase 3 specified it,
and widen it on three axes:

**Compare four properties, not one.** Value, `is_volatile`, `payload_required`, and the
surfaced error (type, message, position) — Phase 3's contract. The volatility comparison is
what would have caught pitfall 3 (a volatile predecessor hidden behind a boundary, so the
command ran once instead of twice) without needing a call-counting test to be written for it.

**Vary the CWD.** Every shape runs three ways, because the harness's inability to vary this
is what let Cause 1 survive:

| Condition | How | Reaches |
|---|---|---|
| No CWD | `Recipe::new(q, "", "")`, `cwd: None` | today's coverage |
| Recipe CWD | `recipe.cwd = Some("a/c")` | **Cause 1** |
| Provider (keyed) recipe | recipe read from `a/c/recipes.yaml`, evaluated by key | the prologue *and* the keyed-asset path |

**Report, do not stop.** Table-driven over the twelve shapes with a per-shape result row, so
one run prints every divergence. The four remaining ones were found in one forced run of the
whole suite; a fail-fast harness would have surfaced them one release apart.

Shapes E1–E12 are specified in `plan-cwd-freeze/phase3-examples.md` and stand unchanged, E8
included. E2, E3, E4, E5 and E9 need a store, so they run on `SimpleEnvironment<Value>` with
an `AsyncMemoryStore` rather than `ImmediateEnvironment`; E7 and E8 need
`SimpleEnvironmentWithPayload<Value, String>`.

Two shapes are added for §2, both on `SimpleEnvironmentWithPayload`:

| # | Shape | Query | Covers |
|---|---|---|---|
| E13 | Mid-chain payload | `fetch/personalize/render`, `personalize` declaring `payload: required` | The cut steps back to `fetch`; the boundary query is frozen at that deeper level, not left relative |
| E14 | Head payload | `personalize/fetch/render` | No boundary is safe; `was_cut` is false and the value matches |

E13 is the one that would catch the freeze wrinkle: a stepped-back candidate whose operands
were left CWD-relative produces a boundary query that resolves against the wrong folder, which
is Cause 1 reappearing one level down. It runs under the recipe-CWD condition for that reason.

No production switch is needed to run them: the harness finalizes a plan and calls
`plan.cut_predecessor()` on a clone, which is exactly what `evaluate_both_ways` already does.
The `LQ_FORCE_CUT` probe from `analysis.md` is a measurement tool and is not landed.

## 5. Documentation

- `DOC_08_RECIPES_PLANS.md`, "Predecessor boundaries": add the pitfall — *a boundary query
  frozen before the prologue* — beside its sibling *a step-range recorded before a prefix is
  inserted*; both are the same prepended `SetCwd`, one compensated in the count and one in
  the cursor. Restate the "undeclared payload" row: the cut is now declined rather than made
  and broken. Add `prologue_steps` to the plan-fields table. `## History` row and `reviewed:`
  bump in the same commit.
- `PREDECESSOR-CUT-NOT-YET-EQUIVALENT`: closed by this design; its speculation that the two
  CWD failures came from "a nested keyed recipe re-deriving its own working key" is corrected
  in `analysis.md` §Cause 1.
- `CORE-PLAN-POLICY-AND-DEFAULTS`: the blocker note is updated — equivalence holds, and the
  remaining question is purely the per-query memory-versus-recomputation trade that issue
  already states.
- `specs/README.md`: the design folder is added to the map.

## 6. What this design does not do

It does not flip the default, add a policy knob, or introduce a per-query cut annotation.
Those are `CORE-PLAN-POLICY-AND-DEFAULTS`, and the reference already argues the trade is per
query rather than global. This design's product is the ability to make that decision on
evidence: after it, cutting is a choice about memory and scheduling, not a choice about
whether the answer is right.

It does settle *where* a boundary goes when one is cut — §2's walk back to the last
payload-free candidate — because that is a correctness question, not a policy one.
