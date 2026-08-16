# Phase 2: Solution & Architecture - Plan CWD Freeze

## Overview

`Plan::freeze_cwd(entry)` walks a plan's steps in execution order with one `CwdCursor` and rewrites
every CWD-relative operand — keys, nested queries, link parameters and nested plans — into absolute
form. It is called from `finalize_plan` before dependency analysis, so the two existing analysis
cursors observe an already-absolute plan and become identities. `PlanBuilder` is unchanged in
traversal: it always expands, and records the predecessor sub-query it descended into so a separate
post-freeze pass can replace those steps with a single `Step::Evaluate` boundary. `Context`'s CWD
accessors become crate-private and `evaluate`/`apply` reject relative queries, which makes "the
frozen query determines the value" an enforced invariant rather than a convention.

## Known-Issue Preflight

Searched `specs/index.csv` for open (`draft`/`accepted`/`in_progress`) issues whose `area` includes
`core/plan`, `core/query`, `core/context` or `core/assets`, plus the two issues already linked from
`DESIGN.md`, plus `specs/design/plan-relative-resolution/` (the design this one extends).

| Issue | Status | Priority | Relevance and solution impact | First? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| `CORE-RECIPES-EXPAND-PREDECESSORS-CRASH` | accepted | P0 | The issue this design resolves. Its four root causes are addressed or made unreachable. | no | no | Close in Phase 5 | Keep P0 |
| `PARAMETER-ESCAPING-INCOMPLETE` | **closed** | P0 | Resolved on `main` by `parameter-entity-escaping` (PR #34). **Re-measured after rebase:** all eleven probes now round-trip with `ast_eq=true txt_stable=true`, including the three that failed before — `f-~Hapi.example.com~/data`, `f-~hexample.com~/x` and `f-~P` now encode back to themselves rather than expanding the mnemonic into an unescapable `:`. `parse(encode(q)) == q` holds and encoding is idempotent. | no | no | None. The AST-construction constraint below is kept as good practice, not as a defect workaround. | n/a |
| `QUERY-AST-DISCARDS-ENTITIES` | draft | P3 | The AST keeps the *decoded* value, so `encode(parse(t))` normalises spelling (measured: `~I` encodes as `~/`). Mildly **favourable** here: `query_assets` is keyed by AST and `DependencyKey` by `encode()`, so both mechanisms are canonical over the same decoded semantics and cannot disagree about whether two spellings name one asset. | no | no | Monitor; note the canonical-identity property in the reference update | Keep P3 |
| `RESOURCE-NAME-ASCII-ONLY` | draft | P2 | Resource names are ASCII-only with no entity production. Freeze never parses new resource text — `Key::to_absolute` concatenates existing `ResourceName`s — so a key that parsed once stays representable and no new exposure is added. | no | no | Monitor | Keep P2 |
| `CORE-PLAN-POLICY-AND-DEFAULTS` | accepted | P2 | Owns the `expand_predecessors` default. This design makes the option viable and moves it from `PlanBuilder` to a plan transformation, so the issue's framing changes. | no | no | Update its text in Phase 5; the default flip stays its decision | Keep P2 |
| `CORE-EVALUATE-PATH-CONSOLIDATION` | accepted | P1 | Several evaluation paths exist, and freeze must apply on all of them. Verified: `apply_recipe` has 6 implementations in `liquers-core` plus one in `liquers-lib/src/environment.rs:120` (calls `finalize_plan`) and one `todo!()` stub in `liquers-py/src/context.rs:115`. Putting freeze **inside** `finalize_plan` inherits the existing "must be called in every `apply_recipe`" contract instead of adding a second one. | no | no | Freeze inside `finalize_plan`; add no new mandatory call | Keep P1 |
| `QUERY-BUILDER-TOOLING` | accepted | P2 | This design constructs queries (promoted default links, boundary query). Its guidance — build programmatically and encode, do not concatenate — is adopted as a constraint. | no | no | Follow the AST-construction constraint | Keep P2 |
| `CONTEXT-APPLY-BARE-KEY-ILL-DEFINED` | rejected | — | Establishes that `Context::apply` must not reject a query because the input state has no effect. Not in conflict: this design rejects on **operand form** (a relative key), not on state consumption. | no | no | Cite the distinction in the reference update | n/a |
| `VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS` | draft | P2 | Touches `ResolvedParameterValues::from_action`, which freeze also traverses. Independent: freeze rewrites link queries inside parameters and does not change arity resolution. | no | no | Monitor | Keep P2 |
| `CORE-MULTI-REALM-INTERPRETER` | accepted | P3 | Freeze matches on `Step` exhaustively, so a future realm-carrying step forces a compile error here — which is the intended effect of the no-default-arm rule. | no | no | Monitor | Keep P3 |

**No blocker.** The one P0 in the table closed on `main` while this design was in Phase 2, and the
re-measurement above confirms the concern it raised no longer applies. Nothing else is blocking.

## Data Structures

### Changed: `Plan` (`liquers-core/src/plan.rs`)

```rust
pub struct Plan {
    // ... existing fields unchanged ...

    /// CWD every operand in this plan was resolved against, or `None` while still source-relative.
    ///
    /// Set exactly once by [`Plan::freeze_cwd`]. A plan is never re-frozen under a different CWD;
    /// callers rebuild from the source `Query` or `Recipe`, per `finalize_plan`'s existing contract.
    #[serde(default)]
    pub frozen_cwd: Option<Key>,

    /// The predecessor sub-query `PlanBuilder` descended into, with relative default links promoted
    /// to explicit query links. `None` when the query has no predecessor.
    #[serde(default)]
    pub predecessor: Option<Query>,

    /// Number of leading `steps` emitted for [`Self::predecessor`].
    #[serde(default)]
    pub predecessor_steps: usize,
}
```

**Ownership rationale:** all three are owned plain data; `Plan` is already `Clone` and serialized as
a whole, and none is shared.

**Serialization:** every new field carries `#[serde(default)]`, so plans serialized before this
change still deserialize. `frozen_cwd: None` on such a plan reads correctly as "not frozen".

**Why `Option<Key>` rather than `bool`:** the entry CWD is needed to explain a resolved operand in a
diagnostic, and it lets `freeze_cwd` be idempotent by comparison rather than by a flag that could
disagree with the steps.

### Changed: `CwdCursor` (`liquers-core/src/query.rs`)

```rust
pub(crate) struct CwdCursor {
    // ... existing fields ...

    /// Set when `resolve_key` took its relative branch. Read by the migration assertion only.
    consumed_cwd: bool,
}

impl CwdCursor {
    /// Whether any resolution performed by this cursor actually consumed the CWD.
    pub(crate) fn take_consumed_cwd(&mut self) -> bool;
}
```

**Rationale:** one flag set in the existing `is_relative` branch of `resolve_key`
(`query.rs:2193-2203`). It exists so the migration step (Q5) can assert that the runtime cursors do
no work on a frozen plan, and is not load-bearing for correctness.

### New Enums

None.

### ExtValue Extensions

None. `-R-key/.` yields `Value::Key`, which already exists (`Value::Key(Key)`, with
`try_into_key`).

## Trait Implementations

No new traits and no new trait implementations. The design changes inherent methods on `Plan`,
`CwdCursor`, `ResolvedParameterValues` and `Context`, and one exhaustive `match` on `Step`.

`IsVolatile`, `RequiresPayload` and `AsyncRecipeProvider` are untouched: freeze runs before
`has_volatile_dependencies`, so those traversals see absolute operands and need no change.

## Generic Parameters & Bounds

No new generic parameters. `freeze_cwd` is inherent on `Plan`, which is not generic; it does not
touch `E: Environment`. This is deliberate — freeze is pure syntax over the plan AST and must not
acquire an environment bound, which is what keeps it synchronous (below).

## Sync vs Async Decisions

| Function | Async? | Rationale |
|---|---|---|
| `Plan::freeze_cwd` | **No** | Pure AST rewriting. No store, no registry, no asset manager. Keeping it sync is what allows it to be called from `Recipe`-side and validation contexts later without an environment. |
| `ResolvedParameterValues::freeze_cwd` | No | Same. |
| `Plan::cut_predecessor` | No | Operates on already-frozen steps and the recorded predecessor query. |
| `finalize_plan` | Yes (unchanged) | Already async; calls the sync freeze first, then the existing async dependency passes. |
| `Context::evaluate` / `apply` | Yes (unchanged) | Only a synchronous guard is added at the head. |

## Error Handling

Every error uses a typed constructor from `liquers_core::error`; `Error::new` is not used
(CLAUDE.md). Three new failure modes, and one deliberate non-failure:

| Condition | Constructor | `ErrorType` | Attached context | Why an error and not a silent fix |
|---|---|---|---|---|
| `freeze_cwd` called on a plan already frozen against a *different* key | `Error::general_error` | `General` | `.with_query(&plan.query)` | Means a caller reused a finalized plan under another CWD, which `finalize_plan` already forbids. Silently re-resolving would produce keys pointing at the wrong folder. |
| `Context::evaluate` / `get_dependency_state` / `apply` given a query with a relative resource key | `Error::not_supported` | `NotSupported` | `.with_query(query)` and `.with_position(&segment.position)` so the offending segment is highlighted | The whole point of the invariant: a relative operand here cannot be represented in the asset's identity. The message names `-R-key/.` as the supported replacement. |
| `cut_predecessor` called on an unfrozen plan | `Error::general_error` | `General` | `.with_query(&plan.query)` | A programming error in pass ordering; cutting an unfrozen plan silently produces a CWD-dependent boundary query, which is the defect this design exists to remove. |
| Relative operand with **no** entry CWD | *not* an error | — | — | Unchanged behaviour: logical root is installed and `RELATIVE_WITHOUT_CWD_WARNING` is logged exactly once, reusing `Context::install_logical_root_if_unset`. Freeze must not turn today's warning into a failure. |

`freeze_cwd` returns `Result<Key, Error>` rather than infallibly rewriting, purely so the first and
third rows are reportable. No new `ErrorType` variant is introduced — `CORE-ERROR-PAYLOAD-SIZE` and
the "no new error types outside `liquers_core::error`" rule both point the same way.

The rejection message is user-facing and must be actionable, in the style of the existing payload
message at `interpreter.rs:260`: state that a relative query cannot be evaluated from a command,
and that the directory is obtained as a `-R-key/.` link argument and combined into an absolute
query.

## Function Signatures

### `liquers-core/src/plan.rs`

```rust
impl Plan {
    /// Resolve every CWD-relative operand against `entry`, in execution order.
    ///
    /// Idempotent: freezing an already-frozen plan against the same key is a no-op, because
    /// `CwdCursor::resolve_key` returns a non-relative key unchanged. Returns the CWD in effect
    /// after the last step, which a caller may need when a nested plan continues the walk.
    ///
    /// Errors when the plan is already frozen against a different key, which would mean a caller
    /// reused a finalized plan under another CWD.
    pub fn freeze_cwd(&mut self, entry: &Key) -> Result<Key, Error>;

    /// Replace the leading `predecessor_steps` with a single `Step::Evaluate` boundary, keeping any
    /// `Step::SetCwd` among them in place.
    ///
    /// Requires a frozen plan; returns `Ok(false)` when there is no predecessor to cut.
    pub fn cut_predecessor(&mut self) -> Result<bool, Error>;
}

impl ResolvedParameterValues {
    /// Rewrite every link query in these parameters against a *clone* of `cursor`, so a link's own
    /// `-R-cwd` cannot leak into the enclosing plan.
    pub(crate) fn freeze_cwd(&mut self, cursor: &CwdCursor);
}
```

**Parameter choices:** `entry: &Key` is borrowed — freeze clones into the cursor and the caller keeps
its snapshot. `freeze_cwd` takes `&mut self` because it rewrites in place; producing a new `Plan`
would double peak memory for no benefit, and every caller owns the plan already.

**Why `Result` on an infallible-looking rewrite:** the double-freeze-under-a-different-key case is a
caller error worth reporting rather than silently producing wrong keys. Uses
`Error::general_error`, never `Error::new` (CLAUDE.md).

### Step traversal

`freeze_cwd` matches `Step` **exhaustively, with no `_ =>` arm**:

| Step | Action |
|---|---|
| `GetAsset`, `GetAssetBinary`, `GetAssetMetadata`, `GetAssetRecipe`, `GetAssetDirectory`, `GetResource`, `GetResourceMetadata`, `GetResourceDirectory`, `UseKeyValue` | `*key = cursor.resolve_key(key)` |
| `SetCwd` | `*key = cursor.set_cwd_from(key)` — advances the cursor and rewrites in place |
| `Evaluate`, `UseQueryValue` | `*query = cursor.resolve_query_scoped(query)` |
| `Action { parameters, .. }` | `parameters.freeze_cwd(&cursor)` — cloned cursor, link scope |
| `Plan(nested)` | `nested.freeze_cwd_with(cursor)` — **shared** cursor, matching `find_dependencies_nested_plan_propagates_cwd` |
| `Filename`, `Info`, `Warning`, `Error` | no-op |

The absolute-query case reuses `absolute_query_resource_step_index()` (`plan.rs:1701`) **once**, read
before any rewriting, to pick the step that resolves against logical root instead of `entry`. After
freeze that function has no further callers at runtime: `resolve_absolute_query_resource_step`
(`interpreter.rs:199`) becomes a no-op on a frozen plan and is skipped.

### `liquers-core/src/interpreter.rs`

```rust
pub async fn finalize_plan<E: Environment>(
    envref: EnvRef<E>,
    plan: &mut Plan,
    context: &Context<E>,
) -> Result<(), Error> {
    let initial_cwd = context.get_cwd_key().unwrap_or_else(Key::new); // existing snapshot, line 41
    plan.freeze_cwd(&initial_cwd)?;                                   // NEW, before analysis
    has_volatile_dependencies(envref.clone(), plan, None).await?;     // cursor no longer needed
    has_expirable_dependencies(envref.clone(), plan).await?;
    // ... unchanged ...
}
```

Freeze goes **inside** `finalize_plan` rather than beside it. That inherits the documented contract
"must be called between `recipe.to_plan()` and `apply_plan()` in every `apply_recipe`
implementation", which all eight implementations across three crates already honour — so no
out-of-core caller has to change.

**Root-fallback warning.** `finalize_plan` currently snapshots `Option<Key>`; freeze needs a
concrete entry. Substituting logical root here must emit the existing
`RELATIVE_WITHOUT_CWD_WARNING` exactly once, reusing `Context::install_logical_root_if_unset` so the
"delivered once by the Context that wins installation" behaviour is preserved rather than
duplicated.

### `liquers-core/src/context.rs`

```rust
impl<E: Environment> Context<E> {
    pub(crate) fn get_cwd_key(&self) -> Option<Key>;   // was pub
    pub(crate) fn set_cwd_key(&self, key: Option<Key>); // was pub
}
```

Relative-query rejection is applied at the two choke points that cover all three public entries:

```rust
// context.rs:423, covers Context::evaluate and Context::get_dependency_state
// context.rs:595, covers Context::apply
fn reject_relative_query(query: &Query) -> Result<(), Error>;
```

The test is **"contains a resource segment whose key is relative"** (`CwdCursor::is_relative`, `query.rs:2179`: first
component is `.` or `..`), recursively including link parameters — *not* `!query.absolute`. A query
such as `greet-Hello` has no key operand at all and stays valid.

## Integration Points

### Crate: liquers-core

| File | Change |
|---|---|
| `src/plan.rs` | `Plan::{frozen_cwd, predecessor, predecessor_steps}`; `freeze_cwd`, `cut_predecessor`; `PlanBuilder` records the predecessor instead of cutting; remove the `Step::Evaluate` branch at `:1571` and the `expand_predecessors` flag at `:1064`/`:1089` |
| `src/query.rs` | `CwdCursor::consumed_cwd` + `take_consumed_cwd` |
| `src/interpreter.rs` | `finalize_plan` calls `freeze_cwd`; `apply_plan` skips `resolve_absolute_query_resource_step` on a frozen plan; `word` test command gains `payload: required` |
| `src/context.rs` | accessor visibility; `reject_relative_query` at both choke points |
| `src/recipes.rs` | delete the commented `disable_expand_predecessors` call at `:217` |
| `src/assets.rs` | chain the dependency's `Error` into the parent's at `:4446` so a cut does not degrade diagnostics |

### Crate: liquers-lib

`src/environment.rs:120` — no change required; its `apply_recipe` already calls `finalize_plan`.

### Crate: liquers-py

`src/context.rs:115` — `apply_recipe` is `todo!()`; unaffected.

### Dependencies

None added.

## Documentation Architecture

### Reference Plan

| Path | Kind | Audience | Area | Change |
|---|---|---|---|---|
| `specs/reference/api/DOC_08_RECIPES_PLANS.md` | reference | internal | `core/plan` | Replace the "Planning contract" row for `disable_expand_predecessors` (the option no longer exists on `PlanBuilder`). Add a "Freezing" subsection: what `freeze_cwd` resolves, when it runs, that a frozen plan is never re-frozen, and that boundary cutting is a post-freeze transformation. |
| `specs/reference/PROJECT_OVERVIEW.md` | reference | both | `core/plan`, `core/query` | State the point at which a plan stops being CWD-relative, and that `Context::evaluate`/`apply` require absolute queries with `-R-key/.` as the supported way to obtain the directory. |
| `specs/reference/PAYLOAD_GUIDE.md` | reference | internal | `core/context` | Note that an evaluation boundary makes the "declare it, or lose it" trap reachable from plan policy, not only from hand-written sub-queries. |

Each gets a `## History` row and a bumped `reviewed:` in the same commit (§9.2).

### Guide Plan

**None.** Nothing here is a repeatable developer task. Reconsider if the boundary default is
flipped, since that changes how recipe authors reason about caching.

### Other Documents to Create

**None** planned. `-R-key/.` replacing relative `evaluate`/`apply` is a migration note inside
`PROJECT_OVERVIEW.md`, not its own document.

### New Reference or Guide Documents

None. See the Phase 1 rationale: freeze belongs beside the existing plan contract, not in a new
reference.

### Existing Documents to Review or Update

Authoritative `affects_docs`: the three references above, plus
`specs/design/plan-relative-resolution/` (its "Future Plan Normalization and Optimization" section
is realised in part by this design and must say so), `specs/issues/CORE-RECIPES-EXPAND-PREDECESSORS-CRASH.md`,
`specs/issues/CORE-PLAN-POLICY-AND-DEFAULTS.md`, `specs/README.md`, `specs/index.csv`.

Discarded candidates, with reason: `specs/reference/ASSET_LIFECYCLE.md` (asset states are unchanged);
`specs/reference/STORE_CONFIG_FSD.md` (no store surface); `specs/guides/COMMAND_REGISTRATION_GUIDE.md`
(no registration syntax change — `-R-key/.` uses the existing `= query "..."` default form).

### Design and Capability Links

`specs/README.md` gains the design folder on creation and, in Phase 5, points readers at
`DOC_08_RECIPES_PLANS.md` for freezing rather than at this folder.

### Evidence to Collect During Implementation

Whether the runtime cursors are provably no-ops after freeze (Q5's assertion); how many tests
depend on relative `evaluate`/`apply` and what their `-R-key/.` rewrites look like; whether inline
`Step::UseKeyValue` link resolution is measurably worth it (Q3); the actual step-count reduction
after a boundary cut.

## Relevant Commands

### New Commands

**None.** This design adds no command. `-R-key/.` uses the existing `= query "..."` default-value
form of `register_command!` and the existing `Step::UseKeyValue`/`Value::Key` path.

### Relevant Existing Namespaces

None specific. The change is core plumbing beneath every namespace, so `pl` (Polars), `lui`/`egui`
(UI) and the root namespace are affected only in that their commands stop being able to pass a
relative query to `Context::evaluate`/`apply`. **Question for the user:** no `liquers-lib` command
does this today (verified — the only callers are in `liquers-core` tests), so I have assumed no
namespace needs a migration. Confirm if you know of a downstream command that does.

## Cut/No-Cut Equivalence

The cut is a **policy** choice, not a correctness mechanism: given freeze, the relative-query
rejection, and correct `volatile` / `payload: required` declarations, no probed case changes the
produced value.

| Candidate difference | Outcome |
|---|---|
| Payload | `schedule_payload_dependency_asset` inherits the payload and evaluates the dependency inline when the command declares `payload: required`. An undeclared command differs — that is R1, a command defect, not a reason to keep expansion. |
| Volatile | A volatile query asset is never inserted into `query_assets` (`get_volatile_query_asset`), so it recomputes; expanded it also recomputes. Identical. |
| Side-effecting command | Cutting lets two parents with a common prefix share one evaluation. Differs only when a command has side effects and is not declared `volatile` — again a declaration defect. |
| Cycles | Cutting adds a dependency edge, so a self-referential prefix is caught by cycle detection where expansion would recurse. Cutting is no worse and arguably stricter. |

Two differences are real and must be handled rather than accepted:

- **Error attribution — in scope for this design.** `assets.rs:4446` constructs the parent's failure
  from scratch and never chains the dependency's error, so a cut replaces `Command 'word' failed: No
  payload in context for injected parameter payload at position 1` with `Dependency asset 1001 did
  not produce a value (status Error)`. Observed in the HEAD experiment. The dependency's `Error` must
  be chained into the parent's, otherwise the two forms are not equivalent in any sense a caller can
  observe.
- **Progress and log attribution.** A cut predecessor's progress and log entries land on the
  sub-asset. `test_evaluate_immediately` asserts `primary_progress().is_done()` on the parent, so
  Phase 3 must state what the parent is expected to show.

Legitimately different, and not a defect either way: **memory versus recomputation**. A cut retains
the intermediate in the asset manager, so a large intermediate used once is better left expanded,
while a slow prefix shared by many parents is much better cut. That is a per-query trade, which
argues against a single global default and suggests the policy could later move to the asset
manager — when asked for `a/b/c` it may choose to request `a/b` first. Out of scope here; recorded
so `CORE-PLAN-POLICY-AND-DEFAULTS` inherits it.

**Consequence for Phase 3:** the equivalence suite is the primary deliverable — every scenario built
both ways, asserting the same value, the same `is_volatile` / `payload_required` / `expires`, and
the same surfaced error.

## Decisions Taken on the Open Questions

These were open at the Phase 1 gate and are resolved here; each is a recommendation to confirm.

| Q | Decision | Rationale |
|---|---|---|
| 1 | Rejection is **unconditional**, tested on operand form (a relative resource key), not on `query.absolute`. | Catches mistakes at the call, not at the boundary. Does not conflict with `CONTEXT-APPLY-BARE-KEY-ILL-DEFINED`, which rejects a *state-consumption* objection. **Cost below.** |
| 2 | The builder **records** `predecessor: Option<Query>` with relative default links already promoted, plus `predecessor_steps`. | Removes any positional step↔segment matching, which is the mechanism that made `absolute_query_resource_step_index` fragile. Promotion is a pure syntax test at build time ("is this default link relative?"). |
| 3 | A `-R-key/.` link resolves **inline**. | The value is a key already present in the plan; a full dependency asset per CWD-consuming action buys nothing. Implemented as a fast path where a link's plan is a single `Step::UseKeyValue`. |
| 4 | `freeze_cwd` **consumes** `Step::SetCwd` and keeps the steps for provenance. Nothing downstream may depend on them. | With relative `evaluate` rejected, nothing reads the context CWD, so executing them is pure bookkeeping. Keeping them preserves provenance and the ordering barrier from `plan-relative-resolution`; dropping them is an optimiser's job. |
| 5 | Land freeze with the runtime cursors **in place**, asserting `take_consumed_cwd() == false` on frozen plans in tests; remove them in a follow-up. | Turns residual disagreement into a test failure rather than a silent behaviour change. |
| 6 | Non-keyed `Step::Evaluate` pre-scheduling is **filed separately**. | It is a throughput optimisation, not correctness, and it wants its own measurement. |

### Cost of decision 1, stated explicitly

Rejecting relative queries removes a capability that is currently tested on purpose. Verified
callers:

- `liquers-core/tests/recipe_cwd_resolution.rs` — three command helpers, `via_evaluate`
  (`context.evaluate("-R/./hello.txt")`), `via_state`
  (`context.get_dependency_state("-R/./hello.txt")`) and `via_apply`
  (`context.apply("-R-stored/./identity")`), exercised by `context_boundary_commands_use_active_cwd`.
- `liquers-core/src/context.rs:1601` — a unit test asserting `apply` with `-R-key/./from-apply`
  resolves against the context CWD.

`plan-relative-resolution` phase 2 explicitly blessed this capability ("commands may observe CWD and
issue relative `evaluate`/`apply` calls"). Those four tests must be rewritten, not deleted: each
becomes a command taking `cwd` as a `-R-key/.` link argument and building an absolute query from it,
which is the behaviour the new invariant requires. No `liquers-lib`, `liquers-axum` or `liquers-web`
command is affected.

## Review Record

Host does not permit spawning review agents (`Do not call the AgentTool unless the user requested
it`), so the two Phase 2 review passes were performed sequentially against the same briefs, per this
skill's host-compatibility clause.

**Reviewer A — Phase 1 conformity.** Scope holds: freeze primary, boundary as payoff, policy
deferred. One drift corrected — Phase 1 scoped `freeze_cwd` as recursing into `DefaultLink`; this
document additionally has the *builder* promote relative default links into `predecessor`, which
Phase 1 listed under scope item 2 but did not assign to a component. Assigned here.

**Reviewer B — codebase alignment.** Findings folded in: (i) freeze must live inside `finalize_plan`
because `apply_recipe` has 8 implementations across 3 crates and only that contract is already
universal; (ii) `find_dependencies_nested_plan_propagates_cwd` requires `Step::Plan` to share the
cursor, not clone it, while links must clone — the asymmetry is now explicit in the traversal table;
(iii) `finalize_plan` holds `Option<Key>` while freeze needs a concrete key, so the root-fallback
warning path must be reused rather than reimplemented; (iv) `DependencyKey` is `String`-based via
`query.encode()` (`metadata.rs:250`), which is why `PARAMETER-ESCAPING-INCOMPLETE` had to be
assessed rather than waved through — and, after the rebase onto `parameter-entity-escaping`,
re-measured rather than assumed still broken.

**Rebase, 2026-08-15.** Branch rebased onto `origin/main` (PR #34, `parameter-entity-escaping`).
Conflicts were confined to `specs/issues/PARAMETER-ESCAPING-INCOMPLETE.md` and `specs/index.csv`,
both resolved to `main`'s versions — the reproducer this design had added to that issue is obsolete
now that it is closed. Every other code reference in this document was re-verified against the new
tree; two shifted (`query.rs` `is_relative` 2180→2179, `resolve_key` 2194→2193) and the
`Step::Evaluate` branch is at `plan.rs:1571`, not `:1574`. `CORE-RECIPES-EXPAND-PREDECESSORS-CRASH`
is still `accepted`. Baseline `cargo test -p liquers-core --lib`: 548 passed, 0 failed.
