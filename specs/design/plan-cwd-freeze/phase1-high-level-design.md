# Phase 1: High-Level Design - Plan CWD Freeze

## Feature Name

Freeze CWD in the plan, then cut correct predecessor evaluation boundaries

## Purpose

CWD-relative operands are resolved independently in three places today — dependency analysis,
dependency pre-scheduling and runtime step execution — each with its own `CwdCursor` that must
agree with the others. `Plan::freeze_cwd` resolves every static operand to absolute form once, so
a finalized plan is CWD-independent by construction. That is worth doing on its own, and it is also
the precondition for `disable_expand_predecessors()`, which is unusable today
(`CORE-RECIPES-EXPAND-PREDECESSORS-CRASH`, P0) and blocks the default-policy decision in
`CORE-PLAN-POLICY-AND-DEFAULTS`.

## Why now: what a boundary costs today

`disable_expand_predecessors()` cuts a query's predecessor into a separate `Step::Evaluate`
dependency, so intermediates become individually cached, addressable, schedulable assets. Enabling
it at HEAD gives 11 failures in `cargo test -p liquers-core --lib`, from four causes:

| # | Cause | Disposition |
|---|---|---|
| R1 | The named test's `word` command omits `payload: required`, so the payload does not reach it across the boundary — the documented "declare it, or lose it" rule. Nothing panics. | Test defect; one line |
| R2 | `Query::predecessor()` splits a trailing **filename** off as the remainder, so `Step::Evaluate` swallows the real last action. Recipe value/link overrides then have no action to patch and fail hard — for every recipe whose query ends in a filename, which is every recipe addressable in a directory. | Fix the cut point |
| R3 | `Step::Evaluate` is a CWD scope boundary by design, so a predecessor containing `-R-cwd/…` sets a CWD that never reaches the enclosing plan. `absolute_query_resource_step_index()` also maps query segments onto steps positionally and returns `None` once a step is opaque. | **Dissolved by freeze** |
| R4 | `PlanBuilder` never inspects the predecessor it cut away, so build-time `is_volatile`, `payload_required` and `expires` lose everything behind the boundary. `Recipe::is_volatile` and `to_plan_for_key` read exactly those fields, so a volatile keyed recipe stops recomputing and the keyed-payload rejection stops firing. | Harvest the sub-plan |

R3 is why this design leads with freeze rather than with the cut.

## Core Interactions

### Query System
No syntax change. `CwdCursor::resolve_query_scoped` already produces the canonical absolute form
and is reused verbatim; freeze applies it to plan steps rather than re-deriving per pass. Verified:
relative operands become per-folder keys, absolute ones are returned unchanged, so a shared input
referenced from many folders keeps **one** cache entry.

### Command System
CWD reaches a command as **data**, not ambient context: a default link argument `-R-key/.` resolves
to the current directory as a key value, is overridable, and is visible to the plan builder.
`Context::get_cwd_key` and `set_cwd_key` become `pub(crate)` — verified to have zero users outside
`liquers-core`. `Context::evaluate`/`apply` keep accepting relative queries: after freeze the
interpreter installs each step's frozen CWD, so dynamic evaluation resolves against a
statically-known value. `payload: required` is added to the `word` test command (R1).

### Asset System
The payoff: a frozen plan yields a CWD-free `Step::Evaluate` query, so a predecessor becomes a
first-class asset with a correct cache key — cached, independently expiring, schedulable.

### Store System / Value Types / Web / UI
Not applicable.

## Crate Placement

`liquers-core` only — `src/plan.rs` (`freeze_cwd`, cut rule, sub-plan harvest), `src/query.rs`
(reuse the cursor), `src/interpreter.rs` (call freeze in `finalize_plan`; test declarations),
`src/context.rs` (accessor visibility), `src/recipes.rs` (drop the commented call).

## Documentation Intent

**Reference:** Extend two. `specs/reference/api/DOC_08_RECIPES_PLANS.md` — its planning-contract
table describes `disable_expand_predecessors` as merely "Emits `Step::Evaluate` boundaries", which
is what misled this work; it must state when a boundary may be cut and what crosses it.
`specs/reference/PROJECT_OVERVIEW.md` — freeze changes when a plan stops being CWD-relative, which
is a core-concept change. No new reference: freeze belongs beside the existing plan contract.

**Guide:** Neither. Nothing here is a repeatable developer task. Reconsider if the boundary default
is flipped, since that would change how every recipe author reasons about caching.

**Other documents to create:** None expected. If Phase 2 puts the relative-`evaluate` question
(open question 1) out of scope, it becomes one `specs/issues/` file.

**Specific documents to update:** `specs/design/plan-relative-resolution/` — its "Future Plan
Normalization and Optimization" section anticipated exactly this pass and is superseded in part;
`specs/issues/CORE-RECIPES-EXPAND-PREDECESSORS-CRASH.md` and
`specs/issues/CORE-PLAN-POLICY-AND-DEFAULTS.md` (status and unblocking);
`specs/reference/PAYLOAD_GUIDE.md` (a boundary makes the declaration trap reachable from plan
policy, not only from hand-written sub-queries); `specs/README.md`; `specs/index.csv`.

Audience is internal. A future reader should learn, without opening this folder, at which point a
plan stops being CWD-relative and why cutting a predecessor is not free.

## Scope

1. **Freeze** — `Plan::freeze_cwd(entry: &Key)`, called from `finalize_plan` *before* dependency
   analysis so `find_dependencies` and `schedule_plan_dependencies_from` need no cursor. Explicit,
   never folded into `build()`. Recurses into `Step::Plan` and link parameters.
2. **Boundary** — strip the filename before consulting `predecessor()`; materialize `DefaultLink`
   into explicit relative links in the emitted query; harvest the sub-plan's volatility, payload and
   expiration; leave `Step::SetCwd` behind in the parent when cutting.
3. **Policy** — the option stays off by default. Flipping it belongs to
   `CORE-PLAN-POLICY-AND-DEFAULTS` and needs its own evidence.

## Open Questions

1. At a cut, does the boundary query carry the frozen CWD? Freeze makes the CWD deterministic at
   every step, and the interpreter installs it, so `Context::evaluate` keeps accepting relative
   queries and resolves them against the frozen value — execution is correct either way. What
   freeze does *not* do is remove the CWD from the *value*: for `load_sibling-data`, whose text has
   no relative operand, the boundary query is byte-identical in every folder while the value is not.
   Either prefix it with `-R-cwd/<frozen>` (always correct, one entry per folder — the waste this
   design otherwise avoids) or hand over the bare query (one shared entry, correct only if the
   sub-plan never consumes the CWD). Leading option: prefix only when consumption is **statically
   visible** — freeze already knows whether a static operand resolved relatively and whether an
   action carries a `-R-key/.` link argument, both precise and needing no new metadata — and cover
   the runtime residue with a **poison flag** at `CwdCursor::resolve_key`'s existing `is_relative`
   branch, marking the asset non-shareable so it skips the `query_assets` map exactly as a volatile
   asset does. Caveat to settle in Phase 2: the poison is late, so it prevents wrong *reuse* but not
   a concurrent join to the first in-flight evaluation. A hard guarantee for the dynamic residue
   needs a declaration or an API restriction instead.
2. A **default** link is invisible to the cache key — verified: with default `-R-key/.`,
   `plan.query.encode()` stays `list_stuff` under every CWD, while an explicit link normalizes to
   `list_stuff-~X~-R-key/proj/a~E`. Materialize all default links at a cut, or only relative ones?
   Materializing all is simpler and does not split entries, since absolute defaults render
   identically everywhere.
3. Does `freeze_cwd` **remove** now-inert `Step::SetCwd` steps, or keep them? Keeping them is
   required while relative `evaluate` is allowed (question 1) and is useful for provenance. The
   prior design's ordering-barrier argument applies only to removal.
4. Migration: land freeze while **leaving** the runtime cursors in place and assert they become
   no-ops (`resolve_key` already early-returns on absolute keys), or remove them in the same
   change? The former turns any residual disagreement into a test failure instead of a silent
   behaviour change.
5. Should `schedule_plan_dependencies_from` pre-schedule non-keyed `Step::Evaluate` queries? Today
   it schedules only keyed ones, so the parallelism half of the motivation is unrealized. Fix here
   or file separately?

## References

- `specs/design/plan-relative-resolution/phase2-architecture.md` §"Future Plan Normalization and
  Optimization" — anticipates this pass and names what blocks *removing* `SetCwd`
- `specs/issues/CORE-RECIPES-EXPAND-PREDECESSORS-CRASH.md` (P0), `CORE-PLAN-POLICY-AND-DEFAULTS.md` (P2)
- `specs/reference/api/DOC_08_RECIPES_PLANS.md` §"Planning contract"; `specs/reference/PAYLOAD_GUIDE.md`
- `liquers-core/src/plan.rs:1559-1604` (cut site), `:1191-1210` (existing sub-plan harvest pattern),
  `:1701` (positional resource-step derivation that freeze makes unnecessary)
- `liquers-core/src/interpreter.rs:41` (entry CWD snapshot), `src/context.rs:423` (relative
  resolution inside `schedule_dependency_asset`)
