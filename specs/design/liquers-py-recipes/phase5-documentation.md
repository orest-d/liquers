# Phase 5: Documentation - liquers-py-recipes

## Completion Preconditions

- [x] Implementation is finished and validated: Phase 4 steps 1-5 (836b00a, 30fe5c7); `cargo test
  -p liquers-core` recipe tests (76) and `cargo test -p liquers-lib --lib --tests` pass; liquers-py
  suite 346 passed; PR #229 CI
- [x] All user comments are answered or incorporated: repeated types with different arguments
  (Orest, 2026-11-01); factory model with free type names and arguments (2026-10-31)
- [x] All review comments are answered or incorporated: compact form, author passes per phase
- [x] Documentation is consistent with the implemented and tested behavior: guide examples run
- [x] Documentation is included in the implementation PR when practical: PR #229

## Implementation Summary

liquers-py now has the `RECIPE` feature. `Environment.register_recipe_provider_factory(type,
factory)` declares a `recipes:` entry type; an entry `{type, …}` calls the factory with its other
keys. The provider object (`src/recipe_provider.rs`) needs `recipe_opt(key)` returning `lq.Recipe`,
query text or `None`; `assets_with_recipes`, `contains` and `directory_changed` are optional, each
`def` or `async def`. A recipe without `cwd` gets the key's folder; its plan is the new public
`Recipe::plan_with_dependencies`, so `volatile` and `expires` behave as for `recipes.yaml`.
`env.recipes` (`lq.RecipeProviders`) inspects the whole chain with `_async` twins, and `lq.Recipe`
takes `cwd`, `volatile` and `expires` keywords.

In `liquers-core`, `expand_recipes` refuses only an identical entry, so one type with different
arguments makes several members, named `type#position` when the type repeats; an appended
provider is still skipped when its type is listed.

Conformance: as approved. Added: the shared argument binding and factory call with store factories.
Omitted: nothing.

## Documentation Delivered

### New Reference Documents
None: `reference/PYTHON_INTEGRATION.md` §10 Recipe providers, as planned.

### New Guide Documents
None: `guides/PYTHON_GUIDE.md` §6 "Recipe providers written in Python", as planned.

### Existing Documents Reviewed or Updated
`affects_docs`: `reference/PYTHON_INTEGRATION.md`, `reference/ENVIRONMENT_CONFIG.md` (repeated
types, member names, integration entry types replacing `{type: python, …}`),
`reference/RECIPES_AND_PLANS.md` (`Recipe::plan_with_dependencies`), `guides/PYTHON_GUIDE.md`,
`guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` (the identical-entry rule),
`guides/LANGUAGE-INTEGRATION_GUIDE.md` (§RECIPE factory model, §6 Python), `liquers-py/README.md`
(`RECIPE` row, `ENVIRON` limitation lifted), `specs/README.md`. Each has a 2026-11-01 History row.

### Links and Capability Map
`specs/README.md`: the "Python stores and recipe providers" line is "built".

## Issues Filed

- `RECIPE-MISSING-RECIPE-IS-A-GENERAL-ERROR` (P3): the chain's `recipe` for a key no member has
  raises `GeneralError`, while a `recipe_opt`-based provider raises `KeyNotFound`.

Closed: `PY-NO-LANGUAGE-DEFINED-RECIPE-PROVIDER`.

## Important Learning

- "Every member is asked" makes a cheap `None` for foreign keys the most important property of a
  language provider; the guide's example leads with it.
- The provider-plan helper was private, so an external provider could not build the plan the
  built-in ones do; making it public was smaller than reproducing the analysis.

## Conformance and Remaining Work

Requested: Python recipe providers placed by configuration, repeatable with different arguments.
Implemented in full; nothing remains beyond the issue above and the pre-existing ones recorded in
Phase 2's preflight (`VALIDATE-CANNOT-SEE-NON-STANDARD-RECIPE-PROVIDERS`,
`RECIPE-DEPENDENCY-KEY-IS-NEVER-VERSIONED`).

## Validation

`python3 scripts/docs_index.py`; `validate_phase.py liquers-py-recipes 5`; guide examples and
docstring tests pass.
