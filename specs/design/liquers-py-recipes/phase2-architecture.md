# Phase 2: Solution & Architecture — liquers-py-recipes

## Overview

A new `liquers-py/src/recipe_provider.rs` holds `PyRecipeProvider`, an
`AsyncRecipeProvider<PyEnvironment>` adapter over a Python object resolved once into a method
table, and `PyRecipeProviderFactory`, one `RecipeProviderFactory<PyEnvironment>` holding every
Python entry type of an environment. `EnvCell` keeps the registered types among its declarations
and passes the factory to `EnvironmentBuilder::with_recipe_provider_factory` on every build. Python
is called through `src/pycall.rs`, shared with `liquers-py-store` (its Phase 2). `env.recipes` is an
`lq.RecipeProviders` handle on the built chain. `liquers-core` changes in two small places: the
duplicate rule (question 2) and a public plan constructor the adapter needs.

Rejected: `with_appended_recipe_provider` (a provider supplied in code, not placed by the
document); an object registry with a fixed `python` type (`liquers-py-store` Decision 1); caching
answers in the adapter (a provider caches itself, Phase 1 non-goal).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `RECIPE-DEPENDENCY-KEY-IS-NEVER-VERSIONED` | draft | P3 | A provider changing its answer does not expire assets | No | No | Documented |
| `DEPENDENCY-ANALYSIS-USES-CALLER-CWD-FOR-A-CWD-LESS-RECIPE` | draft | — | Would misplace a `cwd`-less recipe | No | No | Avoided: the adapter always sets `cwd` |
| `VALIDATE-CANNOT-SEE-NON-STANDARD-RECIPE-PROVIDERS` | draft | — | Python recipes invisible to `liquers-validate` | No | No | Documented |
| `RECIPE-ONLY-FOLDER-NAMES-HIDE-A-CREATED-FOLDER-FROM-ITS-PARENT` | draft | P3 | Listing contract | No | No | Documented in the guide |
| `LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION` | draft | P3 | A late registration rebuilds | No | No | Documented |
| `language-late-registration-without-rebuild` (design) | in_review | P3 | Removes the command rebuild | No | No | A recipe factory registration still marks the environment `Pending` |

No blocker. Command namespaces: none.

## Interfaces

**liquers-core** (`src/recipes.rs`, `src/environment_builder.rs`):

```rust
// expand_recipes: refuse an entry equal to an earlier one (type and arguments), not a repeated type.
// resolve_recipes and EnvironmentBuilder::build's appended entries: a member is named by its type
// when the type occurs once in the chain, else "type#position"; an appended entry is skipped only
// when an identical entry is already a member.
impl Recipe {
    /// The recipe's plan for `key` with dependency analysis, as DefaultRecipeProvider builds it
    /// (today the private `create_plan_with_init_metadata`, which becomes a call to this).
    pub async fn plan_with_dependencies<E: Environment>(&self, envref: EnvRef<E>, key: Option<&Key>)
        -> Result<Plan, Error>;
}
```

**liquers-py** (all async methods call Python through `pycall::call`: a `def` on `spawn_blocking`,
an `async def` on the loop pool; `create` is synchronous by the trait and runs during `build()`):

```rust
struct RecipeMethods { recipe_opt: PyMethod, assets_with_recipes: Option<PyMethod>,
    contains: Option<PyMethod>, directory_changed: Option<PyMethod> }
pub(crate) struct PyRecipeProvider { name: String, methods: Arc<RecipeMethods>,
    loops: Arc<LazyLoopPool>, shut: Arc<Shut>, _handle: LiveHandle }
#[async_trait] impl AsyncRecipeProvider<PyEnvironment> for PyRecipeProvider {
    async fn recipe_opt(&self, key: &Key, envref: EnvRef<PyEnvironment>) -> Result<Option<Recipe>, Error>;
        // lq.Recipe | str | None; cwd defaults to key.parent(); a str is Recipe::new(query, "", "")
    async fn recipe(&self, key: &Key, envref: EnvRef<PyEnvironment>) -> Result<Recipe, Error>;
    async fn recipe_plan(&self, key: &Key, envref: EnvRef<PyEnvironment>) -> Result<Plan, Error>;
    async fn assets_with_recipes(&self, folder: &Key, envref: EnvRef<PyEnvironment>)
        -> Result<Vec<ResourceName>, Error>;          // [] when absent (question 1)
    async fn has_recipes(&self, folder: &Key, envref: EnvRef<PyEnvironment>) -> Result<bool, Error>;
    async fn contains(&self, key: &Key, envref: EnvRef<PyEnvironment>) -> Result<bool, Error>;
        // the Python contains(), else membership in the listing
    async fn directory_changed(&self, dir: &Key);      // forwarded when present
    // can_make and get_asset_info: core defaults
}
pub(crate) struct PyRecipeProviderType { info: RecipeProviderTypeInfo, factory: Py<PyAny>,
    takes_kwargs: bool, _handle: LiveHandle }
pub(crate) struct PyRecipeProviderFactory { types: Arc<Vec<PyRecipeProviderType>>,
    loops: Arc<LazyLoopPool>, shut: Arc<Shut> }
impl RecipeProviderFactory<PyEnvironment> for PyRecipeProviderFactory {
    fn provider_types(&self) -> Vec<RecipeProviderTypeInfo>;
    fn create(&self, entry: &RecipeProviderEntry)
        -> Result<Arc<dyn AsyncRecipeProvider<PyEnvironment>>, Error>;
}
#[pyclass(frozen, name = "RecipeProviders", module = "liquers")]
pub struct RecipeProviders { cell: Weak<EnvCell> }
// recipe_opt, recipe, recipe_plan (an lq.Plan), assets_with_recipes, contains, can_make, each with
// an `_async` twin; after shutdown every call raises.
// Environment: register_recipe_provider_factory(type, factory, *, label="", doc="", arguments=None);
//              #[getter] recipes -> RecipeProviders
// lq.Recipe: keyword constructor (query, title="", description="", *, cwd=None, volatile=False,
//            expires=None); get/set properties cwd, volatile, expires (an lq.Expires)
```

Provider methods may be `def` or `async def` (detected once), receive `lq.Key`, and a listing
returns `lq.Key`s or names (an entry outside `folder` is refused). The factory is any non-`async`
callable, called with the entry's arguments as keywords; `liquers/_stores.py` `_declare` derives the
declaration as for store factories (`type` is a refused parameter name). Registration refuses a
name the kind's factory declares (`LibKind::recipe_provider_factory().provider_types()`), another
Python type, or a keyword (`default`, `trivial`, `none`, `no_recipes`), because
`with_recipe_provider_factory` is consulted first and would otherwise override it.

## Integration Points

- `liquers-core/src/recipes.rs`: `expand_recipes`, `resolve_recipes`, `Recipe::plan_with_dependencies`,
  `create_plan_with_init_metadata`; `liquers-core/src/environment_builder.rs`: appended-entry
  de-duplication in `build`.
- `liquers-py/src/recipe_provider.rs` (new); `src/environment.rs` (`Declarations.recipe_types`,
  passed to every build; registration marks `Pending`; the `BUILDING` guard of `liquers-py-store`
  Phase 2 covers the factory call; `shutdown()` drops the types after the guard);
  `src/recipes.rs` (`lq.Recipe` properties); `src/lib.rs`; `python/liquers/__init__.py`.
- Tests asserting member names (`recipes.rs` `named_chain_reports_member_names`,
  `liquers-core/tests/recipe_provider_chain_config.rs`, `liquers-lib/tests/recipe_provider_chain_config.rs`)
  keep passing, since unique types keep bare names; the "listed more than once" test becomes two
  identical entries, and a new one lists one type with two argument sets.

## Error Handling

| Case | Error |
|---|---|
| A provider method raises | `pyerr_to_error`: its `LiquersError` type, or `ExecutionError` with the traceback |
| `recipe_opt` returns another type | `Error::execution_error` naming the type |
| Missing `recipe_opt`, taken name, bad arguments, factory raised, environment used during the build | `Error::configuration_error` naming the entry (`create_at` adds its position) |
| An identical entry listed twice | `Error::configuration_error` from `expand_recipes` |
| Two members claim one key | the chain's existing `configuration_error`, naming `type#position` when the type repeats |
| `env.recipes` after `shutdown()` | `shut_down_error()` |

## Relevant Commands

None; no namespace.

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `specs/reference/PYTHON_INTEGRATION.md` | reference | new §Recipe providers; §3 recipe chain paragraph |
| `specs/guides/PYTHON_GUIDE.md` | guide | §6 "Recipe providers", with the listing contract and the `liquers-validate` limitation |
| `specs/reference/ENVIRONMENT_CONFIG.md` | reference | §The recipe provider chain: duplicate rule, member names, integration types; `{type: python, …}` example replaced |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | guide | §RECIPE factory model; §6 Python |
| `liquers-py/README.md` | readme | `RECIPE` row |
| docstrings | API | every new class and method (`test_DOCS_docstrings.py`) |

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/recipes.rs`, `environment_builder.rs`; liquers-py `recipe_provider.rs`, `recipes.rs`, `environment.rs` |
| Crates and workflows affected | `liquers-core` (every environment's chain), `liquers-py`; CI for both |
| Existing tests likely to change | the core "listed more than once" test |
| New validation | `test_RECIPE_*.py`, core tests for repeated types |
| Compatibility / data / concurrency / performance / security | A document without repeated types behaves as before. Performance: every Python provider is asked for every key, on a blocking thread with the GIL (documented: answer `None` fast) |
| Recovery | Revert |
| Certainty and open questions | High |
