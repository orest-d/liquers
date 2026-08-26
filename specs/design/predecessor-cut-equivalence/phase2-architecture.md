# Phase 2: Solution & Architecture - Predecessor Cut Equivalence

## Overview

Cutting at the outermost cacheable predecessor becomes the default. The architecture that takes
it there is small: two fields on `Plan`, one changed signature, one structural change to how a
`Plan` is copied, a placement rule computed per candidate rather than per plan, and the fold of a
recipe's own declarations into the plan it builds.

One new enum. No new trait, no new command, no new error variant. Everything is inside
`liquers-core`, and the crate dependency flow is untouched.

Five causes drive it. Four were measured at `d1bd02e` by forcing the cut on; the fifth was found
by reading and then measured.

| # | Cause | Divergences | Verdict |
|---|---|---|---|
| 1 | The predecessor query is frozen against the *entry* CWD, one step before the recipe's `SetCwd` prologue | 2 (`recipe_cwd_resolution`) | **Defect.** §"Recording the prologue" — fix verified |
| 2 | A cut boundary is a cache entry, and a payload is not part of a cache key | 1 (`injection`) | Mis-declared command, fixed in the test; exposes the placement rule |
| 3 | A test asserts the *expanded* plan's step shape | 1 (`--lib`) | Not a defect; measured equivalent in value |
| 4 | A whole-plan volatility declaration (`v`, a recipe `volatile:`) is in no candidate query, so it does not reach a boundary | 0 measured | **Defect.** §"The `v` instruction, and volatility scope" |
| 5 | `Plan::split` copies a field list and drops the coupled predecessor fields | 0, latent | In scope — the field list is the shape of every defect here |

## Known-Issue Preflight

### Open issues bearing on this design

| Issue | Status / Pri | Bearing | Blocking? |
|---|---|---|---|
| `PREDECESSOR-CUT-NOT-YET-EQUIVALENT` | draft P1 | The issue this design closes. | — |
| `CORE-PLAN-POLICY-AND-DEFAULTS` | accepted P2 | Its `expand_predecessors` default half is **answered** by this design; the `cache`, `volatile flags` and `inline flag` markers are untouched. | No |
| `PLAN-SPLIT-DROPS-PREDECESSOR-FIELDS` | accepted P2 | In scope, §"Coupled fields". Raised P3 → P2 during Phase 1. | No |
| `RECIPE-TO-PLAN-IGNORES-RECIPE-LEVEL-VOLATILE-AND-EXPIRES` | draft P2 | Filed *from* this preflight, then **taken into scope** at the author's direction. §"Folding the recipe's declarations into the plan". | No — fixed here |
| `V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL` | draft P3 | Complement: the positional volatility instrument. Today's `v` is consistent with the decision taken here, so nothing depends on it. | No |
| `CORE-ASSET-GC` | accepted, L | Owns the memory counterweight to cutting — a retention policy, per Phase 1. Not a prerequisite. | No |
| `PAYLOAD-SOURCED-INJECTION-NOT-DECLARED` | rejected P3 | Filed and rejected during Phase 1; the payload need is on command metadata. | No |
| `RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING` | draft P3 | Adjacent; `get_asset_info` reruns the analysis passes unfrozen. Does not touch the cut. | No |
| `CWD-KEY-LINK-NOT-CONSUMABLE-BY-COMMAND` | P1 | Blocked `plan-cwd-freeze` steps 8-12. That was about narrowing `Context::get_cwd_key`, which this design does not touch. | No |

**No blocker.** Nothing must be resolved before this design proceeds, and no blocker sits below P1.

### Measurements taken for this phase

Two Phase 1 confirmation items, both now measured rather than reasoned.

**The recorded predecessor rebuilds to the same step count** — the assumption the placement walk
rests on. Probed across promotion, freezing, absolute queries and an inner `cwd` instruction,
asserting `predecessor_steps - prologue_steps == rebuilt.steps.len()`:

```
plain, no cwd            recorded=a/b                                    2-0 == 2  ok
plain, recipe cwd        recorded=a/b                                    3-1 == 2  ok
relative resource, cwd   recorded=-R/proj/x/in.csv/-/a                   3-1 == 2  ok
absolute query, cwd      recorded=/-R/in.csv/-/a                         3-1 == 2  ok
promoted default link    recorded=siblings-~X~-R-key/proj/x~E            2-1 == 1  ok
promoted link + more     recorded=siblings-~X~-R-key/proj/x~E/a          3-1 == 2  ok
filename tail, cwd       recorded=a/b                                    3-1 == 2  ok
cwd instruction inside   recorded=-R-cwd/proj/x/sub/-R/proj/x/sub/in.csv/-/a  4-1 == 3  ok
```

**The rebuild must allow placeholders**, and this is not cosmetic. `Recipe::to_plan` builds with
`with_placeholders_allowed()`, because a recipe may fill a missing argument through its
`arguments:`/`links:` overrides. Measured on a query whose *non-last* action omits a required
argument:

```
parent Recipe::to_plan            -> Ok, recorded predecessor = needs_arg
rebuild WITH placeholders         -> Ok(1 step)
rebuild WITHOUT placeholders      -> Err("Missing argument 'x' (pop_value)")
```

Without it the cut would **fail where expansion proceeds**. Since Phase 1 fixed the expanded plan
as the oracle, the cut must never be stricter than it. The rebuild mirrors the parent's setting.

## Data Structures

One new field, and one marker whose form is the phase's open decision.

```rust
pub struct Plan {
    // ... existing fields ...

    /// Number of leading [`Self::steps`] that were *not* emitted by the builder for
    /// [`Self::query`] — a recipe's CWD prefix.
    ///
    /// Three places currently infer this independently and differently. Recording it once
    /// makes the recipe prefix a fact rather than a guess, and lets an index recorded against
    /// the query's own steps survive the insert.
    #[serde(default)]
    pub prologue_steps: usize,

    /// Where this plan's volatility came from, which decides whether it may be cut at all.
    ///
    /// [`VolatilitySource::Declared`] is a statement about the *whole* plan and is in no
    /// query, so no candidate boundary's own plan can reveal it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volatility_source: Option<VolatilitySource>,
}

/// How a plan came to be volatile. Closed set, so a new source is a compile error at every
/// match rather than a silent fallthrough.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VolatilitySource {
    /// A volatile command, or a volatile dependency. **Positional**: the prefix ahead of it is
    /// pure, so a boundary may be cut in front of it.
    Positional,
    /// A whole-plan declaration — the `v` instruction, a recipe's `volatile: true`, or a
    /// recipe expiration that is itself volatile. **Not positional**: it says nothing here is
    /// cacheable, so the plan may not be cut at all.
    Declared,
}
```

`Declared` outranks `Positional` when both are present; the builder keeps the stronger.

**This replaces the `uncuttable` marker of an earlier draft**, and it is the `v` instruction that
forced the better shape — see §"The `v` instruction". One field now covers three declarations
(`v`, `recipe.volatile`, a volatile `recipe.expires`) instead of a recipe-only marker that `v`
would have had to route around.

`#[serde(default)]` on both fields, matching the three `plan-cwd-freeze` added — a plan
serialized before this change loads at the pre-change values.

## Trait Implementations

`VolatilitySource` derives `Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize`. No
`Display`: the `init_info` sentences differ per call site (which command, which declaration), so
they are composed where the context is known rather than rendered from the enum alone.

`Plan` gains nothing: `usize` and `Option<VolatilitySource>` are covered by every derive already
on it. No trait is added, changed, or given a new bound anywhere — which is what keeps this
change from reaching `liquers-py`, whose implementors would break on a trait signature change.

## Sync vs Async

Everything here is **synchronous**, deliberately.

`cut_predecessor` runs `PlanBuilder`, which is sync by construction: it resolves command metadata
from a registry it holds by reference, and touches no store, no asset and no environment. Making
the cut async to accommodate it would push `async` up through `finalize_plan`'s callers for no
gain, and would invite a later implementer to do I/O inside a plan transform — the thing the
builder's sync-ness currently prevents.

The async boundary stays where it is: `finalize_plan` is async because
`has_volatile_dependencies` and `has_expirable_dependencies` consult assets. The cut is a
synchronous transform applied after them, on data already in hand. No blocking I/O is introduced
in an async context, because no I/O is introduced at all.

## Function Signatures

```rust
// plan.rs — changed: gains the registry, so it can build each candidate boundary's plan.
impl Plan {
    pub fn cut_predecessor(
        &mut self,
        cmr: &CommandMetadataRegistry,
    ) -> Result<bool, Error>;

    /// Checks the invariants over the coupled predecessor fields.
    pub(crate) fn check_consistent(&self) -> Result<(), Error>;
}

// plan.rs — unchanged signatures whose behaviour changes
impl Plan {
    pub(crate) fn freeze_cwd_with(&mut self, cursor: &mut CwdCursor) -> Result<(), Error>;
    pub fn split(&self) -> (Plan, Plan);
}

// recipes.rs — unchanged signature; now records `prologue_steps` and `volatility_source`,
// and folds its own `volatile:` / `expires:` into the plan
impl Recipe {
    pub fn to_plan(&self, cmr: &CommandMetadataRegistry) -> Result<Plan, Error>;
}
```

`check_consistent` returns `Result` rather than being a `debug_assert!` helper: library code must
not panic, and every call site here already has a `Result` to propagate into. Where a caller has
none, `debug_assert!(plan.check_consistent().is_ok())` is still available without putting a panic
on a release path.

`cut_predecessor`'s signature change is breaking. Verified across the workspace: the only callers
are `liquers-core`'s own tests. `Environment` and `EnvRef` both expose
`get_command_metadata_registry(&self) -> &CommandMetadataRegistry`, so every prospective call site
already holds one, borrowed — no `Arc`, no clone, no lifetime on `Plan`.

## Integration Points

| Crate / module | Change |
|---|---|
| `liquers-core/src/plan.rs` | `prologue_steps`, `volatility_source`, `VolatilitySource`; the prologue walk in `freeze_cwd_with`; the candidate walk in `cut_predecessor`; `split` rebuilt from `self.clone()`; `check_consistent` |
| `liquers-core/src/recipes.rs` | `to_plan` records `prologue_steps` and `volatility_source`, folds `volatile:`/`expires:` into the plan, beside the existing `predecessor_steps` bump |
| `liquers-core/src/interpreter.rs` | `finalize_plan` calls the cut — **this is the default flip**; the harness moves out of its `#[cfg(test)] mod`; one test's shape assertions become policy-explicit |
| `liquers-core/tests/` | `plan_cwd_freeze.rs` gains the suite; `injection.rs` gains two `payload: required` declarations |

Nothing outside `liquers-core`. `liquers-lib --lib --tests` was measured green under a forced cut
with the prologue fix applied, so its `apply_recipe` inherits the change without its own call.

## The three mechanisms

### Recording the prologue (verified)

`freeze_cwd_with` resolves `self.predecessor` from the cursor's entry state:

```rust
// The predecessor is the leading steps, so it resolves from the entry state of this walk.
if let Some(predecessor) = &mut self.predecessor {
    let mut scoped = cursor.clone();
    *predecessor = scoped.resolve_query_scoped(predecessor);
}
```

True for a plan built from a query; false for one built from a recipe with `cwd:`, because
`Recipe::to_plan` prepends a `Step::SetCwd` the builder never emitted. The *count* is compensated
(`predecessor_steps += 1`); the *cursor* is not. The fix advances a scoped cursor over the
prologue's `SetCwd` steps before resolving:

```rust
for step in self.steps.iter().take(self.prologue_steps) {
    if let Step::SetCwd(key) = step {
        scoped.set_cwd_from(key);
    }
}
```

Measured: both `recipe_cwd_resolution` divergences clear, `liquers-core` stays green with the cut
off, `liquers-lib` green with it on.

### Where a boundary goes

Cut at the last candidate that can be **cached**. A candidate cannot be cached if its plan
requires a payload or is volatile — one rule, two predicates, and the justification is the same
for both: a boundary that cannot be cached buys none of the three things a boundary exists for
and costs an extra asset and an extra hop.

`Plan::payload_required` and `Plan::is_volatile` are whole-query flags and are the wrong
granularity, wrong in *both* directions: as a veto they discard the boundary in
`fetch/expensive/render_with_payload`, where everything behind the only candidate is clean; as a
permit they cut straight across `fetch/personalize/render`.

The builder already computes what is needed. Measured by instrumenting its recording point: it
recurses into the predecessor first, so on the way back up it visits every prefix in order and
holds, at each one, the promoted prefix query, that prefix's step count, and the *cumulative*
flags for that prefix. It then keeps only the longest.

```
prefix/vol/tail/render      steps  volatile  payload   remainder_is_action
  prefix                      1      false     none       true
  prefix/vol                  2      true      none       true     <- volatility enters here
  prefix/vol/tail             3      true      none       true
```

Whether the builder **keeps** that list or `cut_predecessor` recovers it by rebuilding is an
implementation detail (Phase 1). Either satisfies the rule; the recorded-list form trades a
rebuild for state that must survive the prologue and serde, and if taken should record indices
against the query's own steps so the prefix insert cannot invalidate them.

Two candidates are excluded regardless: one whose `remainder_is_action` is false — a trailing
filename, where cutting leaves the parent nothing but a `Filename` step and a recipe's overrides
nothing to patch — and every candidate in a plan whose volatility is `Declared`.

Every level passed over, and the decline, appends a planning `Plan::init_info` naming the command
and the reason:

```
Predecessor boundary expanded at 'personalize': command requires an evaluation payload
Predecessor boundary expanded at 'vol_prefix': command is volatile
Predecessor boundary not cut: the plan is declared volatile
```

`init_info` rather than `Step::Info`: this is established once at planning time, and `init_steps`
are copied into metadata rather than re-logged on every execution. Without it a declined cut is
indistinguishable from a plan that had no predecessor.

**The freeze wrinkle.** `freeze_cwd` resolves `plan.predecessor`, so the longest candidate arrives
frozen. A shorter candidate recovered by rebuilding is built fresh from source and is **not**:
its operands are still CWD-relative, and cutting on it reproduces cause 1 one level down. Each
such candidate is frozen against a clone of the prologue-advanced cursor before its own
predecessor is read — reusing `freeze_cwd_with` rather than writing a second cursor walk.

### The `v` instruction, and volatility scope

`v` is checked here because it is the case that breaks a naive walk. Measured by instrumenting
the builder's candidate recording point:

```
a/b/c        a(1,false)  a/b(2,false)                 -> steps=3  pred=a/b(2)  plan.volatile=false
a/v/b/c      a(1,false)  a/v(1,TRUE)   a/v/b(2,TRUE)  -> steps=3  pred=a/v/b(2) plan.volatile=true
a/b/v        a(1,false)  a/b(2,false)                 -> steps=2  pred=a/b(2)  plan.volatile=TRUE
v/a/b        v(0,TRUE)   v/a(1,TRUE)                  -> steps=2  pred=v/a(1)  plan.volatile=true
a/b/v/out.txt a(1,false) a/b(2,false)  a/b/v(2,TRUE,filename) -> steps=3 pred=a/b(2) plan.volatile=true
```

Three obstacles, in increasing severity.

**1. `v` emits no step, so candidate → step index is not injective.** `a/b` and `a/b/v` both
report 2 steps. Any bookkeeping that identifies a cut position by step count cannot tell them
apart, and the step-count cross-check would pass for the wrong candidate.

**2. `v` at the end defeats itself.** In `a/b/v` the outermost non-volatile prefix is `a/b`,
which is the *entire* plan: `predecessor_steps == steps.len() == 2`, and the existing guard
(`predecessor_steps > steps.len()`) does not catch equality. Cutting yields `[Evaluate(a/b)]`
with an **empty tail** — a volatile parent whose whole content is one cached boundary. The parent
dutifully recomputes and restores the same cached value every time. `v` becomes a no-op. Same for
`a/b/v/out.txt`, which leaves only a `Filename`.

**3. The root: one flag, two meanings.** In `a/v/b/c` the walk lands on `a` — correct if `v`
means *volatile from here onward*, wrong if it means *this plan is volatile*, which is what the
implementation does (`mark_volatile` sets the builder's single whole-plan flag; `plan.rs:1486`).

The three collapse into one distinction the builder does not currently draw. **Volatility has two
scopes:**

- **Positional** — a volatile command, or a volatile dependency. Volatility is a property *of
  that command*; everything ahead of it is genuinely pure, so caching the prefix is sound and
  cutting in front of it is right. This is the case measured as already equivalent (2 runs both
  ways).
- **Declared** — `v`, `recipe.volatile`, a volatile `recipe.expires`. A statement about the
  whole, carrying no position: *nothing here is cacheable*. The plan may not be cut.

`PlanBuilder` collapses both into `is_volatile`, which is why `v` looked like an obstacle rather
than a missing distinction. `Plan::volatility_source` draws it, and the rule follows:

```rust
// cut_predecessor, before the walk
if self.volatility_source == Some(VolatilitySource::Declared) {
    self.init_info("Predecessor boundary not cut: the plan is declared volatile".to_owned());
    return Ok(false);
}
```

All three obstacles become unreachable: a plan containing `v` is never cut, so the degenerate
empty tail cannot arise and the non-injective index is never consulted.

**`v` does not need redesigning for this design.** Its current whole-plan meaning is exactly
`Declared`, and that is the meaning Phase 1 already chose for a volatile recipe — *volatile from
the first action*, because a last-action reading yields an asset dutifully recomputed from a
fixed cache. The obstacles come from the missing scope distinction, not from `v`.

`V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL` (P3) remains the place to reconsider that: were `v`
positional, it would become a `Positional` source, the walk would cut in front of it, and an
author's declared volatility boundary and the cache boundary would coincide. That is a more
expressive language, and it would need obstacles 1 and 2 answered — a step for `v`, or an index
that does not rely on step counts, and a diagnostic for the now-meaningless `a/b/v`. Out of scope
here; nothing in this design forecloses it.

### Folding the recipe's declarations into the plan

Taken into scope at the author's direction, from
`RECIPE-TO-PLAN-IGNORES-RECIPE-LEVEL-VOLATILE-AND-EXPIRES` (P2). Measured, `Recipe::to_plan`
reads neither of its own declarations:

```
recipe.volatile = true        -> plan.is_volatile = false
recipe.expires  = Immediately -> plan.is_volatile = false, plan.expires = Never
```

so a recipe preview under-reports both (`get_asset_info` fills `AssetInfo` straight from the
plan), and any consumer asking "is this plan volatile" has to know to consult the recipe too.
`to_plan` folds them in:

```rust
if self.volatile || self.expires.is_volatile() {
    plan.is_volatile = true;
    plan.volatility_source = Some(VolatilitySource::Declared);
}
if !self.expires.is_never() {
    plan.expires = plan.expires.clone().combine(self.expires.clone());
}
```

The second line is what `Recipe::expires`'s own doc comment already promises — "Recipe-level
expiration combined with finalized plan expiration" — and did not do.

Per Phase 1, a **finite** `expires:` does not block a cut: it bounds how long the resulting asset
stays valid, not the purity of the computation. Only `Expires::is_volatile()` — true for
`Immediately`, or a `Combination` containing one — contributes a `Declared` source. That is the
same predicate `resolve_volatility_before_evaluation` already ORs at `assets.rs:1610`, reused
rather than reinvented.

**Blast radius, measured rather than assumed.** The issue flagged that `finalize_plan` skips
dependency registration for a volatile plan (`if !plan.is_volatile { … }`), so folding makes a
volatile *recipe* stop registering plan dependencies — as a volatile *plan* already does. Applied
as a probe, **all 19 `liquers-core` suites stay green**, and the cross-crate `liquers-lib` loop
was run too. Green is not proof: no existing test asserts a volatile recipe's dependency records,
so Phase 3 owes one. Recorded as the fold's specific risk rather than waved through.

### Coupled fields are carried by construction

`Plan::split` builds both halves with `Plan::new()` and copies a field list, dropping
`frozen_cwd`, `predecessor` and `predecessor_steps` — so a half is silently *un-frozen*. It has
no production caller; the field list is the point. This is the third instance of one shape:

| Where | What went stale |
|---|---|
| `Recipe::to_plan` inserting `SetCwd` | `predecessor_steps`, until `plan-cwd-freeze` bumped it — a cut ran the predecessor's action twice |
| `Plan::freeze_cwd_with` | the cursor resolving `predecessor` — cause 1, live at HEAD |
| `Plan::split` | `frozen_cwd`, `predecessor`, `predecessor_steps` — latent |

Two of the three shipped. So both halves are built from `self.clone()` with only what differs
replaced, and a field that must *not* be carried is cleared deliberately, in the diff.

Measured, `split_index == predecessor_steps` on every shape tried, prologue included — the first
half **is** the predecessor's steps. So copying `predecessor` into it would give
`predecessor_steps == steps.len()`, passing every guard and cutting every step into a boundary
that recomputes the same thing.

| Field | First half | Second half |
|---|---|---|
| `frozen_cwd` | carried | carried |
| `predecessor`, `predecessor_steps` | cleared | cleared |
| `volatility_source` | carried | carried |
| `prologue_steps` | carried, clamped | `0` |

`check_consistent` covers `prologue_steps <= steps.len()`, and
`prologue_steps <= predecessor_steps <= steps.len()` when `predecessor.is_some()`, called after
`build`, after `to_plan`'s insert, after `split` and after `cut_predecessor`.

## Documentation Architecture

Phase 1 decided extend-not-new for the reference, no guide, no other documents.

| Path | Kind | Audience | Change |
|---|---|---|---|
| `liquers-core/src/recipes.rs` | rustdoc | recipe authors, contributors | `Recipe::volatile` — replace the one-line comment with the meaning (volatile **from the first action**), the consequence (nothing cached, no boundary cut), why the last-action reading fails, and that the positional instrument is `v`. `Recipe::expires` — that it bounds the result's validity, is combined into the plan's expiration, and does *not* block a cut unless itself volatile. `Recipe::to_plan` — the two facts it now records and why neither is recoverable later |
| `liquers-core/src/plan.rs` | rustdoc | contributors | `PlanBuilder` type doc — what it records for later passes and does not act on. `Plan::cut_predecessor` — the placement rule and the measured 2 → 1, since this is where a reader lands from a stack trace or an `init_info` line. `prologue_steps`, `volatility_source`, `VolatilitySource` field and variant docs at the `plan-cwd-freeze` standard; the scope distinction is the part a reader will otherwise get wrong |
| `specs/reference/api/DOC_08_RECIPES_PLANS.md` | reference | planner / architecture readers | A **"Where a boundary goes"** subsection ahead of "Pitfalls". **Rewrite the closing paragraph of "Why the default should make the predecessor available"**, which defers the decision to `CORE-PLAN-POLICY-AND-DEFAULTS` and is superseded: one cut retains one intermediate, and the memory counterweight is a retention policy (`CORE-ASSET-GC`), not a plan shape. Two pitfall rows: *a boundary query frozen before the prologue*, and *a recipe-level flag is not in the query* with the measured 2 → 1. `prologue_steps` and `volatility_source` in the plan-fields table. A paragraph for `v` in "Building a plan" — builder-intercepted like `q` and `ns`, no step so an identity on the value, and **whole-plan** volatility regardless of position |
| `specs/README.md` | map | orientation | the design folder |

Authoritative `affects_docs`: `[specs/reference/api/DOC_08_RECIPES_PLANS.md]`. Candidates
generated by area `core/plan` and rejected: `DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` (the
evaluation entry points are unchanged) and `PROJECT_OVERVIEW.md` (no core concept changes).
`## History` row and `reviewed:` bump on `DOC_08` in the same commit, per §9.2.

## Relevant Commands

**No new commands, and no existing `liquers-lib` namespace is involved** — `pl`, `lui`, `egui`
and `img` are all untouched. This is plan-building and asset machinery below the command layer.

Commands appear only as **test fixtures**: `fetch`, `expensive`, `prefix`, `tail`, `render`
(plain), `personalize` (`payload: required`), `vol_prefix` (`volatile: true`), plus the existing
`word`, `seed`, `upper`, `boom`, `identity` and the `recipe_cwd_resolution` set.
`injection.rs`'s `first_cmd` and `third_cmd` gain `payload: required` — a declaration fix, not a
new command.

The one command-facing decision worth confirming: **`injected` is left alone.** It means
`InjectedFromContext`, which may be satisfied from the environment, so it is not evidence of a
payload read in either direction; the payload need continues to be declared explicitly.

## Error Handling

No new error variants and few new error paths, because the design's decisions are *declines*
rather than failures.

| Situation | Behaviour |
|---|---|
| Plan not frozen when cut | Existing `Error::general_error` with the query attached — unchanged |
| Candidate needs a payload, or is volatile | `Ok(false)` at that level after stepping back; an `init_info` names the command and reason |
| The plan's volatility is `Declared` (`v`, `recipe.volatile`, volatile `recipe.expires`) | `Ok(false)` before the walk, with an `init_info` naming it |
| Candidate step count disagrees with the recorded range | `Ok(false)` — decline rather than mis-split |
| `check_consistent` fails | `Error::general_error` with the query attached, propagated with `?` |
| `PlanBuilder::build` fails on a candidate | Propagated with `?`. A recorded predecessor that will not build is an inconsistency, not a policy outcome — and placeholders are allowed, so it cannot fail merely for a missing argument the parent tolerated |

`Error::general_error` and the existing typed constructors throughout; no `Error::new`, no
`unwrap`/`expect` outside tests, no `println!`.

## Review Record

The host does not permit spawning agents, so the two Phase 2 review passes ran sequentially
against the same briefs, per this skill's host-compatibility clause. `plan-cwd-freeze` recorded
the same limitation.

**rust-best-practices pass.** Four findings, all applied:

1. *Blocking-adjacent.* `assert_consistent()` as a panicking helper puts a panic on a library
   path. Changed to `check_consistent() -> Result<(), Error>`, propagated with `?`; a
   `debug_assert!` wrapper remains available where a caller has no `Result`.
2. *Advisory, applied.* A reason field typed `Option<String>` allocates on every volatile-recipe
   plan and leaves the set open. Changed to a `Copy` enum, exhaustively matchable under the
   no-`_ =>` rule — and the `v` review then generalised it into `Option<VolatilitySource>`.
3. *Advisory, applied.* `#[serde(skip_serializing_if = "Option::is_none")]` on the new option, to
   match the convention already used on `payload_required` and the `Recipe` string fields.
4. *Advisory, applied.* `freeze_cwd_with(&mut base.clone())` relies on temporary lifetime
   extension; bind the clone to a named local instead, so the intent (a fresh cursor per
   candidate) is legible.

Confirmed clean: borrowed `&CommandMetadataRegistry` rather than `Arc` (no lifetime on `Plan`);
sync justified rather than assumed; no trait signature changed, so `liquers-py` implementors are
unaffected; dependency flow one-way; no default match arm introduced.

**Reviewer A — Phase 1 conformity.** Scope holds: every change is in the crate and the
interaction set Phase 1 named, and the default flip is present as Phase 1's confirmed intent
rather than deferred. Phase 1's two confirmation items were discharged by measurement in this
phase, not carried forward.

**Reviewer B — Codebase alignment.** Signatures checked against HEAD:
`get_command_metadata_registry(&self) -> &CommandMetadataRegistry` on `Environment` and `EnvRef`;
`PlanBuilder::new(Query, &CommandMetadataRegistry)` and `with_placeholders_allowed`;
`CwdCursor::{new, set_cwd_from, resolve_query_scoped}`; `Expires::is_volatile`.

**Reusable functionality found, and used rather than duplicated:** `freeze_cwd_with` for candidate
freezing; the builder's own predecessor recording for the next candidate; and
`resolve_volatility_before_evaluation`'s existing `recipe.volatile || recipe.expires.is_volatile()`
predicate rather than a second, subtly different one.

**Raised as a question, then taken.** Whether to fold the recipe's volatility into
`plan.is_volatile` was surfaced rather than decided; the author took it into scope, so
`RECIPE-TO-PLAN-IGNORES-RECIPE-LEVEL-VOLATILE-AND-EXPIRES` is fixed here and its blast radius was
measured — 19 suites green — rather than argued.

**Revision after the `v` review.** Checking the `v` instruction against the walk replaced the
`uncuttable` marker with `Plan::volatility_source`. `v` is not an obstacle to be worked around; it
exposed that `PlanBuilder` collapses two different kinds of volatility into one flag. Drawing the
distinction covers `v`, `recipe.volatile` and a volatile `recipe.expires` with one field, and
makes the three measured obstacles unreachable rather than guarded against. The earlier
recipe-only marker would have had to route around `v` separately.
