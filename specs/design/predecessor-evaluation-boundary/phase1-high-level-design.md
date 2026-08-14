# Phase 1: High-Level Design - Predecessor Evaluation Boundary

## Feature Name

Correct `Step::Evaluate` predecessor boundaries in `PlanBuilder`

## Purpose

`PlanBuilder::disable_expand_predecessors()` is supposed to cut a query's predecessor into a
separate `Step::Evaluate` dependency, so intermediate results become individually cached,
individually addressable assets that can be scheduled in parallel. It is unusable today: it is not
semantics-preserving, so `recipes.rs:217` keeps the call commented out and the option has no call
site anywhere in the workspace. This design makes the boundary correct, so the option can be turned
on deliberately (default choice stays with `CORE-PLAN-POLICY-AND-DEFAULTS`).

## Findings at HEAD

The reported "crash" is not a crash. Enabling the option and running `cargo test -p liquers-core
--lib` gives 11 failures with four distinct root causes; the named test is the least serious one.

| # | Root cause | Failing tests |
|---|---|---|
| R1 | Payload does not reach a command across the boundary when the command omits `payload: required` — the documented "declare it, or lose it" rule. The test under-declares `word`. | `test_evaluate_immediately` |
| R2 | `Query::predecessor()` splits a trailing **filename** off as the remainder, so `Step::Evaluate` swallows the real last action. Recipe value/link overrides then have no action to patch and fail hard. | `recipe_plan_round_trip…`, `expiration_nested_recipe…`, `find_dependencies_respects_nested_recipe_cwd`, `serialization_keeps_raw_cwd…` |
| R3 | `Step::Evaluate` is deliberately a CWD scope boundary (`find_dependencies_child_query_cwd_does_not_leak`), so a predecessor containing `-R-cwd/…` sets a CWD that never returns to the enclosing plan. `absolute_query_resource_step_index()` also maps query resource segments onto steps positionally and returns `None` once a step is opaque. | `resolver_scopes_nested_links`, `recipe_prefix_info…`, `recipe_to_plan_preserves_programmatic_cwd`, `absolute_source_cwd_skips…`, `absolute_outer_resource_keeps…` |
| R4 | `PlanBuilder` never inspects the predecessor it cut away, so build-time `is_volatile`, `payload_required` and `expires` lose everything behind the boundary. `Recipe::is_volatile` and `to_plan_for_key` read exactly those build-time fields, so a volatile keyed recipe stops recomputing and the "keys are a payload boundary" rejection stops firing. | `volatile_keyed_recipe_recomputes_every_time` |

R2 and R4 are the damaging ones: R2 breaks the entire point of recipes (overrides on the last
action) for every recipe whose query ends in a filename — which is every recipe addressable in a
directory — and R4 silently weakens two documented invariants.

## Core Interactions

### Query System
No syntax change. `Query::predecessor()` keeps its current contract; `PlanBuilder` stops using it
as the sole cut rule and strips the filename before consulting it.

### Command System
No new or changed commands. R1 is corrected by declaring `payload: required` on the test command.

### Asset System
This is the point of the change: a correctly cut boundary makes each predecessor a first-class
asset, so intermediates are cached, expire independently, and can be scheduled as dependencies.

### Store System / Value Types / Web / UI
Not applicable.

## Crate Placement

`liquers-core` only — `src/plan.rs` (cut rule, sub-plan property harvest), `src/recipes.rs` (remove
the commented call), `src/interpreter.rs` (test declaration; optional dependency pre-scheduling).

## Documentation Intent

**Reference:** Extend `specs/reference/api/DOC_08_RECIPES_PLANS.md` — its "Planning contract" table
already lists `disable_expand_predecessors` as simply "Emits `Step::Evaluate` boundaries", which is
what misled this work. It must state when a boundary is emitted and what crosses it. No new
reference: the subject is one builder policy, not a new capability.

**Guide:** Neither. Nothing here is a repeatable developer task. Reconsider if the default is
flipped, since that would change how every recipe author reasons about caching.

**Other documents to create:** None, unless Phase 2 confirms the `injected`-implies-payload lint
below is out of scope — then one `specs/issues/` file.

**Specific documents to update:** `specs/issues/CORE-RECIPES-EXPAND-PREDECESSORS-CRASH.md`
(status), `specs/issues/CORE-PLAN-POLICY-AND-DEFAULTS.md` (record that the crash blocker is gone so
the default can be reconsidered), `specs/reference/PAYLOAD_GUIDE.md` (note that an evaluation
boundary makes the "declare it, or lose it" trap reachable from plan policy, not just from
hand-written sub-queries), `specs/README.md`, `specs/index.csv`.

Audience is internal. A future reader should learn, without opening this folder, why a predecessor
boundary is not free and which plan properties must be carried across it.

## Open Questions

1. Cut rule: refuse to cut when the predecessor sub-plan contains a `Step::SetCwd` (build it, then
   splice its steps instead), or classify segments syntactically before building? Building once and
   reusing it for the R4 property harvest looks cheaper and mirrors
   `check_parameter_for_volatile_links`.
2. R3's second half: keep the conservative "do not cut inside an absolute query's resource
   segments", or let `PlanBuilder` record the absolute-resource step index directly on `Plan`
   instead of re-deriving it positionally? The latter removes a fragile mechanism but widens the
   change.
3. Should `schedule_plan_dependencies_from` pre-schedule non-keyed `Step::Evaluate` queries? Today
   it only schedules keyed ones, so the parallelism half of the motivation is not realised. Fix
   here or file separately?
4. Is a lint for "injected parameter without `payload: required`" in scope? `InjectedFromContext`
   receives the whole `Context`, so inference is not sound in general — a warning may be the most
   that is defensible.
5. Does the option stay off by default in this change? Recommended yes; flipping it is
   `CORE-PLAN-POLICY-AND-DEFAULTS`'s decision and needs its own evidence.

## References

- `specs/issues/CORE-RECIPES-EXPAND-PREDECESSORS-CRASH.md` (P0, the issue this resolves)
- `specs/issues/CORE-PLAN-POLICY-AND-DEFAULTS.md` (P2, owns the default)
- `specs/reference/api/DOC_08_RECIPES_PLANS.md` §"Planning contract"
- `specs/reference/PAYLOAD_GUIDE.md` §"Declare it, or lose it"
- `liquers-core/src/plan.rs:1559-1604` (cut site), `:1191-1210` (existing sub-plan harvest pattern)
