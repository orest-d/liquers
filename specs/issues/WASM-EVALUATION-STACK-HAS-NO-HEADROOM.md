---
id: WASM-EVALUATION-STACK-HAS-NO-HEADROOM
kind: issue
title: A few bytes more in the plan walk's future overflow the wasm stack in a guide example
status: draft
priority: P2
complexity: M
area: [web, core/plan, core/assets]
created: 2026-11-17
github:
---

## Problem

The JavaScript guide examples (`liquers-web/scripts/test-guide-examples.sh --features records`)
evaluate under the browser's default wasm stack with almost no headroom. During
`recipe-dependency-versions` (PR #276), adding one awaited, non-boxed provider lookup plus a
`register_recipe_version` call to `DependencyWalk::visit` (`liquers-core/src/plan.rs`) made the
example after `web/01-introduction.md:28` fail with `RuntimeError: memory access out of bounds`
(a stack overflow) inside `AssetRef::compute` → `Recipe::to_plan` → `parse_query`. Boxing the new
awaits (`maybe_boxed`) fixed it. Native tests and the wasm conformance suites passed throughout.

So the evaluation path's stack use on wasm is close to its limit, and any change that grows an
inlined future on that path (the plan walk, `evaluate_recipe_outcome`, `finalize_plan`) can break
the web build with an error that names neither the change nor the cause.

## Problem Example

Revert the `maybe_boxed()` wrappers around the recipe-version registrations in
`DependencyWalk::visit` (commit after 4c1215f on `claude/project-thread-nbt4m2`), then run
`./liquers-web/scripts/test-guide-examples.sh --features records`: the run stops at the example
after `web/01-introduction.md:28` with `memory access out of bounds`.

## Expected behaviour

The evaluation path has measurable headroom on wasm: either a larger stack for the wasm build
(a linker `stack-size` setting), boxing of the large futures on that path by rule, or a test that
reports the stack used by a reference evaluation so a regression is caught by name.

## Discovery

`recipe-dependency-versions` implementation, 2026-11-17 (CI job `node` of `liquers-web-tests`).
