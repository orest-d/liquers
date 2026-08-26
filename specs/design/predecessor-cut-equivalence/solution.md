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

## 2. A boundary is never cut across payload-sensitive steps

Cause 2 is not a defect in cutting and not a missing declaration. Confirmed with the author:
**an injected parameter does not imply a payload requirement** — `injected` means
`InjectedFromContext`, and a value may be injected from the environment alone. The correct
behaviour for payload processing is to *expand* the payload-sensitive part of the plan rather
than cut across it.

### The rule

A boundary may be cut only where every step behind it is payload-free. A boundary is a cache
entry; a payload is deliberately not part of a cache key; so a value computed from a payload
must never end up behind one.

### Where the plan learns this

`cut_predecessor` takes no registry and cannot ask a command whether it reads the payload.
`PlanBuilder` can — it holds the metadata when it emits each `Step::Action` — so the plan
records the answer:

```rust
// plan.rs, in `struct Plan`
/// Index of the earliest step whose execution reads the evaluation payload.
///
/// A boundary may only be cut *before* it: a boundary is a cache entry, and a payload is
/// not part of a cache key, so a value computed from one must not end up behind it.
#[serde(default)]
pub payload_sensitive_from: Option<usize>,
```

set by the builder, and shifted by `Recipe::to_plan`'s prologue exactly as `predecessor_steps`
is — one more reason §1 records `prologue_steps` rather than leaving each site to compensate
for the prefix on its own.

```rust
// plan.rs, in `cut_predecessor`, after the existing frozen / predecessor / range guards
if self.payload_sensitive_from
    .is_some_and(|first| first < self.predecessor_steps)
{
    return Ok(false);
}
```

### What counts as payload-sensitive, today and later

Today the builder has only one signal it can read: an action with an `injected` argument, or
one whose metadata declares `payload: required`. Since `injected` covers environment-sourced
injection too, that signal is a **conservative over-approximation** — it declines some cuts
that would have been safe. Safe is the right side to err on, and the cost is a lost
optimisation rather than a wrong answer.

Making it exact needs a way to say *this injection reads the payload*, which the registration
surface cannot currently express; filed as `PAYLOAD-SOURCED-INJECTION-NOT-DECLARED`. When it
exists, this design changes by one predicate at the single point that already has the
metadata.

### No exception for a declared payload

An earlier draft kept `payload: required` as an opt-in that would cut the boundary anyway,
on the ground that such a query is routed through `Context::schedule_payload_dependency_asset`
and evaluated inline with the payload forwarded. Dropped, because that route deliberately
registers no graph edge and creates no cache entry — so cutting a payload-requiring
predecessor buys none of the three things a boundary exists for (caching, independent
expiration, parallel scheduling) and costs an extra asset and an extra hop. Declining
unconditionally is both simpler and strictly better.

The upshot is that this design needs **no change to any command declaration**. `payload:
required` keeps exactly its present meaning for nested evaluation.

### Consequence for E8

Phase 3 wrote E8 as a deliberate *inequivalence* test, pinning the undeclared-payload case as
the one place the two forms differ, so "cutting is policy, not correctness" stayed
falsifiable. With this guard the two forms no longer differ there. E8 is restated as an
equivalence test with a structural assertion: *the value is identical, and `was_cut` is
false.* A divergence the code refuses to create cannot be shipped by accident, whereas a
documented one can.

### Follow-up: cut at the largest payload-free prefix

Declining is correct but not always maximal. `PlanBuilder` records exactly **one** predecessor
— `plan.rs:1716` assigns `predecessor_steps = steps.len()` at the outermost recursion level
whose remainder is a real action, overwriting every inner level — so the only lever available
is cut or do not cut.

Where the payload reader is at the head of the chain the two coincide: in
`authenticate/fetch/render` with `authenticate` reading the payload, every prefix contains it
and no boundary at any position is safe. Where the reader sits in the middle they do not: in
`fetch/personalize/render` the recorded predecessor `fetch/personalize` is payload-sensitive
and is declined, while `fetch` alone is a perfectly good boundary — cacheable and shared,
with `personalize/render` running inline per payload.

Reaching it means recording every candidate level rather than only the outermost, and cutting
at the last one whose range ends at or before `payload_sensitive_from`. That is an
optimisation on top of a correct rule, not part of it, so it is out of scope here and noted
for `CORE-PLAN-POLICY-AND-DEFAULTS`, which owns where a boundary should go.

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

Shapes E1–E12 are specified in `plan-cwd-freeze/phase3-examples.md`; they stand unchanged
except E8 (§2 above). E2, E3, E4, E5 and E9 need a store, so they run on
`SimpleEnvironment<Value>` with an `AsyncMemoryStore` rather than `ImmediateEnvironment`;
E7 and E8 need `SimpleEnvironmentWithPayload<Value, String>`.

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
