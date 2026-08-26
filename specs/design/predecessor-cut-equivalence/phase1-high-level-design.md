# Phase 1: High-Level Design - Predecessor Cut Equivalence

## Feature Name

`predecessor-cut-equivalence` — make `Plan::cut_predecessor` observably equivalent to expansion.

## Purpose

`plan-cwd-freeze` built the boundary machinery and left it switched off, because cutting a
predecessor into a `Step::Evaluate` still changes what queries produce. This effort closes the
remaining divergences and builds the suite that keeps them closed, so that cutting becomes a
choice about memory and scheduling rather than about whether the answer is right.
`CORE-PLAN-POLICY-AND-DEFAULTS` cannot decide the default until it is.

## Core Interactions

- **Plan / PlanBuilder** — freezing resolves the recorded predecessor; the builder records one
  predecessor per plan, which is what makes a per-candidate cut decision possible.
- **Recipe** — prepends a `SetCwd` the builder never emitted, and carries `volatile:` / `expires:`
  declarations that exist in no query.
- **Assets** — a boundary becomes a cache entry, which is why payload and volatility bear on
  where it may be placed.
- **Context** — `schedule_dependency_asset` chooses the payload path from the boundary query's
  own plan.

## Crate Placement

`liquers-core` only: `plan.rs`, `recipes.rs`, `interpreter.rs`, and the test suites. No
`liquers-lib`, `liquers-axum` or `liquers-web` change; `liquers-py`'s `apply_recipe` is `todo!()`.

## Documentation Intent

- **Reference: extend, not new.** `reference/api/DOC_08_RECIPES_PLANS.md` already owns "Freezing"
  and "Predecessor boundaries". A second document would split one mechanism across two.
- **Guide: neither.** `plan-cwd-freeze` Phase 1 decided this and nothing here overturns it — there
  is no repeatable developer task, only internal behaviour. A recipe author's one question,
  "why was my boundary not cut", is answered by the `init_info` the design emits.
- **Other documents: none**, beyond issue updates.
- **Update:** `DOC_08_RECIPES_PLANS.md`; the rustdoc on `Recipe`, `PlanBuilder` and the cut, where
  a contributor lands from a stack trace; `specs/README.md`. Exact paths and changes at Phase 2.

## Open Questions

Two semantic questions were raised and settled in Phase 1 discussion — where a boundary may sit
relative to a payload need, and what a recipe-level `volatile:` means. Both are recorded in
`DESIGN.md` and formalised at Phase 2.

Genuinely open, none of them blocking:

1. The one-boundary-per-plan limit. `PlanBuilder` keeps only the outermost predecessor, so a plan
   has a single candidate position and the walk must re-derive deeper ones. Whether that stays a
   re-derivation or becomes recorded state is a Phase 2 trade.
2. Whether a recipe-level `expires:` should be treated as strictly as `volatile:`. A finite
   expiration speaks about the result, and a pure prefix could still be cached.
3. What "equivalent" is defined to cover. Cutting changes asset count, dependency edges and
   metadata by design, so the comparison set is a decision, not an omission.

## Phase 1 Critical Review

Run inline against `references/review-checklist.md`.

**Scope clarity.** Purpose is two sentences. Interactions cover Plan/PlanBuilder, Recipe, Assets
and Context; Query, Store, Value types, Web/API and UI are **not** interactions here and are
listed as such rather than omitted — the change sits below the command layer and adds no value
type, no endpoint and no widget. Crate placement is one crate, with the dependency flow untouched.

**Scope size.** In the Goldilocks zone, but only just above the lower bound: the verified fix is
small. What makes it a project rather than a bug fix is that three further decisions hang off it
(where a boundary may sit, what a recipe flag means, how a plan's coupled fields are copied) and
that the deliverable the issue actually names is a sixteen-shape equivalence suite.

**No duplication.** The machinery exists; this design does not add a parallel mechanism. Checked
for an existing equivalence harness — `evaluate_both_ways` in `interpreter.rs`'s `#[cfg(test)]`
mod, three shapes, one compared property — and the intent is to move and widen it, not to write a
second one. `has_volatile_dependencies` and `Context::schedule_dependency_asset` already answer
the volatility and payload questions this design consults; nothing re-implements them.

**Aligns with Liquers philosophy.** A boundary is a query becoming an asset, which is the layered
model working as designed rather than an exception to it.

**Open questions.** Three, none blocking. Two earlier semantic questions were settled in
discussion and are recorded rather than left implicit.

**One weakness, recorded rather than fixed.** "Documentation Intent" declines a guide on the
grounds `plan-cwd-freeze` used. That reasoning should be re-tested at Phase 3 against the
`init_info` messages: if a recipe author needs to be taught how to read them, the `neither`
becomes wrong.
