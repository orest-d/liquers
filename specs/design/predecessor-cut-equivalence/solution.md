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

## 1b. Coupled plan fields are carried by construction

`Plan::split` builds both halves with `Plan::new()` and then copies a field list — `query`,
`init_steps`, `steps`, `is_volatile`, `payload_required`, `expires`, `error`, `dependencies`.
It does not copy `frozen_cwd`, `predecessor` or `predecessor_steps`, and would not copy
`prologue_steps`. Both halves therefore report `predecessor: None` and, more consequentially,
`frozen_cwd: None` — a half is silently *un-frozen*, so it would accept a re-freeze against a
different key that the whole plan refuses, and fail `cut_predecessor`'s frozen guard.

Filed as `PLAN-SPLIT-DROPS-PREDECESSOR-FIELDS` and originally left out of scope on the ground
that `split` has no production caller (confirmed: the only call sites are `plan.rs`'s own
tests). Brought in, because the omission is not the interesting part — **the field list is**.
Every defect this design lineage has found is the same shape: a plan mutated through a subset
of coupled fields.

| Where | What went stale |
|---|---|
| `Recipe::to_plan` inserting `SetCwd` | `predecessor_steps`, until `plan-cwd-freeze` bumped it — a cut ran the predecessor's action twice |
| `Plan::freeze_cwd_with` | the cursor for `predecessor`, still stale at HEAD — §1, Cause 1 |
| `Plan::split` | `frozen_cwd`, `predecessor`, `predecessor_steps` — latent |

Three instances, two of them shipped. So the change is structural rather than a one-time
top-up: **build each half from `self.clone()` and replace only what differs**, so a field
added to `Plan` later is carried by construction and a field that must *not* be carried has to
be cleared deliberately, in the diff, where a reviewer sees it.

### What each half should carry

Measured while scoping this, and it makes the naive fix wrong:

```
fetch/expensive/render           steps=3 split_index=2 predecessor_steps=2
fetch/expensive/render/out.txt   steps=4 split_index=2 predecessor_steps=2
-R/./a.txt/-/fetch/render        steps=3 split_index=2 predecessor_steps=2
recipe cwd=a/c, 4-action query   steps=5 split_index=3 predecessor_steps=3
```

`split_index == predecessor_steps` on every shape, prologue included. **The first half is
exactly the predecessor's steps.** So copying `predecessor` into it — which is what the issue
as filed proposed — gives a half whose `predecessor_steps == steps.len()`: it passes
`cut_predecessor`'s range guard and §2's step-count cross-check, and cuts every step into a
boundary that recomputes the same thing. A degenerate wrapper rather than a wrong value, but
not what anyone means by splitting.

The first half's genuine predecessor is one level deeper, and `split` has no registry to build
it. So:

| Field | First half | Second half | Why |
|---|---|---|---|
| `frozen_cwd` | carried | carried | A fact about the operands, true of each half independently |
| `predecessor`, `predecessor_steps` | cleared | cleared | A boundary is a property of a whole plan; a fragment has none, and `Ok(false)` from `cut_predecessor` is the honest answer |
| `prologue_steps` | carried, clamped | `0` | The non-query-derived prefix is leading, so it is in the first half |

### Invariants worth asserting

`prologue_steps <= steps.len()`, and when `predecessor.is_some()`,
`prologue_steps <= predecessor_steps <= steps.len()`. A `debug_assert`-backed
`Plan::assert_consistent()` called after `build`, after `Recipe::to_plan`'s insert, after
`split` and after `cut_predecessor` costs nothing in release and would have caught the
double-execution bug at its source rather than through a failing evaluation two layers away.

### Noted, not acted on

`split_index()` rescans the steps to derive a number `predecessor_steps` already records, and
they agreed on every shape measured. That is a plausible simplification and also exactly the
kind of coincidence that should be *pinned by a test* before anything relies on it — they are
derived by different means, one from the step list and one from the query recursion. A test
asserting the equality across the suite's shapes is cheap; collapsing one into the other is
not part of this design.

## 2. The cut walks back to the last cacheable boundary

Cause 2 is not a defect in cutting. A boundary is a cache entry; a payload is deliberately
not part of a cache key; so a value computed from a payload must never end up behind one. The
correct behaviour is to leave the payload-sensitive part of the plan **expanded** and cut, if
at all, in front of it.

Cause 4 turns out to want the same walk, so the two are one rule: **cut at the last candidate
that can be cached.** A candidate cannot be cached if it needs a payload, or if it is volatile.
The justification is identical in both halves — a boundary that cannot be cached buys none of
the three things a boundary exists for (caching, independent expiration, parallel scheduling)
and costs an extra asset and an extra hop.

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

Build the candidate boundary's plan and cut only if that plan is cacheable — no payload
required, not volatile. Otherwise step back one level and try the shorter candidate; if none
qualifies, do not cut.

One declaration cannot be seen this way and is handled separately: a **recipe-level**
`volatile:` or `expires:` is not in any query, so no candidate's plan can reveal it (Cause 4,
measured 2 → 1). `Recipe::to_plan` has the recipe in hand, so it records the fact on the plan —
`uncuttable: Option<String>`, carrying the reason — and `cut_predecessor` returns `Ok(false)`
before the walk starts.

`expires:` is the weaker half of that guard: a finite expiration says the *result* should be
refreshed, and if the prefix is pure, caching it is still sound. Treated conservatively here
because the flag exists to cover what the system cannot infer; relaxing it to `volatile:` only
is a one-line change if it proves too blunt.

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

    // A recipe-level `volatile:` / `expires:` is in no query, so the walk below cannot see it.
    if let Some(reason) = &self.uncuttable {
        self.init_info(format!("Predecessor boundary not cut: {reason}"));
        return Ok(false);
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
        if candidate.payload_required.is_none() && !candidate.is_volatile {
            break;                                        // cacheable: safe to make it an asset
        }
        // Say why this level was passed over, naming the command responsible.
        self.init_info(boundary_expansion_reason(&candidate));
        candidate.freeze_cwd_with(&mut base.clone())?;    // resolve the next candidate's operands
        let Some(inner) = candidate.predecessor.clone() else {
            return Ok(false);                             // it reaches the head; nothing to cut
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

Volatility behaves identically in the walk, since it is the same predicate on the same
candidate plan: `prefix/vol_prefix/tail` steps back to `prefix`. Measured separately, a
**command**-level volatile boundary is already equivalent without any of this — the boundary
query carries the volatile command, so the asset manager evaluates it as a volatile query and
it recomputes (2 runs both ways). Including volatility in the walk is therefore not a
correctness fix for that case but the same "do not create an uncacheable boundary" rule, and it
avoids allocating an asset per evaluation that is guaranteed to be recomputed.

Step counts line up at every level — the parent's `predecessor_steps` equals the candidate
plan's `steps.len()`, and the candidate's `predecessor_steps` equals the next one's — which is
what makes `cut_at` derivable from the recursion rather than needing to be recorded.

**What this measurement does not cover.** These queries were built raw. The real input is
`plan.predecessor`, which is *promoted* (relative default links made explicit) and *frozen*
(operands absolute). Neither should change a step count — promotion turns a `DefaultLink` into
a `ParameterLink` on the same step, and freezing rewrites operands in place — but that is
reasoning, not measurement, and it is the load-bearing assumption of this section. The
step-count cross-check turns a violation into "no cut" rather than a mis-split, so the failure
mode is a silently lost boundary, not a wrong value. Measure it on promoted and frozen input as
the first implementation step, and pin it with a test.

`with_placeholders_allowed()` appears in the sketch and is **not** established. Recipe overrides
patch only the last action, which is in the tail, so a recorded predecessor should be
placeholder-free; if that holds, drop it and let a placeholder be the error it would be.

### Saying why a boundary was expanded

Every place the walk passes over a level, and the place it declines outright, appends a
planning `Plan::init_info` naming the reason and the command responsible:

```
Predecessor boundary expanded at 'personalize': command requires an evaluation payload
Predecessor boundary expanded at 'vol_prefix': command is volatile
Predecessor boundary not cut: recipe declares volatile: true
```

`init_info` rather than `Step::Info`: this is a fact established once at planning time, and
`init_steps` are copied into metadata rather than re-logged on every execution. Without it, a
declined cut is indistinguishable from a plan that had no predecessor — which is exactly the
kind of silence that let the four divergences in this issue sit unexplained.

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

## 4. The equivalence suite — E1 to E16, with a CWD axis

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

Four shapes are added for §2 — E13/E14 on `SimpleEnvironmentWithPayload`, E15/E16 on a
store-backed `SimpleEnvironment` with keyed recipes and a call counter:

| # | Shape | Query | Covers |
|---|---|---|---|
| E13 | Mid-chain payload | `fetch/personalize/render`, `personalize` declaring `payload: required` | The cut steps back to `fetch`; the boundary query is frozen at that deeper level, not left relative |
| E14 | Head payload | `personalize/fetch/render` | No boundary is safe; `was_cut` is false and the value matches |
| E15 | Recipe-level volatility | `prefix/tail/out.txt` in a recipe with `volatile: true` | §2 — the prefix runs the same number of times both ways (measured 2 vs 1 without the guard) |
| E16 | Command-level volatility, mid-chain | `prefix/vol_prefix/tail` | The walk steps back to `prefix`; and the already-equivalent baseline stays equivalent |

### What "equivalent" means, stated

Cutting *does* change observable things by design: it creates an asset, adds a dependency edge,
and writes a dependency record into the parent's metadata. Phase 3's four properties — value,
`is_volatile`, `payload_required`, surfaced error — are therefore the definition, not an
abbreviation of one. Asset count, dependency records and log contents are expected to differ
and are not compared. Worth stating in the suite's header comment, because the next person to
add a shape will otherwise reach for a metadata assertion and find a difference that is the
feature working.

E13 is the one that would catch the freeze wrinkle: a stepped-back candidate whose operands
were left CWD-relative produces a boundary query that resolves against the wrong folder, which
is Cause 1 reappearing one level down. It runs under the recipe-CWD condition for that reason.

No production switch is needed to run them: the harness finalizes a plan and calls
`plan.cut_predecessor()` on a clone, which is exactly what `evaluate_both_ways` already does.
The `LQ_FORCE_CUT` probe from `analysis.md` is a measurement tool and is not landed.

## 5. Documentation

Two audiences, and the mechanism has to be readable from both. Rustdoc is where someone writing
a recipe or reading `PlanBuilder` will look; `reference/api` is where someone asking "how do
boundaries work" will look. Neither is a summary of the other.

### Rustdoc — `Recipe`

`Recipe::volatile` currently reads, in full: *"Forces volatile evaluation in addition to
volatility inferred from the plan."* True, and it answers none of the questions this design
raised. Replace it with the meaning and its consequence:

> Marks the whole plan volatile, in addition to volatility inferred from it.
>
> A volatile recipe is volatile **from its first action**, not merely in its result. Nothing in
> it is cached, and no predecessor boundary is cut out of it — a boundary is a cache entry, and
> a plan declared volatile is one whose intermediates must not be cached. The alternative,
> volatility that applies only to the last action, produces an asset that is dutifully
> recomputed and restores the same cached prefix every time: volatile in name, fixed in value.
>
> This flag carries no position, so it cannot mark where a non-volatile part of a plan ends.
> The positional instrument is the `v` instruction. Use `volatile: true` to say *this recipe is
> volatile*, which is the case it is for — covering impurity a command did not declare.

`Recipe::expires` gains the same note in short form: a recipe-level expiration also makes the
plan uncuttable, and why that is the conservative reading rather than an obviously correct one.

`Recipe::to_plan`'s doc comment gains the two things it now does beyond building: it records
`prologue_steps` for the `SetCwd` it prepends, and `uncuttable` when the recipe declares
volatility or expiration — with a sentence on why each cannot be recovered later (one because
the prefix is indistinguishable from a query-authored `cwd`, the other because it is in no
query).

### Rustdoc — `PlanBuilder` and `Plan`

`PlanBuilder`'s type doc says it works "without an environment" and that dependency-derived
values come later. Add what it records *for* the later passes and does not itself act on:
`predecessor` / `predecessor_steps` for a boundary it never cuts, and the volatility and
payload facts a cut will consult per candidate. The existing `// TODO: support volatile flags`
marker beside it is `CORE-PLAN-POLICY-AND-DEFAULTS`; leave it, but the doc should no longer
imply volatility is unhandled.

`Plan::cut_predecessor` carries the rule itself, since that is where a reader lands from a
stack trace or an `init_info` message: cut at the last candidate that can be cached; a
candidate cannot be cached if its plan requires a payload or is volatile; a recipe-level
declaration is consulted first because no candidate can show it. Include the measured 2 → 1, in
one line — it is the difference between a rule a reader follows and one they work around.

`Plan::uncuttable` and `Plan::prologue_steps` get field docs at the same standard as the three
`plan-cwd-freeze` added.

### `reference/api/DOC_08_RECIPES_PLANS.md`

"Predecessor boundaries" gains:

- **Where a boundary goes** — a new subsection ahead of "Pitfalls", since it is now a rule
  rather than a single position. The walk, the two predicates, the recipe-level guard, and the
  `init_info` messages an author will actually see.
- Two pitfall rows: *a boundary query frozen before the prologue* (beside its sibling, *a
  step-range recorded before a prefix is inserted* — both the same prepended `SetCwd`, one
  compensated in the count and one in the cursor), and *a recipe-level flag is not in the
  query*, with the measured 2 → 1.
- The "undeclared payload" row stays as it is; it is still the rule.
- `prologue_steps` and `uncuttable` in the plan-fields table.

The `v` instruction is currently one clause — *"marks a plan volatile without creating an
action step"*. Give it its own paragraph in "Building a plan": it is intercepted by the builder
like `q` and `ns`, takes no parameters, emits no step and so is an identity on the value, and
marks the **whole** plan volatile regardless of where it appears. That last point is the one a
reader will get wrong, and it is what
`V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL` proposes to change.

`## History` row and `reviewed:` bump in the same commit, per `DOCS_STRUCTURE_GUIDE` §9.2.

### The rest

- `PREDECESSOR-CUT-NOT-YET-EQUIVALENT`: closed by this design; its speculation that the two
  CWD failures came from "a nested keyed recipe re-deriving its own working key" is corrected
  in `analysis.md` §Cause 1.
- `CORE-PLAN-POLICY-AND-DEFAULTS`: the blocker note is updated — equivalence holds, and the
  remaining question is the per-query memory-versus-recomputation trade that issue already
  states.
- `specs/README.md`: the design folder is added to the map.

## 6. What this design does not do

It does not flip the default, add a policy knob, or introduce a per-query cut annotation.
Those are `CORE-PLAN-POLICY-AND-DEFAULTS`, and the reference already argues the trade is per
query rather than global. This design's product is the ability to make that decision on
evidence: after it, cutting is a choice about memory and scheduling, not a choice about
whether the answer is right.

It does settle *where* a boundary goes when one is cut — §2's walk back to the last
payload-free candidate — because that is a correctness question, not a policy one.
