# Phase 2: Solution & Architecture - Predecessor Cut Equivalence

## Overview

Cutting at the outermost cacheable predecessor becomes the default. The architecture that takes
it there is small: one field on `Plan`, one marker, one changed signature, one structural change
to how a `Plan` is copied, and a placement rule computed per candidate rather than per plan.

No new type, no new trait, no new command, no new error variant. Everything is inside
`liquers-core`, and the crate dependency flow is untouched.

Five causes drive it. Four were measured at `d1bd02e` by forcing the cut on; the fifth was found
by reading and then measured.

| # | Cause | Divergences | Verdict |
|---|---|---|---|
| 1 | The predecessor query is frozen against the *entry* CWD, one step before the recipe's `SetCwd` prologue | 2 (`recipe_cwd_resolution`) | **Defect.** §"Recording the prologue" — fix verified |
| 2 | A cut boundary is a cache entry, and a payload is not part of a cache key | 1 (`injection`) | Mis-declared command, fixed in the test; exposes the placement rule |
| 3 | A test asserts the *expanded* plan's step shape | 1 (`--lib`) | Not a defect; measured equivalent in value |
| 4 | A recipe-level `volatile:` is in no query, so it does not reach a boundary | 0 measured | **Defect.** §"Marking a plan uncuttable" |
| 5 | `Plan::split` copies a field list and drops the coupled predecessor fields | 0, latent | In scope — the field list is the shape of every defect here |

## Known-Issue Preflight

### Open issues bearing on this design

| Issue | Status / Pri | Bearing | Blocking? |
|---|---|---|---|
| `PREDECESSOR-CUT-NOT-YET-EQUIVALENT` | draft P1 | The issue this design closes. | — |
| `CORE-PLAN-POLICY-AND-DEFAULTS` | accepted P2 | Its `expand_predecessors` default half is **answered** by this design; the `cache`, `volatile flags` and `inline flag` markers are untouched. | No |
| `PLAN-SPLIT-DROPS-PREDECESSOR-FIELDS` | accepted P2 | In scope, §"Coupled fields". Raised P3 → P2 during Phase 1. | No |
| `RECIPE-TO-PLAN-IGNORES-RECIPE-LEVEL-VOLATILE-AND-EXPIRES` | draft P2 | Filed *from* this preflight. It is why a plan cannot simply be asked whether it is volatile, and so why §"Marking a plan uncuttable" needs a marker. Fixing it would subsume that marker — see the decision recorded there. | No |
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

    /// Why this plan may not be cut, when a declaration outside its query forbids it.
    ///
    /// A recipe-level `volatile:` is in no query, so no candidate boundary's own plan can
    /// reveal it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uncuttable: Option<UncuttableReason>,
}

/// Closed set, so a new reason is a compile error at every match rather than a new string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UncuttableReason {
    /// The recipe declares `volatile: true`.
    RecipeVolatile,
    /// The recipe declares an expiration that is itself volatile (`Expires::Immediately`).
    RecipeExpiresImmediately,
}
```

`UncuttableReason` rather than `Option<String>`: the reason is data that a message is rendered
*from*, not the message itself. An enum keeps the set explicit, matches exhaustively under the
project's no-`_ =>` rule, is `Copy`, and allocates nothing on the common path. `Display` supplies
the `init_info` text.

`#[serde(default)]` on both, matching the three fields `plan-cwd-freeze` added — a plan
serialized before this change loads at the pre-change values.

## Trait Implementations

`UncuttableReason` derives `Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize` and
implements `Display` by hand (the rendered sentence is user-facing text, not a derive's business).

`Plan` gains nothing: `usize` and `Option<UncuttableReason>` are covered by every derive already
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

// recipes.rs — unchanged signature; now also records `prologue_steps` and `uncuttable`
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
| `liquers-core/src/plan.rs` | `prologue_steps`, `uncuttable`, `UncuttableReason`; the prologue walk in `freeze_cwd_with`; the candidate walk in `cut_predecessor`; `split` rebuilt from `self.clone()`; `check_consistent` |
| `liquers-core/src/recipes.rs` | `to_plan` records `prologue_steps` and `uncuttable` beside the existing `predecessor_steps` bump |
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
nothing to patch — and any candidate in a plan marked `uncuttable`.

Every level passed over, and the decline, appends a planning `Plan::init_info` naming the command
and the reason:

```
Predecessor boundary expanded at 'personalize': command requires an evaluation payload
Predecessor boundary expanded at 'vol_prefix': command is volatile
Predecessor boundary not cut: recipe declares volatile: true
```

`init_info` rather than `Step::Info`: this is established once at planning time, and `init_steps`
are copied into metadata rather than re-logged on every execution. Without it a declined cut is
indistinguishable from a plan that had no predecessor.

**The freeze wrinkle.** `freeze_cwd` resolves `plan.predecessor`, so the longest candidate arrives
frozen. A shorter candidate recovered by rebuilding is built fresh from source and is **not**:
its operands are still CWD-relative, and cutting on it reproduces cause 1 one level down. Each
such candidate is frozen against a clone of the prologue-advanced cursor before its own
predecessor is read — reusing `freeze_cwd_with` rather than writing a second cursor walk.

### Marking a plan uncuttable

A recipe-level `volatile:` is in no query, so the walk cannot see it. `Recipe::to_plan` has the
recipe in hand and records `uncuttable`; `cut_predecessor` reports it and returns `Ok(false)`
before the walk starts.

Per Phase 1, `expires:` does **not** block a cut — it bounds how long the resulting asset stays
valid, not the purity of the computation. The predicate is
`recipe.volatile || recipe.expires.is_volatile()`, the same one
`resolve_volatility_before_evaluation` already ORs at `assets.rs:1610`; `Expires::is_volatile` is
true only for `Immediately` (or a `Combination` containing one), so a plain finite expiration is
unaffected.

**Decision to confirm.** The cleaner architecture is to have no marker at all: fold the recipe's
volatility into `plan.is_volatile` at `to_plan`, and the walk's existing volatility predicate
covers it. Measured, `to_plan` ignores both `Recipe::volatile` and `Recipe::expires` today —
`volatile: true` yields `plan.is_volatile == false`, `expires: Immediately` yields
`plan.expires == Never` — which is a defect in its own right and is filed as
`RECIPE-TO-PLAN-IGNORES-RECIPE-LEVEL-VOLATILE-AND-EXPIRES` (P2). Folding would fix that *and*
remove the field.

It is **not** taken here, because `finalize_plan` skips dependency registration for a volatile
plan (`if !plan.is_volatile { … }`), so folding stops a volatile recipe registering plan
dependencies. That is probably the correct and consistent behaviour — a volatile recipe becoming
a volatile plan — but it is blast radius outside this design's subject, and it deserves its own
verification rather than riding along. The marker is the contained choice; the issue records the
better one.

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
| `predecessor`, `predecessor_steps`, `uncuttable` | cleared | cleared |
| `prologue_steps` | carried, clamped | `0` |

`check_consistent` covers `prologue_steps <= steps.len()`, and
`prologue_steps <= predecessor_steps <= steps.len()` when `predecessor.is_some()`, called after
`build`, after `to_plan`'s insert, after `split` and after `cut_predecessor`.

## Documentation Architecture

Phase 1 decided extend-not-new for the reference, no guide, no other documents.

| Path | Kind | Audience | Change |
|---|---|---|---|
| `liquers-core/src/recipes.rs` | rustdoc | recipe authors, contributors | `Recipe::volatile` — replace the one-line comment with the meaning (volatile **from the first action**), the consequence (nothing cached, no boundary cut), why the last-action reading fails, and that the positional instrument is `v`. `Recipe::expires` — that it bounds the result's validity and does *not* block a cut. `Recipe::to_plan` — the two facts it now records and why neither is recoverable later |
| `liquers-core/src/plan.rs` | rustdoc | contributors | `PlanBuilder` type doc — what it records for later passes and does not act on. `Plan::cut_predecessor` — the placement rule and the measured 2 → 1, since this is where a reader lands from a stack trace or an `init_info` line. `prologue_steps`, `uncuttable`, `UncuttableReason` field and variant docs at the `plan-cwd-freeze` standard |
| `specs/reference/api/DOC_08_RECIPES_PLANS.md` | reference | planner / architecture readers | A **"Where a boundary goes"** subsection ahead of "Pitfalls". **Rewrite the closing paragraph of "Why the default should make the predecessor available"**, which defers the decision to `CORE-PLAN-POLICY-AND-DEFAULTS` and is superseded: one cut retains one intermediate, and the memory counterweight is a retention policy (`CORE-ASSET-GC`), not a plan shape. Two pitfall rows: *a boundary query frozen before the prologue*, and *a recipe-level flag is not in the query* with the measured 2 → 1. `prologue_steps` and `uncuttable` in the plan-fields table. A paragraph for `v` in "Building a plan" — builder-intercepted like `q` and `ns`, no step so an identity on the value, and **whole-plan** volatility regardless of position |
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
| Recipe declares volatility | `Ok(false)` before the walk, with the recorded reason |
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
2. *Advisory, applied.* `uncuttable: Option<String>` allocates on every volatile-recipe plan and
   leaves the reason set open. Changed to `Option<UncuttableReason>` — `Copy`, exhaustively
   matchable under the no-`_ =>` rule, with `Display` supplying the message.
3. *Advisory, applied.* `#[serde(skip_serializing_if = "Option::is_none")]` on `uncuttable`, to
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

**One finding raised as a question rather than fixed:** whether to fold the recipe's volatility
into `plan.is_volatile` and drop the `uncuttable` field entirely. Recorded in §"Marking a plan
uncuttable" and filed as `RECIPE-TO-PLAN-IGNORES-RECIPE-LEVEL-VOLATILE-AND-EXPIRES`.
