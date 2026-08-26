# Phase 1: High-Level Design - Predecessor Cut Equivalence

## Feature Name

`predecessor-cut-equivalence` — make `Plan::cut_predecessor` observably equivalent to expansion.

## Purpose

Cutting a plan at its **outermost cacheable predecessor** — the longest leading prefix that is
neither volatile nor payload-requiring — is what lets the `AssetManager` cache, share, expire and
schedule an intermediate instead of recomputing it inside every consumer. That is the intended
default. It is not the default today only because it does not yet work: `plan-cwd-freeze` built
the machinery and left it switched off, four measured divergences deep.

This effort makes it correct and makes it the default.

## Core Interactions

- **Plan / PlanBuilder** — freezing resolves the recorded predecessor; the builder records one
  predecessor per plan, which is what makes a per-candidate cut decision possible.
- **Recipe** — prepends a `SetCwd` the builder never emitted, and carries `volatile:` / `expires:`
  declarations that exist in no query.
- **Assets** — a boundary becomes a cache entry, which is why payload and volatility bear on
  where it may be placed.
- **Context** — `schedule_dependency_asset` chooses the payload path from the boundary query's
  own plan.

## Scope and Non-Goals

**In scope:** one cut, at the outermost cacheable predecessor, as the default policy; the
divergences that stop it being correct; and the equivalence suite that holds it correct.

**Not a goal: decomposing a plan completely**, a boundary at every action. It is interesting
mainly because it is possible, and the case for it is a volatile plan — where nothing is
cacheable, so an asset per step buys the dependency graph and parallel scheduling rather than
caching. Nothing here forecloses it; nothing here builds it.

That distinction also answers `DOC_08`'s standing argument that the memory-versus-recomputation
trade is per query rather than global. Cutting *everywhere* retains every intermediate, which is
what makes a global default look wrong. One cut retains one intermediate — bounded, and the one
most likely to be shared by other queries.

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
- **Update:** `DOC_08_RECIPES_PLANS.md` — including "Why the default should make the predecessor
  available", whose closing paragraph defers the decision to `CORE-PLAN-POLICY-AND-DEFAULTS` and
  is superseded by it; the rustdoc on `Recipe`, `PlanBuilder` and the cut, where a contributor
  lands from a stack trace; `specs/README.md`. Exact paths and changes at Phase 2.

## Open Questions

Two semantic questions were raised and settled in Phase 1 discussion — where a boundary may sit
relative to a payload need, and what a recipe-level `volatile:` means. Both are recorded in
`DESIGN.md` and formalised at Phase 2.

Genuinely open, neither blocking:

1. Whether a recipe-level `expires:` should be treated as strictly as `volatile:`. A finite
   expiration speaks about the result, and a pure prefix could still be cached soundly.
2. What "equivalent" is defined to cover. Cutting changes asset count, dependency edges and
   metadata by design, so the comparison set is a decision, not an omission — and it becomes a
   shipping gate rather than a preparatory exercise once the default flips.

Whether `PlanBuilder` keeps the candidate list it already computes, or the cut recovers it, is an
implementation detail for Phase 2 rather than an open question here.

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
