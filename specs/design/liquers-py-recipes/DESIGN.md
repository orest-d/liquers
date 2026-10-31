---
id: LIQUERS-PY-RECIPES
kind: design
title: Recipe providers written in Python
form: compact
workflow: liquers-project
status: draft
phase: high-level
readiness: needs-decision
autofix: not-eligible
area: [py, core/assets, core/context]
issues: [PY-NO-LANGUAGE-DEFINED-RECIPE-PROVIDER]
affects_docs: [specs/reference/PYTHON_INTEGRATION.md, specs/reference/ENVIRONMENT_CONFIG.md, specs/guides/PYTHON_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, liquers-py/README.md, specs/README.md]
created: 2026-10-31
---
# liquers-py-recipes — recipe providers written in Python

The `RECIPE` feature of liquers-py, milestone M4 of the liquers-py plan together with
[`liquers-py-store`](../liquers-py-store/DESIGN.md). It builds on the configurable recipe provider
chain (`recipe-provider-chain-config`, PR #224): a Python provider is one more entry type in
`config["recipes"]`, declared through `EnvironmentBuilder::with_recipe_provider_factory` (the
plan's M4 says `with_appended_recipe_provider`; PR #224 made the factory the route for a configured
entry, so this deliberately updates the plan). Compact form, since the source issue is `M`.

## Phase 1: High-Level Design

### Purpose

Let Python name results that no `recipes.yaml` lists: a catalogue in a database, reports generated
per customer, a pattern the program knows. A Python object answers "which query makes this key",
and the recipe provider chain asks it beside `recipes_yaml` and `manifest` (`VISION.md`, Python
data science).

### Problem Example

```python
import liquers as lq

class Reports:                                  # a provider with one method
    def recipe_opt(self, key):                  # an lq.Recipe, query text, or None if not mine
        if str(key.parent()) == "reports":
            return f"hello/{key.filename()}"    # cwd defaults to the key's folder
        return None

env = lq.Environment(config={"recipes": ["recipes_yaml", {"type": "python", "object": "reports"}]})
env.register_recipe_provider("reports", Reports())

@env.command
def hello():
    return "Hello"

assert env.evaluate("-R/reports/summary.txt") == "Hello"
```

**Today** `python` is an unknown entry type and the build fails. **After** the chain asks
`Reports.recipe_opt` for every key, and `reports/summary.txt` evaluates `hello/summary.txt`.

### Scope and Acceptance Criteria

- **AC-1** A Python provider in the chain
  WHEN the problem example runs
  THEN `-R/reports/summary.txt` evaluates to `"Hello"` (`RECIPE06`), and a key no member owns and
  no store holds raises `AssetNotFoundError`
- **AC-2** "Not mine" is cheap and silent
  WHEN `recipe_opt` returns `None`
  THEN the other members decide the key; two members answering for one key is the chain's
  configuration error naming both
- **AC-3** Return forms
  WHEN `recipe_opt` returns an `lq.Recipe` or a query string
  THEN either becomes the key's recipe, its `cwd` the key's folder unless the recipe sets one
- **AC-4** Errors map through ERROR
  WHEN a provider method raises (`RECIPE04`)
  THEN the lookup fails with that `lq.LiquersError`'s type, or `ExecutionError`, carrying the
  Python traceback
- **AC-5** The protocol is resolved once
  WHEN an object without `recipe_opt` is registered, or a configuration names an unregistered object
  THEN registration, or the build, raises `ConfigurationError` naming the method or the object
- **AC-6** Listing and containment
  WHEN a provider has `list(folder)`, and another lacks it
  THEN every listed name is contained and addressable, and an unlisted name its `recipe_opt`
  answers is addressable too (`RECIPE02`); the second lists nothing and contains nothing, yet its
  keys evaluate (question 1)
- **AC-7** Recipe metadata survives
  WHEN a returned recipe is volatile or has an expiry (`RECIPE05`)
  THEN its plan is volatile and the asset expires accordingly, and the plan is valid (`RECIPE03`)
- **AC-8** Callbacks may evaluate
  WHEN a provider method evaluates a query through its environment (`RECIPE07`)
  THEN it completes under the `RUNTIME` rules, with no deadlock; `def` and `async def` both work
- **AC-9** Inspecting the chain
  WHEN Python calls `env.recipes.recipe_opt(key)`, `recipe(key)`, `list(folder)`, `contains(key)`
  or their `_async` twins
  THEN it gets the chain's answers (`RECIPE01`-`RECIPE03`, `RECIPE05`)
- **AC-10** Registration timing and release
  WHEN a provider is registered after the first query, or `shutdown()` is called
  THEN the next use builds a new chain with the store kept (a provider holds no data, unlike a
  store object); shutdown releases every provider object (a `debug-handles` count)
- **AC-11** Several Python providers
  WHEN a chain holds two Python providers (question 2)
  THEN both are asked for every key, and two answering for one key is a configuration error naming both

Non-goals: caching recipes in the adapter (a provider caches itself); Python recipe providers in
`liquers-validate` (`VALIDATE-CANNOT-SEE-NON-STANDARD-RECIPE-PROVIDERS`, documented as a limitation);
stores.

Core interactions: recipes (`AsyncRecipeProvider` adapter, a `RecipeProviderFactory` for `python`
through `EnvironmentBuilder::with_recipe_provider_factory`); bindings (`register_recipe_provider`,
`Environment(recipe_providers=…)`, `env.recipes`). Crate: `liquers-py` (a new
`src/recipe_provider.rs`; `src/recipes.rs` keeps the `Recipe` wrappers). `recipe_plan` needs the
plan's dependency analysis, which core keeps private (`create_plan_with_init_metadata`,
`analyze_plan_dependencies`): a small core visibility change, decided in Phase 2.

Documentation intent: a §Recipe providers in `specs/reference/PYTHON_INTEGRATION.md` (protocol,
derived methods, errors, threading); a "Recipe providers" section in `specs/guides/PYTHON_GUIDE.md`
§6 beside "Named results: recipes", stating the listing contract
(`RECIPE-ONLY-FOLDER-NAMES-HIDE-A-CREATED-FOLDER-FROM-ITS-PARENT`) and that `liquers-validate` cannot
see Python recipes; the `python` row of `specs/reference/ENVIRONMENT_CONFIG.md`'s entry types;
docstrings on every new class and method; the `RECIPE` row of `liquers-py/README.md`;
`LANGUAGE-INTEGRATION_GUIDE.md` §6 Python; `specs/README.md`. Open problems documented: the rebuild
on late registration drops the asset cache (`LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION`).

### Design Readiness

- **Readiness:** `needs-decision`
- **Automatic fixing:** `not-eligible`: a new public Python API.
- **Leading issue:** Open design question 1.
- **Explanation:** the chain already defines ownership, errors and listing; what remains is the
  Python surface, and questions 1-2 change what a provider author writes.
- **Open questions:**
  1. **Open design - an absent `list`.** Recommended: the provider lists nothing and contains
     nothing (it is generative: addressable ⊋ listed, which the guide allows), so the chain's
     listing is the other members'; its keys are still found through `recipe_opt`. This departs
     from the guide's "absent optional method fails loudly" rule because in a chain a failing member
     fails *every* listing. Alternative: `KeyNotSupported`, making `list` mandatory in practice.
  2. **Open design - several Python providers.** The chain refuses an entry type listed twice
     (`expand_recipes`), so today a chain holds one `python` entry. Recommended: the entry takes
     `object: name` or `objects: [a, b]`, and the adapter asks them in order with the chain's own
     rules (one owner per key, any error fails). Alternatives: a core change keying duplicates on
     type and `object`; or one Python provider per chain, documented.
  3. **Open design - registration API.** Recommended: `env.register_recipe_provider(name, obj)`
     and `Environment(recipe_providers={...})`, names separate from store objects, mirroring
     `liquers-py-store` question 2. Unlike a store object a late provider is accepted: the chain
     is rebuilt and holds no data, only the asset cache is lost.
  4. **Proposed resolution - return forms.** `recipe_opt` returns an `lq.Recipe`, query text, or
     `None`; anything else (a `dict` included) fails the lookup with `ExecutionError` naming the
     type. A recipe without `cwd` gets the key's folder, as `recipes_yaml` sets it.
  5. **Proposed resolution - containment is listing.** `contains` is core's meaning ("listed"):
     taken from `contains(key)` when the provider has it, else from `list`; "producible" is
     `can_make`, whose core default already asks `recipe_opt`.
  6. **Proposed resolution - the adapter implements** `recipe`, `recipe_plan`, `has_recipes` and
     `assets_with_recipes` (required trait methods) from `recipe_opt` and `list`; `get_asset_info`
     takes core's default; an optional `directory_changed(folder)` is forwarded.
  7. **Proposed resolution - sync and async**, keys as `lq.Key`, the protocol inspected at
     registration: as `liquers-py-store` questions 6 and 7. A callback may evaluate through its
     `Environment` under the `RUNTIME` rules (AC-8); evaluating the key it is resolving recurses,
     which is documented.

### Design Dependencies

- `PY-NO-LANGUAGE-DEFINED-RECIPE-PROVIDER` — **implemented** by this design.
- `recipe-provider-chain-config` — **required** (complete): the chain and the factory seam.
- `liquers-py-store` — **sibling**; same registration and method-dispatch rules.
- `language-late-registration-without-rebuild` — **ordering**: it removes the command rebuild; if
  it lands first, AC-10's late provider needs its own rebuild of the chain, stated in Phase 2.

### References

- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §Service adapters, §RECIPE, Appendix A RECIPE
- `specs/reference/ENVIRONMENT_CONFIG.md` §The recipe provider chain
