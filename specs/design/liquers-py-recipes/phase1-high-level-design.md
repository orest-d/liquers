# Phase 1: High-Level Design — liquers-py-recipes

## Purpose

Let Python name results that no `recipes.yaml` lists: a catalogue in a database, reports generated
per customer, a pattern the program knows. A Python object answers "which query makes this key",
and the recipe provider chain asks it beside `recipes_yaml` and `manifest` (`VISION.md`, Python
data science).

## Problem Example

```python
import liquers as lq

class Reports:                                  # a provider with one method
    def __init__(self, folder):
        self.folder = folder
    def recipe_opt(self, key):                  # an lq.Recipe, query text, or None if not mine
        if str(key.parent()) == self.folder:
            return f"hello/{key.filename()}"    # cwd defaults to the key's folder
        return None

env = lq.Environment(config={"recipes": ["recipes_yaml", {"type": "reports", "folder": "reports"}]})
env.register_recipe_provider_factory("reports", Reports)   # (i) register; parameters = entry arguments

@env.first_command
def hello():
    return "Hello"

assert env.evaluate("-R/reports/summary.txt") == "Hello"
```

**Today** `reports` is an unknown entry type and the build fails. **After** the chain asks
`Reports.recipe_opt` for every key, and `reports/summary.txt` evaluates `hello/summary.txt`.

## Scope and Acceptance Criteria

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
- **AC-5** The factory and the protocol are checked
  WHEN a factory is registered under a taken name, an entry lacks a required argument or adds an
  unknown one, the factory raises or calls its environment, or it returns an object without `recipe_opt`
  THEN registration refuses the name; the build raises `ConfigurationError` naming the entry and
  the cause, and none hangs
- **AC-6** Listing and containment
  WHEN a provider has `assets_with_recipes(folder)`, and another lacks it
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
  WHEN Python calls `env.recipes.recipe_opt(key)`, `recipe(key)`, `recipe_plan(key)`,
  `assets_with_recipes(folder)`, `contains(key)`, `can_make(key)` or their `_async` twins
  THEN it gets the chain's answers (`RECIPE01`-`RECIPE03`, `RECIPE05`)
- **AC-10** Registration timing and release
  WHEN a factory is registered after the first use, or `shutdown()` is called
  THEN the next use rebuilds the environment and its chain with the store kept (`liquers-py-store`
  Decision 2), so a factory registered after a failed build lets it build; shutdown releases every
  factory and provider object (a `debug-handles` count)
- ~~**AC-11** Several Python providers~~ — replaced by AC-12 when the registration model changed
  (Scope Changes, 2026-10-31).
- **AC-12** One factory, several entries
  WHEN a chain lists the `reports` type twice with different arguments
  THEN both providers are asked for every key (question 2); an identical entry listed twice is
  still a configuration error

Non-goals: caching recipes in the adapter (a provider caches itself); Python recipe providers in
`liquers-validate` (`VALIDATE-CANNOT-SEE-NON-STANDARD-RECIPE-PROVIDERS`, documented as a limitation);
stores.

## Core Interactions

Recipes: an `AsyncRecipeProvider` adapter and one `RecipeProviderFactory` holding every Python
type, through `EnvironmentBuilder::with_recipe_provider_factory`. Bindings:
`register_recipe_provider_factory`, `env.recipes`.

## Crate Placement

`liquers-py` (a new
`src/recipe_provider.rs`; `src/recipes.rs` keeps the `Recipe` wrappers), and `liquers-core` for
question 2 and for `recipe_plan`'s dependency analysis, which core keeps private
(`create_plan_with_init_metadata`, `analyze_plan_dependencies`): a small visibility change, Phase 2.

## Documentation Intent

A §Recipe providers in `specs/reference/PYTHON_INTEGRATION.md` (factory
registration, protocol, derived methods, errors, threading); a "Recipe providers" section in
`specs/guides/PYTHON_GUIDE.md` §6 beside "Named results: recipes", stating the listing contract
(`RECIPE-ONLY-FOLDER-NAMES-HIDE-A-CREATED-FOLDER-FROM-ITS-PARENT`) and that `liquers-validate` cannot
see Python recipes; `specs/reference/ENVIRONMENT_CONFIG.md` (integration-registered entry types,
the duplicate rule of question 2); docstrings on every new class and method; the `RECIPE` row of
`liquers-py/README.md`; `LANGUAGE-INTEGRATION_GUIDE.md` §RECIPE and §6 Python; `specs/README.md`.
The superseded `{type: python, object: …}` sketch is replaced in `LANGUAGE-INTEGRATION_GUIDE.md`
§RECIPE, `ENVIRONMENT_CONFIG.md` (`[default, {type: python, …}]`) and
`PY-NO-LANGUAGE-DEFINED-RECIPE-PROVIDER`. Open problems documented: every Python provider takes
the GIL on every key lookup, since the chain asks every member; a late registration drops the asset
cache (`LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION`); a provider changing its answer
does not expire assets (`RECIPE-DEPENDENCY-KEY-IS-NEVER-VERSIONED`).

## Design Readiness

- **Readiness:** `ready`
- **Automatic fixing:** `not-eligible`: a new public Python API and a core rule change.
- **Leading issue:** None.
- **Explanation:** Phase 1 approved by Orest on 2026-11-01 (`proceed all`), pre-approving Phases
  2-4, the implementation and Phase 5. Questions 1-2 are his decisions; questions 3-7 are explicit
  assumptions taken as recommended. Provisional until the Phase 4 consistency pass.
- **Decision log (pre-approval):** (a) Phase 2 review: an *appended* entry (liquers-lib's
  `with_records_recipe_provider` appends a bare `manifest`) is still skipped when any member has its
  type, so the repeat rule of question 2 applies to documents only. Within question 2; no decision needed.
- **Open questions:**
  1. **Decided (Orest, 2026-11-01, as recommended) - an absent `assets_with_recipes`.** The method is the protocol's listing and
     is supported. Recommended when it is absent: the provider lists nothing and contains nothing
     (it is generative: addressable ⊋ listed, which the guide allows), so the chain's listing is
     the other members'; its keys are still found through `recipe_opt`. This departs from the
     guide's "absent optional method fails loudly" rule because in a chain a failing member fails
     *every* listing. Alternative: `KeyNotSupported`, making it mandatory in practice.
  2. **Decided (Orest, 2026-11-01) - the same type twice.** "Types with (different) arguments can
     be repeated", for example one provider type making PNG from SVG and another entry of it making
     JPG from SVG. So for *every* type an identical entry (type and arguments) is refused and the
     same type with different arguments is allowed; the `repeatable` opt-in below is dropped. Two
     such members claiming one key is the chain's ownership error, as for any two members.
     Original question: `recipe-provider-chain-config` made an entry type listed
     twice a configuration error (`expand_recipes`), which forbids one factory with two argument
     sets. Recommended: a type may opt in with `repeatable` on `RecipeProviderTypeInfo` (default
     false, set for Python types only), so `[default, {type: manifest, x: 1}]` stays refused; an
     *identical* entry is always refused; a member keeps its bare type as its name unless repeated,
     then `type#position` (`liquers-core`, `ENVIRONMENT_CONFIG.md`). Alternative: keep the rule; a
     factory then appears once per chain.
  3. **Proposed resolution - the factory declaration**, as `liquers-py-store` questions 4-6:
     `env.register_recipe_provider_factory(type, factory, *, label="", doc="", arguments=None)`;
     arguments from the factory's keyword parameters or a list of `lq.StoreArgumentInfo` (core's
     `RecipeProviderTypeInfo` reuses it), checked and defaulted by liquers-py, unknown keys refused
     unless it takes `**kwargs`; a unique name (not a type of the kind's factory, nor `default`,
     `trivial`, `none`, `no_recipes`); a callable, not `async def`, called at build, which may not
     use its environment; its result inspected at once.
  4. **Proposed resolution - return forms.** `recipe_opt` returns an `lq.Recipe`, query text, or
     `None`; anything else (a `dict` included) fails the lookup with `ExecutionError` naming the
     type. A recipe without `cwd` gets the key's folder, as `recipes_yaml` sets it.
  5. **Proposed resolution - containment is listing.** `contains` is core's meaning ("listed"):
     taken from `contains(key)` when the provider has it, else from `assets_with_recipes`;
     "producible" is `can_make`, whose core default already asks `recipe_opt`.
  6. **Proposed resolution - the adapter implements** `recipe`, `recipe_plan`, `has_recipes` and
     `assets_with_recipes` (required trait methods) from `recipe_opt` and the Python listing;
     `get_asset_info` takes core's default; an optional `directory_changed(folder)` is forwarded.
  7. **Proposed resolution - sync and async**, keys as `lq.Key`: as `liquers-py-store` questions 8
     and 9. A callback may evaluate through its `Environment` under the `RUNTIME` rules (AC-8);
     evaluating the key it is resolving recurses, which is documented.

## Design Dependencies

- `PY-NO-LANGUAGE-DEFINED-RECIPE-PROVIDER` — **implemented** by this design.
- `recipe-provider-chain-config` — **required** (complete): the chain and the factory seam; its
  duplicate rule is revisited (question 2).
- `liquers-py-store` — **sibling**; same factory model (its Decisions 1-2) and method dispatch.
- `language-late-registration-without-rebuild` — **ordering**: it removes the command rebuild; if
  it lands first, AC-10's late factory needs its own rebuild, stated in Phase 2.

## Scope Changes

- **2026-10-31** — Orest: a configuration names a Python *factory*, its arguments the factory's
  parameters, in place of `{type: python, object: …}`; `assets_with_recipes` is the Python listing
  method; late registration rebuilds. AC-5, AC-6, AC-9, AC-10 reworded; AC-11 replaced by AC-12.
  Questions: the former 3 (registration API) retired into the store design's decisions, so 3 is
  now the factory declaration; former 2 (several providers) became the core duplicate rule. Adds a
  small `liquers-core` change (question 2). Phases: 1.
- **2026-11-01** — Orest decided question 2: any type may repeat with different arguments. AC-12
  reworded. Phases: 1.
- **2026-11-01** — converted from the compact to the full form: with the `liquers-core` rule change
  and the Python surface (`lq.RecipeProviders`, the `lq.Recipe` properties) the design is `L`, and
  the compact form's five-page limit could not hold Phases 2-4. Phases: all.

## References

- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §Service adapters, §RECIPE, Appendix A RECIPE
- `specs/reference/ENVIRONMENT_CONFIG.md` §The recipe provider chain
