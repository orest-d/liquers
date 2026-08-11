# Phase 1: High-Level Design - Recipe CWD Propagation and Resolution
## Feature Name
Recipe CWD Propagation and Relative Query Resolution
## Purpose
Make a recipe's authoritative working key affect planning, dependency identity, scheduling, and interpretation: provider-loaded recipes use the containing `recipes.yaml` folder, while programmatic recipes respect explicit `cwd`.
## Current Behaviour
- `Recipe::cwd` is a public Serde field. `Recipe::new` leaves it empty; callers and custom providers may set it programmatically.
- `DefaultRecipeProvider` assigns the containing folder when loading `recipes.yaml` and rejects a recipe that already specifies `cwd`.
- `Recipe::key`, `get_cwd`, and `store_to_key` respect `cwd`, but `Recipe::to_plan` and all built-in `Environment::apply_recipe` implementations ignore it.
- `-R-cwd/<key>` parses as a resource selector that builds `Step::SetCwd`; execution updates `Context::cwd_key`, but plan pre-scheduling and context dependency scheduling do not consult it.
- `Plan::init_steps` are planning diagnostics copied to metadata; unlike `Plan::steps`, they are not executed.
## Core Interactions
- **Recipes and queries:** Preserve relative plan operands and propagate provider-derived or programmatic `cwd` as an ordered executable context change.
- **Assets:** Plan finalization, pre-scheduling, dependency records, cycle checks, cache identity, and evaluation use the same resolved query/key.
- **Store, commands, value types, Web/API, and UI:** No direct interface changes; existing callers inherit corrected core behavior.
## Crate Placement
`liquers-core`, across recipes, query/plan evaluation, context, and asset dependency integration; the invariant belongs below environment adapters.
## Intended Solution
`PlanBuilder` preserves source-relative operands; `Recipe::to_plan` prepends recipe-derived `SetCwd` and adds an init `Info` diagnostic that records that the recipe established the CWD.
Resolution is ordered: resolve relative `SetCwd` first, then use it subsequently; if a relative operand needs a missing CWD, use logical root `/` and warn, while absolute operands remain unchanged and silent.
The interpreter is the semantic authority: it maintains live CWD in `Context`, while dependency analysis and pre-scheduling simulate the same ordered cursor without rewriting the plan.
The executable `SetCwd` is deliberately retained even when a consumer can resolve static operands: commands and nested plans can observe the context state, so removing it requires a separate proof that the state change is unobservable.
With initial CWD `a/b`, `-R-cwd/../c/-/action-~X~-R/./hello.txt~E` resolves the link like `action-~X~-R/a/c/hello.txt~E`.
## Documentation Intent
**Reference:** Extend `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` and `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` with the verified recipe-CWD contract.
**Guide:** Extend `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` so integration authors can rely on provider-derived and programmatic working keys.
**Other documents to create:** None; the capability belongs in existing references and guide.
**Specific documents to update:** `specs/reference/PROJECT_OVERVIEW.md`, `specs/README.md`, and the linked issue; internal contributors should not need this design to understand the invariant.
## Open Questions
None blocking at the high level; Phase 2 specifies interpreter ownership and future plan-rewrite constraints.
