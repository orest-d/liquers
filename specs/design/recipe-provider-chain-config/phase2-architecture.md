# Phase 2: Solution & Architecture — recipe-provider-chain-config

## Overview

`EnvironmentConfig.recipes` becomes a `RecipesConfig`: the kind's default, or an explicit list of
`RecipeProviderEntry` values (a bare name or `{type: …, <arguments>}`). One function,
`resolve_recipes`, turns it into a named `RecipeProviderChain` through a `RecipeProviderFactory`
chain that the environment kind seeds and the building crate extends. The chain asks every member
for a key and refuses a key two members define. `RecipeProviderChoice` and every existing setter
stay, routed through the kind.

Rejected: a `with_lib_config` helper (every configured path must remember it); a `Chain` variant
of `RecipeProviderChoice` (an enum cannot name another crate's provider); a recipe factory as a
second `with_config` argument (breaks every caller).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `RECIPE-PROVIDER-CHAIN-NOT-CONFIGURABLE` | draft | P2 | Source | — | No | Closed by this design |
| `CONFIGURED-RECIPE-CHOICE-DROPS-MANIFEST-PROVIDER`, `PY-RECIPE-PROVIDER-NOT-CONFIGURABLE` | draft | P3 | Absorbed sources | — | No | Closed by this design (AC-3, AC-9) |
| `VALIDATE-CANNOT-SEE-NON-STANDARD-RECIPE-PROVIDERS`, `LIB-RECIPE-PROVIDERS-HAVE-NO-VALIDATION-CLI` | draft | P3 | Both need "the environment's chain"; `resolve_recipes` defines it | No | No | Note in their designs at Phase 5 |
| `WEB-EXPLICIT-ENVIRONMENT-CONFIGURES-THE-SINGLETON` | draft | P2 | A wasm test would have to build through the singleton | No | No | The AC-9 web path is tested natively instead (Phase 3): `WebEnvironment` is `DefaultEnvironment<Value>` built with `with_default_recipe_provider()` |
| `LANGUAGE-STORE-TYPE-NOT-DEFINABLE` | draft | P2 | Store-side twin of "an integration supplies an entry type" | No | No | Same factory shape, no dependency |

`RECIPE-ONLY-FOLDER-NAMES-…` and `LANGUAGE-INTEGRATIONS-REBUILD-…` are unaffected. Command
namespaces: none.

## Interfaces

All synchronous: resolution runs at build time and touches no store. `liquers-core/src/recipes.rs`:

```rust
/// One `recipes:` entry; `manifest` and `{type: manifest}` are equal (AC-12). Serializes as the
/// bare name when it has no arguments.
#[derive(Debug, Clone, PartialEq, Eq)]           // hand-written Serialize/Deserialize
pub struct RecipeProviderEntry {
    pub provider_type: String,
    pub arguments: BTreeMap<String, serde_json::Value>,
}
impl RecipeProviderEntry { pub fn named(provider_type: &str) -> Self; }

/// The `recipes:` value. Deserializes from: absent / `default` → `KindDefault`;
/// `trivial` | `none` | `no_recipes` → `Chain([])`; a list → `Chain`. An entry of type `default`
/// splices the kind's list (Phase 1 question 1); a list entry `trivial` / `none` is refused.
/// Serializes canonically: `KindDefault` omits the key, `Chain` writes a list (`[]` included).
#[derive(Debug, Clone, PartialEq, Eq, Default)]  // hand-written serde, no `untagged`
pub enum RecipesConfig { #[default] KindDefault, Chain(Vec<RecipeProviderEntry>) }
impl From<RecipeProviderChoice> for RecipesConfig;   // Default → KindDefault, Trivial → Chain([])

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeProviderTypeInfo {
    pub provider_type: String, pub label: String, pub doc: String,
    pub arguments: Vec<crate::store_factory::StoreArgumentInfo>,      // reused, same crate
    pub availability: crate::store_factory::StoreTypeAvailability,
}
impl RecipeProviderTypeInfo { pub fn unavailability_error(&self, position: usize) -> Option<Error>; }

/// No `Send`/`Sync` bound, as `StoreFactory`: it lives in a builder.
pub trait RecipeProviderFactory<E: Environment> {
    fn provider_types(&self) -> Vec<RecipeProviderTypeInfo>;
    fn create(&self, entry: &RecipeProviderEntry) -> Result<Arc<dyn AsyncRecipeProvider<E>>, Error>;
}
pub struct RecipeProviderTypeMap<E: Environment>;          // infos + boxed constructors
pub struct ChainedRecipeProviderFactory<E: Environment>;   // `new`, `chain(Box<dyn …>)`; first declarer wins
pub fn core_recipe_provider_factory<E: Environment>() -> RecipeProviderTypeMap<E>; // `recipes_yaml`

/// Expands `KindDefault` and `default` entries to `default_list`, refuses duplicate entry names,
/// creates each entry through `factory`. The single definition of "the environment's chain".
pub fn resolve_recipes<E: Environment>(
    recipes: &RecipesConfig, default_list: &[RecipeProviderEntry],
    factory: &dyn RecipeProviderFactory<E>,
) -> Result<RecipeProviderChain<E>, Error>;

impl<E: Environment> RecipeProviderChain<E> {
    pub fn named(providers: Vec<(String, Arc<dyn AsyncRecipeProvider<E>>)>) -> Self;
    pub fn names(&self) -> Vec<String>;          // `new` names members `#0`, `#1`, …
}
```

**Chain semantics (AC-1, AC-7, AC-11, Phase 1 question 5).** The per-key methods (`recipe_opt`,
`recipe`, `recipe_plan`, `get_asset_info`, `can_make`) ask every member. An error from any member
fails the lookup, in every order. Two members answering `Some` fail with a configuration error
naming both. One answer is used. A private `owner_of(key) -> Option<(usize, Recipe)>` replaces
`provider_index_for`, so a resolved key is not looked up twice. `contains`, `has_recipes`,
`assets_with_recipes` and `directory_changed` keep their behaviour, and the doc table is corrected:
`contains` asks each member's `contains`.

`liquers-core/src/environment_builder.rs`:

```rust
pub trait AssetManagerKind {
    fn default_recipe_provider<E: Environment>() -> Arc<dyn AsyncRecipeProvider<E>>; // unchanged: unconfigured
    /// The list a document without `recipes:` means. Default: `[recipes_yaml]`.
    fn default_recipes() -> Vec<RecipeProviderEntry>;
    /// The entry types this kind knows. Default: a chain of `core_recipe_provider_factory()`.
    fn recipe_provider_factory<E: Environment>() -> ChainedRecipeProviderFactory<E>;
}
impl<V, P, K> EnvironmentBuilder<V, P, K> {
    pub fn with_recipes(self, recipes: RecipesConfig) -> Self;
    /// Chained before the kind's factory, so an integration may override a name, as stores can.
    pub fn with_recipe_provider_factory(
        self, factory: Box<dyn RecipeProviderFactory<GenericEnvironment<V, P, K>>>) -> Self;
    /// Appends an entry after the resolved list; skipped if an entry of that name is present.
    pub fn with_appended_recipe_entry(self, entry: RecipeProviderEntry) -> Self;
}
```

The private `recipe_provider: Option<Arc<…>>` becomes `base: RecipeBase` (`Unset`,
`Provider(Arc<…>)`, `Config(RecipesConfig)`): the last of `with_recipe_provider`,
`with_recipe_provider_choice` (= `with_recipes(choice.into())`) and `with_recipes` wins. `build()`
resolves `Config` with `resolve_recipes(…, &K::default_recipes(), &factories)`. It puts the base,
the appended entries and the appended providers (unchanged API, named `#n`) into one `named` chain.
The duplicate check covers base and appended entry names together.

`liquers-core/src/environment_config.rs`: `pub recipes: RecipesConfig`. `with_config` calls
`with_recipes`. A deserialization error becomes `Error::parse_error` in the existing
`from_yaml` / `from_json` wrappers.

`liquers-core/src/context.rs` `GenericEnvironment`: new
`try_with_recipes(&mut self, RecipesConfig) -> Result<&mut Self, Error>`.
`with_recipe_provider_choice` and its two wrappers stay infallible: `Trivial` installs an empty
chain, and `Default` resolves the kind's default list. A failure (a kind whose own default is
unbuildable) installs a provider that fails every lookup with that error, not a panic. A test per
kind asserts the default resolves. `GenericEnvironment::with_appended_recipe_provider` is
unchanged.

**Named objects (Phase 1 question 3).** An integration supplies an entry type whose constructor
looks objects up in a registry it owns (`{type: python, object: notes}`). The registry and the
`python` type are liquers-py M4. This design only provides `with_recipe_provider_factory`.

`liquers-lib/src/environment.rs` `LibKind`: `default_recipes()` is `[recipes_yaml, manifest]` with
`records`. Its factory adds `manifest`, declared `unavailable("enable the records feature of
liquers-lib")` without it. `RecordsRecipeProvider::with_records_recipe_provider` becomes
`with_appended_recipe_entry(manifest)`, so it can no longer add a second manifest provider. The
`LibKind` and chain doc comments that recommend it after a configured base are rewritten.

`liquers-records/src/provider.rs`: `check_no_recipes_yaml_collision` is removed, because the chain
detects every clash.

`liquers-py/src/environment.rs` `EnvCell`: the refusal goes, and `configured_builder` calls
`with_recipes`. `liquers-web`: no code change beyond a comment. Its `with_default_recipe_provider()` now resolves the
kind's list.

## Integration Points

Per crate as in Interfaces: core `recipes.rs`, `environment_builder.rs`, `environment_config.rs`,
`context.rs`; lib `environment.rs`; records `provider.rs`; py `environment.rs`; web: the comment at `environment.rs:106` only, its setter path tested natively.

## Error Handling

| Case | Error |
|---|---|
| Unknown keyword, malformed entry, list entry `trivial` | `de::Error::custom` naming the accepted keywords → `Error::parse_error` |
| Entry type no factory declares | `Error::configuration_error` naming the entry, its position and the known types |
| Entry type declared unavailable | `RecipeProviderTypeInfo::unavailability_error`: `configuration_error` naming entry, position and reason (AC-6) |
| Duplicate entry name | `Error::configuration_error` at build |
| A constructor rejects its arguments | its error, prefixed with the entry name and position |
| Two members define one key | `Error::configuration_error(…).with_key(key)` at lookup, naming both |
| A member fails (malformed `recipes.yaml` or `*.manifest.yaml`, `listdir` error) | that error, whichever member owns the key |

## Relevant Commands

None.

## Documentation Architecture

`affects_docs` as in `DESIGN.md`, each updated with a History row and `reviewed:`:

- `ENVIRONMENT_CONFIG.md`: the `recipes` grammar, both defaults per kind, the chain rule.
- `RECIPES_AND_PLANS.md` §Composing providers: chain contract, factories.
- `PYTHON_INTEGRATION.md` §3 and `PYTHON_GUIDE.md` §4: `recipes` accepted.
- `JAVASCRIPT_INTEGRATION.md`: manifests kept.
- `ENVIRONMENT_CONSTRUCTION_GUIDE.md`: configuring and extending the chain.
- `LANGUAGE-INTEGRATION_GUIDE.md` §RECIPE: supplying an entry type.

Also `liquers-py/README.md` (`ENVIRON` row), `liquers-web/README.md`, and `specs/README.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files and tests | core `recipes.rs`, `environment_builder.rs` (incl. test at `:809-815`), `environment_config.rs` (`:147-230`), `context.rs`; lib `environment.rs`; records `provider.rs` (collision test moves to the chain); py `environment.rs` (`:114`, `:932`); feature matrix and wasm32 |
| Compatibility | `EnvironmentConfig.recipes` changes type: breaks code that reads it, but not documents. Intended behaviour changes: a `liquers-lib` + `records` build that applies a document, or calls `with_default_recipe_provider()`, gains manifests (`liquers-agent` too in a workspace build, where features unify). A broken member now fails keys it does not own (AC-11) |
| Performance | Every member is asked per keyed lookup. The removed collision check was an *uncached* `recipes.yaml` parse per manifest match; the added work is the manifest provider's cached name lookup, and a metadata read for generated-looking names, on `recipes.yaml` keys. Net about even. Measure before adding a "never overlaps" opt-out |
| Recovery and certainty | Revert per crate; old documents parse throughout. High certainty, except the cost of the strict error rule (Phase 2 gate question) |

**Decided at the Phase 2 gate (Orest, 2026-10-30, `proceed all`):** an error from any member fails
the lookup, owner or not; recipe providers should rarely fail, and a failure is substantial.

**Migration hazard:** code that appends a raw `ManifestRecipeProvider` with
`with_appended_recipe_provider` after a default that already holds `manifest` makes every chunk a
two-owner error. `with_records_recipe_provider` avoids it; the hazard is documented.
