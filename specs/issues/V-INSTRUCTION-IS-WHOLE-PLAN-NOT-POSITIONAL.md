---
id: V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL
kind: issue
title: The `v` instruction marks the whole plan volatile rather than the steps after it
status: draft
priority: P3
complexity: M
area: [core/plan, core/query]
design: 
created: 2026-08-26
github:
---
## Problem

`v` is implemented and behaves as a builder instruction should: `PlanBuilder::process_action`
intercepts it before command metadata is resolved (`liquers-core/src/plan.rs:1486`), like `q`
and `ns`, rejects parameters, and emits **no step** — so it is an identity on the value.

What it does with volatility is not positional:

```rust
self.mark_volatile("Volatile due to instruction 'v'");
return Ok(()); // Don't create Step::Action for 'v'
```

`mark_volatile` sets the builder's single `is_volatile` flag, which becomes `Plan::is_volatile`.
So `a/b/v/c/d` is volatile *throughout*, not volatile from `v` onward, and `v` at the end of a
chain means the same as `v` at the start. Its position carries no information.

## Impact

No wrong result today: whole-plan volatility is the conservative direction, so a plan marked
this way is recomputed more often than strictly needed, never less. Nothing is broken, which is
why this is P3.

What is missing is expressiveness. There is currently **no way to say where a volatile part of
a plan begins.** A recipe's `volatile:` flag carries no position either — that is decided in
`predecessor-cut-equivalence`, which reads a volatile recipe as volatile from its first action
precisely because the flag cannot say otherwise. An author who wants "this prefix is stable,
everything after it must be recomputed" has no way to write it.

That matters more once predecessor boundaries are cut, because a positional `v` and the
boundary walk would compose exactly: the walk cuts at the last candidate whose plan is
non-volatile, so a positional `v` would place the boundary at `v`. The author's declared
volatility boundary and the cache boundary would be the same point, written once.

## Expected behaviour

`v` marks the steps from its own position onward volatile, leaving the prefix non-volatile;
`Plan::is_volatile` stays the whole-plan roll-up it is today, so nothing downstream changes for
plans that do not use `v` mid-chain.

Two things need care and are the reason this is `M` rather than `S`:

- **The builder tracks one `is_volatile` flag.** Positional volatility means recording where it
  started, in the same manner `predecessor_steps` records a step index — and, like it, surviving
  `Recipe::to_plan`'s prepended prologue.
- **`v` at the head must keep meaning "all of it"**, which it does under the new reading too, so
  existing queries are unaffected. `v` in the *middle* changes meaning: today the prefix is
  volatile, after the change it is not. Worth checking whether any recipe in the wild relies on
  the current reading before changing it.

## Discovery

Raised during `predecessor-cut-equivalence` review, 2026-08-26, while deciding what a
recipe-level `volatile:` should mean. The question was whether a positional instrument exists to
express the fine-grained case; `v` is the closest and does not. See that design's `DESIGN.md` notes on what a recipe-level `volatile:` means.
