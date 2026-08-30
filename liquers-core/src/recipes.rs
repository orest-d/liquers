//! Recipe definitions and recipe-provider lookup.
//!
//! A [`Recipe`] augments a query in multiple ways:
//! - It allows to specify additional human facing data such as title and description, which would be difficult in a compact query string.
//! - Recipes can specify whether the asset is volatile or when it expires.
//! - Recipes can place the query in a logical hierarchycal structure similar to a filesystem.
//!   This allows to organize queries, give them names, lazyly execute them, cache the results (via AssetManager) and access the results similarly as files.
//! - Recipes can override query parameters and pass this way more complex arguments to the query, e.g. longer texts or strings.
//! - Recipes can be used internally to represent complex asset queries, e.g. to implement web APIs with arguments specified in JSON format.
//!
//! Converting a recipe with [`Recipe::to_plan`] builds a [`Plan`] but does not execute it.
//!
//! [`AsyncRecipeProvider`] is a service that resolves recipes registered for logical keys.
//! In other words, it says what recipes are available in each directory.
//! By default, it reads a [`RecipeList`] from `<directory>/recipes.yaml`; filenames derived from recipe queries
//! identify the assets in that directory. A recipe looked up by key must use
//! [`Recipe::to_plan_for_key`], because keyed assets are globally shared and cannot receive a
//! per-evaluation payload.
//! Custom recipe providers can be implemented to provide recipes from different sources or automatic default recipes.
//!
//! The [`RecipeList`] is a collection of recipes, which conviniently can be loaded from a YAML file.
//! The **resipes.yaml** files (used by the default recipe provider) are meant to be human-readable and editable
//! and are typically written by hand, specifying what needs to be calculated and how.
//!
//! Recipe conversion is only the synchronous planning phase. Environment-backed dependency
//! volatility and expiration are incorporated later by interpreter finalization before execution.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::{
    command_metadata::CommandMetadataRegistry,
    context::{EnvRef, Environment},
    error::Error,
    expiration::Expires,
    metadata::{AssetInfo, Status},
    parse::{parse_key, parse_query},
    plan::{
        has_expirable_dependencies, has_volatile_dependencies, Plan, PlanBuilder, Step,
        VolatilitySource,
    },
    query::{Key, Query, ResourceName},
};

/// Declarative instructions and metadata for producing an asset.
///
/// [`Recipe::new`] validates and canonicalizes query text. Public fields and Serde
/// deserialization intentionally do not validate eagerly, so methods that parse `query`, `cwd`,
/// or links remain fallible.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Recipe {
    /// Encoded query compiled into a plan.
    /// Note: query can be invalid - e.g. because it is unfinished or by user mistake.
    /// Since queries may often contain such mistakes,
    /// it is important that an invalid query does not invalidate other valid recipes.
    #[serde(skip_serializing_if = "String::is_empty")]
    #[serde(default)]
    pub query: String,
    /// Human-facing recipe title.
    /// The size is not enforced, but this should ideally be a short string, preferably a single line
    #[serde(skip_serializing_if = "String::is_empty")]
    #[serde(default)]
    pub title: String,
    /// Human-facing recipe description.
    /// This should ideally be a short and concise description/documentation of the asset.
    /// The recommended size is be multiple lines - a paragraph or two.
    #[serde(skip_serializing_if = "String::is_empty")]
    #[serde(default)]
    pub description: String,
    /// JSON-value overrides indexed by command parameter name.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(default)]
    pub arguments: HashMap<String, Value>,
    /// Encoded query-link overrides indexed by command parameter name.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(default)]
    pub links: HashMap<String, String>,
    /// Logical working key used to resolve relative keys and links.
    /// This should not be specified in recipes.yaml - it is set by the recipe provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub cwd: Option<String>,
    /// Marks the whole plan volatile, in addition to volatility inferred from it.
    ///
    /// A volatile recipe is volatile **from its first action**, not merely in its result:
    /// nothing in it is cached, and no predecessor boundary is cut out of it. A boundary is a
    /// cache entry, and a plan declared volatile is one whose intermediates must not be cached.
    ///
    /// The alternative reading — volatility applying only to the last action — produces an asset
    /// that is dutifully recomputed and restores the same cached prefix every time: volatile in
    /// name, fixed in value. Measured before this rule existed, the prefix of such a recipe ran
    /// once across two evaluations instead of twice.
    ///
    /// This flag carries no position, so it cannot mark *where* a non-volatile part of a plan
    /// ends. The positional instrument is the `v` instruction. Use `volatile: true` to say
    /// *this recipe is volatile*, which is the case it exists for: covering impurity a command
    /// did not declare.
    ///
    /// Recorded on the plan as [`crate::plan::VolatilitySource::Declared`].
    #[serde(skip_serializing_if = "is_false")]
    #[serde(default = "false_default")]
    pub volatile: bool,
    /// Provider-supplied indication that validation found a circular dependency.
    /// This serves as an early warning that the recipe may not be valid.
    /// `Recipe::to_plan` does not recompute or enforce this field.
    #[serde(default)]
    pub has_circular_dependencies: bool,
    /// Key reported by the provider when `has_circular_dependencies` is true.
    /// This is used to identify the circular dependency.
    #[serde(default)]
    pub circular_dependency_key: Option<Key>,
    /// Recipe-level expiration, combined into the plan's own expiration by [`Self::to_plan`].
    ///
    /// This bounds how long the resulting asset stays valid. It says nothing about the purity of
    /// the computation, so — unlike [`Self::volatile`] — a finite expiration does **not** stop a
    /// predecessor boundary being cut: a pure prefix behind it is still soundly cached, and its
    /// own expiration comes from its own dependencies.
    ///
    /// An expiration that is itself volatile (`Expires::Immediately`, or a combination
    /// containing one) is the exception, and contributes
    /// [`crate::plan::VolatilitySource::Declared`] like `volatile: true`.
    #[serde(default)]
    pub expires: Expires,
}

fn is_false(b: &bool) -> bool {
    *b == false
}

fn false_default() -> bool {
    false
}

impl Recipe {
    /// Creates a recipe after parsing and encoding `query`.
    pub fn new(query: String, title: String, description: String) -> Result<Recipe, Error> {
        Ok(Recipe {
            query: parse_query(&query)?.encode(),
            title,
            description,
            arguments: HashMap::new(),
            links: HashMap::new(),
            cwd: None,
            volatile: false,
            has_circular_dependencies: false,
            circular_dependency_key: None,
            expires: Expires::Never,
        })
    }

    /// Adds or replaces a named JSON-value override.
    pub fn with_argument(mut self, name: String, value: Value) -> Self {
        self.arguments.insert(name, value);
        self
    }

    /// Adds or replaces a named encoded query-link override.
    ///
    /// The link is parsed later by [`Self::to_plan`].
    pub fn with_link(mut self, name: String, value: String) -> Self {
        self.links.insert(name, value);
        self
    }

    /// Parses the stored query text.
    pub fn get_query(&self) -> Result<Query, Error> {
        parse_query(&self.query)
    }

    /// Returns the query filename, if present.
    ///
    /// Provider-backed recipes normally need a filename so they can be addressed in a directory;
    /// ad-hoc recipes do not.
    pub fn filename(&self) -> Result<Option<ResourceName>, Error> {
        Ok(self.get_query()?.filename())
    }

    /// Returns the query filename extension, if present.
    pub fn extension(&self) -> Result<Option<String>, Error> {
        Ok(self.get_query()?.extension())
    }

    /// Returns the filename extension used as a data format, or `"bin"` when absent.
    pub fn data_format(&self) -> Result<String, Error> {
        if let Some(extension) = self.extension()? {
            return Ok(extension);
        }
        Ok("bin".to_string())
    }

    //TODO: specify icons in recipes.yaml?
    /// Returns the Unicode icon associated with the query filename extension.
    ///
    /// Invalid queries and filenames without extensions use the default icon.
    pub fn unicode_icon(&self) -> String {
        if let Ok(Some(extension)) = self.extension() {
            crate::icons::file_extension_to_unicode_icon(&extension).to_owned()
        } else {
            crate::icons::DEFAULT_ICON.to_owned()
        }
    }

    /// Returns whether the recipe contains any value or link overrides.
    pub fn has_arguments(&self) -> bool {
        !self.arguments.is_empty() || !self.links.is_empty()
    }
    /// Returns whether the recipe contains a valid query and no overrides.
    pub fn is_pure_query(&self) -> bool {
        !self.has_arguments() && self.get_query().is_ok()
    }

    // TODO: Non-persistent recipes. Aliases definitely would benefit from this.
    /// Returns the logical key represented by a pure key query.
    /// A recipe returning a `Some` key is effectively an alias for another keyed asset.
    /// Recipes with overrides are not pure and return `None`. When `cwd` is present, relative keys
    /// are converted to absolute logical form.
    pub fn key(&self) -> Result<Option<Key>, Error> {
        let query = self.get_query()?;
        if !self.has_arguments() {
            if let Some(key) = query.key() {
                if let Some(cwd) = &self.cwd {
                    let cwd = parse_key(cwd)?;
                    return Ok(Some(key.to_absolute(&cwd)));
                } else {
                    return Ok(Some(key));
                }
            }
        }
        Ok(None)
    }

    /// Builds a preliminary plan and applies named overrides to its last action.
    ///
    /// Placeholders are allowed during the initial build. Every override name must resolve on the
    /// last action or this returns an error; earlier actions are deliberately not searched. Link
    /// text is parsed here. When `cwd` is present, the resulting plan records it as one leading
    /// executable [`Step::SetCwd`] and one planning [`Step::Info`] without resolving any
    /// query-derived operand. This method neither finalizes dependencies nor executes the plan.
    ///
    /// It also folds two facts onto the plan that nothing downstream could recover, because
    /// neither appears in the recipe's query:
    ///
    /// - [`Plan::prologue_steps`], counting the `SetCwd` prefix. Inserted at index 0 *after*
    ///   building, it shifts every step the builder emitted, so freezing has to advance over it
    ///   before resolving the recorded predecessor — and a boundary query frozen one CWD short
    ///   silently loses its folder.
    /// - The recipe's own [`Self::volatile`] and [`Self::expires`], as
    ///   [`Plan::is_volatile`], [`Plan::expires`] and a
    ///   [`crate::plan::VolatilitySource::Declared`] source. Without this a recipe preview
    ///   under-reports both, and no consumer can ask the plan whether it is volatile.
    pub fn to_plan(&self, cmr: &CommandMetadataRegistry) -> Result<Plan, Error> {
        let query = self.get_query()?;
        let mut planbuilder = PlanBuilder::new(query.clone(), cmr).with_placeholders_allowed();
        let mut plan = planbuilder.build()?;

        for (name, value) in &self.arguments {
            if !(plan.override_value(name, value.clone())) {
                return Err(Error::general_error(format!(
                    "Argument {} not found in last action",
                    name
                ))
                .with_query(&query));
            }
        }
        for (name, link) in &self.links {
            if !(plan.override_link(name, parse_query(link)?)) {
                return Err(Error::general_error(format!(
                    "Link {} not found in last action",
                    name
                ))
                .with_query(&query));
            }
        }

        // A recipe's own declarations are not in its query, so nothing downstream can recover
        // them from the plan: fold them in here, where the recipe is in hand.
        if self.volatile || self.expires.is_volatile() {
            plan.is_volatile = true;
            // Whole-plan, not positional — see `VolatilitySource`. This is what stops a
            // predecessor boundary being cut out of a plan the author declared volatile.
            plan.upgrade_volatility_source(VolatilitySource::Declared);
        }
        if !self.expires.is_never() {
            // What this field's own documentation already promised: "Recipe-level expiration
            // combined with finalized plan expiration."
            plan.expires = plan.expires.clone().combine(self.expires.clone());
        }

        if let Some(cwd) = self.get_cwd()? {
            plan.steps.insert(0, Step::SetCwd(cwd.clone()));
            // The prefix shifts every step the builder emitted, so the recorded predecessor range
            // has to move with it. Leaving it stale makes `cut_predecessor` split in the wrong
            // place and keep the predecessor's own action, which then runs twice.
            if plan.predecessor.is_some() {
                plan.predecessor_steps += 1;
            }
            // Not emitted by the builder for the query, so freezing must advance over it before
            // resolving the recorded predecessor.
            plan.prologue_steps += 1;
            plan.init_info(format!("Recipe set CWD to '{}'", cwd.encode()));
        }

        plan.check_consistent()?;
        Ok(plan)
    }

    /// Builds the plan for a recipe that is *stored at* `key`, rejecting one that requires an
    /// evaluation payload.
    ///
    /// Keys are a payload boundary: a key names a single shared asset, while a payload is
    /// supplied per evaluation, so a keyed recipe requiring one would need a global payload —
    /// which this design does not provide. Rejecting here surfaces the problem against the
    /// recipe that caused it, rather than as an obscure injection failure during evaluation.
    ///
    /// Note this cannot be folded into [`Self::to_plan`]: [`Self::key`] reports the key of the
    /// recipe's *query*, which is unrelated to the key the recipe is registered under. Only the
    /// caller that looked the recipe up knows that key.
    pub fn to_plan_for_key(&self, cmr: &CommandMetadataRegistry, key: &Key) -> Result<Plan, Error> {
        let plan = self.to_plan(cmr)?;
        if plan.payload_required.is_required() {
            return Err(Error::general_error(format!(
                "Recipe for key '{}' requires an evaluation payload, but keyed recipes cannot \
                 receive one: a key identifies a single shared asset, while a payload is \
                 supplied per evaluation. Remove the 'payload: required' commands from this \
                 recipe, or evaluate the query directly with EnvRef::evaluate_immediately \
                 instead of storing it as a recipe.",
                key
            ))
            .with_key(key));
        }
        Ok(plan)
    }

    /// Parses the logical working key, if set.
    ///
    /// Providers normally set it to the directory containing `recipes.yaml`. It is used to
    /// resolve relative keys in the recipe query and links.
    pub fn get_cwd(&self) -> Result<Option<Key>, Error> {
        if let Some(cwd) = &self.cwd {
            Ok(Some(parse_key(cwd)?))
        } else {
            Ok(None)
        }
    }

    /// Derives the logical storage key from `cwd` and the query filename.
    ///
    /// This is descriptive only and performs no write.
    pub fn store_to_key(&self) -> Result<Option<Key>, Error> {
        let filename = self.filename()?;
        let cwd = self.get_cwd()?;
        if let (Some(filename), Some(cwd)) = (filename, cwd) {
            Ok(Some(cwd.join(filename.name)))
        } else {
            Ok(None)
        }
    }

    /// Creates recipe-status asset information without evaluating or loading an asset.
    pub fn get_asset_info(&self) -> Result<AssetInfo, Error> {
        let mut asset_info = AssetInfo::new();
        asset_info.key = None; // Key is not known to the recipe
        if self.is_pure_query() {
            asset_info.query = if let Ok(query) = self.get_query() {
                Some(query)
            } else {
                None
            };
        }
        asset_info.message = "Recipe available".to_string();
        asset_info.title = self.title.clone();
        asset_info.description = self.description.clone();
        asset_info.filename = self.filename()?.map(|f| f.name);
        asset_info.data_format = Some(self.data_format()?);
        asset_info.is_error = false;
        asset_info.is_dir = false;
        asset_info.status = Status::Recipe;
        asset_info.unicode_icon = self.unicode_icon();
        Ok(asset_info)
    }
}

impl fmt::Display for Recipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match serde_yaml::to_string(self) {
            Ok(yaml) => write!(f, "{}", yaml),
            Err(e) => write!(f, "<Failed to serialize Recipe: {}>", e),
        }
    }
}

impl From<&Query> for Recipe {
    fn from(query: &Query) -> Self {
        Recipe {
            query: query.encode(),
            title: "Ad-hoc query".to_string(),
            description: "".to_string(),
            arguments: HashMap::new(),
            links: HashMap::new(),
            cwd: None,
            volatile: false,
            has_circular_dependencies: false,
            circular_dependency_key: None,
            expires: Expires::Never,
        }
    }
}

impl From<Query> for Recipe {
    fn from(query: Query) -> Self {
        Recipe {
            query: query.encode(),
            title: "Ad-hoc query".to_string(),
            description: "".to_string(),
            arguments: HashMap::new(),
            links: HashMap::new(),
            cwd: None,
            volatile: false,
            has_circular_dependencies: false,
            circular_dependency_key: None,
            expires: Expires::Never,
        }
    }
}

impl From<Key> for Recipe {
    fn from(key: Key) -> Self {
        Recipe {
            query: Query::from(key).encode(),
            title: "Ad-hoc key-query".to_string(),
            description: "".to_string(),
            arguments: HashMap::new(),
            links: HashMap::new(),
            cwd: None,
            volatile: false,
            has_circular_dependencies: false,
            circular_dependency_key: None,
            expires: Expires::Never,
        }
    }
}

impl From<&Key> for Recipe {
    fn from(key: &Key) -> Self {
        Recipe {
            query: Query::from(key).encode(),
            title: "Ad-hoc key-query".to_string(),
            description: "".to_string(),
            arguments: HashMap::new(),
            links: HashMap::new(),
            cwd: None,
            volatile: false,
            has_circular_dependencies: false,
            circular_dependency_key: None,
            expires: Expires::Never,
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
/// Asynchronous lookup and planning interface for recipes registered at logical keys.
/// A recipe provider can be understood as a logical overlay of recipes on top of a store.
/// Asset manager unites the store and recipe provider into a single interface.
/// The recipe provider parametrizes what recipes are available and how they are specified.
/// E.g., in case if you need automatically generated recipes or a custome file format for specifying the recipes,
/// this can be achieved by a custom recipe provider (implementing this trait).
///
/// Directory methods operate on the parent directory key, while `recipe`, `recipe_opt`, and
/// `recipe_plan` operate on a complete asset key including its filename. Implementations should
/// distinguish a missing recipe from provider failures: `recipe_opt` returns `Ok(None)` only for
/// absence, while I/O and parsing failures may still be errors.
pub trait AsyncRecipeProvider<E: Environment>:
    crate::maybe_send::MaybeSend + crate::maybe_send::MaybeSync
{
    /// Returns whether the directory represented by `key` has a recipe collection.
    async fn has_recipes(&self, key: &Key, envref: EnvRef<E>) -> Result<bool, Error>;
    /// Lists filenames of recipe-backed assets in the directory represented by `key`.
    async fn assets_with_recipes(
        &self,
        key: &Key,
        envref: EnvRef<E>,
    ) -> Result<Vec<ResourceName>, Error>;
    /// Builds a dependency-analyzed plan for the recipe registered at `key`.
    ///
    /// A missing recipe is an error. This convenience performs more analysis than
    /// [`Recipe::to_plan`] but is not a substitute for complete execution finalization.
    async fn recipe_plan(&self, key: &Key, envref: EnvRef<E>) -> Result<Plan, Error>;
    /// Returns the recipe registered at `key`, or an error when it is absent.
    async fn recipe(&self, key: &Key, envref: EnvRef<E>) -> Result<Recipe, Error>;
    /// Returns the recipe registered at `key`, or `None` when it is absent.
    ///
    /// Provider I/O, decoding, and validation failures remain errors.
    async fn recipe_opt(&self, key: &Key, envref: EnvRef<E>) -> Result<Option<Recipe>, Error>;
    /// Returns whether the complete asset `key` has a recipe.
    async fn contains(&self, key: &Key, envref: EnvRef<E>) -> Result<bool, Error> {
        if let Some(name) = key.filename() {
            let parent_key = key.parent();
            if self.has_recipes(&parent_key, envref.clone()).await? {
                let recipes = self
                    .assets_with_recipes(&parent_key, envref.clone())
                    .await?;
                return Ok(recipes.iter().any(|resourcename| resourcename == name));
            } else {
                return Ok(false);
            }
        } else {
            Ok(false)
        }
    }
    /// Describes the recipe registered at `key` and includes planning diagnostics.
    ///
    /// The returned [AssetInfo] may serve as a quick preview of the asset (recipe).
    /// This reports recipe availability; it does not prove that an evaluated or persisted asset
    /// exists at the key.
    async fn get_asset_info(&self, key: &Key, envref: EnvRef<E>) -> Result<AssetInfo, Error> {
        eprintln!("Getting asset info for recipe at key {}", key);
        let recipe = self.recipe(key, envref.clone()).await?;
        let mut asset_info = recipe.get_asset_info()?;
        match create_plan_with_init_metadata(&recipe, envref, Some(key)).await {
            Ok(plan) => {
                asset_info.is_volatile = plan.is_volatile;
                asset_info.expires = plan.expires;
                if let Some(error) = plan.error {
                    asset_info.is_error = true;
                    asset_info.message = error.message.clone();
                    asset_info.error_data = Some(error);
                } else if plan.init_steps.iter().any(|step| step.is_error()) {
                    asset_info.is_error = true;
                }
            }
            Err(error) => {
                asset_info.is_error = true;
                asset_info.message = error.message.clone();
                asset_info.error_data = Some(error);
            }
        }
        asset_info.key = Some(key.clone());
        Ok(asset_info)
    }
}

async fn create_plan_with_init_metadata<E: Environment>(
    // TODO: missleading name, use conventioanl plan building functionality
    recipe: &Recipe,
    envref: EnvRef<E>,
    key: Option<&Key>,
) -> Result<Plan, Error> {
    let cmr = envref.get_command_metadata_registry();
    let mut plan = match key {
        Some(key) => recipe.to_plan_for_key(cmr, key)?,
        None => recipe.to_plan(cmr)?,
    };
    let _ = has_volatile_dependencies(envref.clone(), &mut plan, None).await; // TODO: looks suspicious, this should be done in plan building or checking
    if plan.error.is_none() {
        let _ = has_expirable_dependencies(envref, &mut plan).await; // TODO: looks suspicious, this should be done in plan building or checking
    }
    Ok(plan)
}

/// Provider used when an environment has no configured recipes.
///
/// Directory and optional lookups are empty; required recipe and plan lookups return errors.
pub struct TrivialRecipeProvider;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<E: Environment> AsyncRecipeProvider<E> for TrivialRecipeProvider {
    async fn assets_with_recipes(
        &self,
        _key: &Key,
        _envref: EnvRef<E>,
    ) -> Result<Vec<ResourceName>, Error> {
        Ok(Vec::new())
    }

    async fn recipe_plan(&self, key: &Key, _envref: EnvRef<E>) -> Result<Plan, Error> {
        return Err(Error::general_error(format!(
            "No recipe plans defined by the trivial recipe provider; key '{}'",
            key
        ))
        .with_key(key));
    }

    async fn recipe(&self, key: &Key, _envref: EnvRef<E>) -> Result<Recipe, Error> {
        return Err(Error::general_error(format!(
            "No recipes defined by the trivial recipe provider; key '{}'",
            key
        ))
        .with_key(key));
    }

    async fn recipe_opt(&self, _key: &Key, _envref: EnvRef<E>) -> Result<Option<Recipe>, Error> {
        Ok(None)
    }

    async fn has_recipes(&self, _key: &Key, _envref: EnvRef<E>) -> Result<bool, Error> {
        Ok(false)
    }
}

/// Store-backed provider using one `recipes.yaml` file per logical directory.
///
/// Each file deserializes as [`RecipeList`]. Recipe query filenames identify the assets exposed in
/// that directory, and the directory key is installed as each recipe's working key.
pub struct DefaultRecipeProvider;

impl DefaultRecipeProvider {
    /// Loads and parses `<key>/recipes.yaml`, then assigns `key` as every recipe's `cwd`.
    ///
    /// The current implementation maps every store read failure—not only a missing file—to an
    /// empty recipe list. Malformed YAML remains an error. [`RecipeList::set_cwd`] may partially
    /// mutate a list before rejecting a recipe that already specifies `cwd`.
    pub async fn get_recipes<E: Environment>(
        &self,
        key: &Key,
        envref: EnvRef<E>,
    ) -> Result<RecipeList, Error> {
        let mut recipes: RecipeList = envref
            .get_async_store()
            .get_bytes(&key.join("recipes.yaml"))
            .await
            .map_or(Ok(RecipeList::new()), |bytes| {
                serde_yaml::from_slice(&bytes)
                    .map_err(|e| Error::general_error(format!("Error parsing recipes: {}", e)))
            })?;
        recipes.set_cwd(key.encode()).map_err(|e| e.with_key(key))?;
        Ok(recipes)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<E: Environment> AsyncRecipeProvider<E> for DefaultRecipeProvider {
    async fn assets_with_recipes(
        &self,
        key: &Key,
        envref: EnvRef<E>,
    ) -> Result<Vec<ResourceName>, Error> {
        if self.has_recipes(key, envref.clone()).await? {
            let recipes = self.get_recipes(key, envref.clone()).await?;
            let mut assets = Vec::new();
            for recipe in recipes.recipes {
                if let Ok(Some(filename)) = recipe.filename() {
                    assets.push(filename);
                }
            }
            Ok(assets)
        } else {
            Ok(Vec::new())
        }
    }

    // TODO: Not used at the moment - consider removing
    /// Fetches the keyed recipe and builds its dependency-analyzed plan.
    async fn recipe_plan(&self, key: &Key, envref: EnvRef<E>) -> Result<Plan, Error> {
        if let Some(filename) = key.filename() {
            let recipes = self.get_recipes(&key.parent(), envref.clone()).await?;
            let recipe = recipes.get(&filename.name).ok_or(
                Error::general_error(format!("No recipe found for key {} (recipe plan)", key))
                    .with_key(key),
            )?;
            create_plan_with_init_metadata(recipe, envref, Some(key))
                .await
                .map_err(|e| e.with_key(key))
        } else {
            return Err(
                Error::general_error(format!("No filename in key '{}'", key)).with_key(key),
            );
        }
    }

    async fn recipe(&self, key: &Key, envref: EnvRef<E>) -> Result<Recipe, Error> {
        if let Some(filename) = key.filename() {
            let recipes = self.get_recipes(&key.parent(), envref).await?;
            recipes.get(&filename.name).map_or(
                Err(Error::general_error(format!("No recipe found for key {}", key)).with_key(key)),
                |recipe| Ok(recipe.clone()),
            )
        } else {
            return Err(
                Error::general_error(format!("No filename in key '{}'", key)).with_key(key),
            );
        }
    }

    async fn recipe_opt(&self, key: &Key, envref: EnvRef<E>) -> Result<Option<Recipe>, Error> {
        if let Some(filename) = key.filename() {
            let parent_key = key.parent();
            if self.has_recipes(&parent_key, envref.clone()).await? {
                let recipes = self.get_recipes(&parent_key, envref).await?;
                return Ok(recipes.get(&filename.name).cloned());
            }
        }
        Ok(None)
    }

    async fn has_recipes(&self, key: &Key, envref: EnvRef<E>) -> Result<bool, Error> {
        envref
            .get_async_store()
            .contains(&key.join("recipes.yaml"))
            .await
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
/// Serialized root of a `recipes.yaml` file.
/// It can also be used as a general collection of recipes (e.g. for batch processing).
pub struct RecipeList {
    /// Recipes in file order.
    pub recipes: Vec<Recipe>,
}

impl RecipeList {
    /// Creates an empty recipe list.
    pub fn new() -> Self {
        RecipeList {
            recipes: Vec::new(),
        }
    }

    /// Appends a recipe, preserving file order.
    pub fn add_recipe(&mut self, recipe: Recipe) {
        self.recipes.push(recipe);
    }

    /// Returns the number of recipes, including entries without valid filenames.
    pub fn len(&self) -> usize {
        self.recipes.len()
    }

    /// Finds the first recipe whose parsed query filename equals `name`.
    ///
    /// Recipes with invalid queries or no filename are skipped.
    pub fn get(&self, name: &str) -> Option<&Recipe> {
        self.recipes.iter().find(|r| {
            if let Ok(Some(filename)) = r.filename() {
                filename.name == name
            } else {
                false
            }
        })
    }

    /// Assigns the same logical working key to every recipe that does not already have one.
    ///
    /// This method mutates in iteration order. If it encounters an explicitly populated `cwd`, it
    /// returns an error after any preceding recipes have already been changed.
    pub fn set_cwd(&mut self, cwd: String) -> Result<(), Error> {
        for recipe in &mut self.recipes {
            if recipe.cwd.is_none() {
                recipe.cwd = Some(cwd.clone());
            } else {
                eprintln!("Recipe already has CWD set to {:?}", recipe.cwd);
                return Err(Error::not_supported(
                    "CWD can't be explicitly specified in a recipe".to_owned(),
                ));
            }
        }
        Ok(())
    }
}

/// Names one of the built-in recipe providers, so a configuration document can select one by
/// name rather than by constructing a Rust value.
///
/// The set is deliberately closed. A host that supplies its own [`AsyncRecipeProvider`] still
/// passes it to the environment directly — custom providers are too varied to be named here, and
/// there is no registration hook.
///
/// # Spelling
///
/// The lowercase spelling is the durable part: `default` and `trivial`, with `none` and
/// `no_recipes` accepted as aliases for `trivial` on input. Serialization always emits the
/// canonical name.
///
/// ```
/// use liquers_core::recipes::RecipeProviderChoice;
///
/// let choice: RecipeProviderChoice = serde_yaml::from_str("no_recipes").unwrap();
/// assert_eq!(choice, RecipeProviderChoice::Trivial);
/// assert_eq!(choice.as_str(), "trivial");
/// ```
///
/// # Default
///
/// [`RecipeProviderChoice::Default`] is the *document* default: a configuration that says nothing
/// about recipes most plausibly wants them to work. This is deliberately not the same as the
/// unconfigured default of every environment constructor, which is per-crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecipeProviderChoice {
    /// [`DefaultRecipeProvider`] — recipes read from `recipes.yaml` through the environment's store.
    #[default]
    Default,
    /// [`TrivialRecipeProvider`] — no recipes at all.
    #[serde(alias = "none", alias = "no_recipes")]
    Trivial,
}

impl RecipeProviderChoice {
    /// The provider this choice names, shared.
    pub fn provider<E: Environment>(self) -> Arc<dyn AsyncRecipeProvider<E>> {
        match self {
            RecipeProviderChoice::Default => Arc::new(DefaultRecipeProvider),
            RecipeProviderChoice::Trivial => Arc::new(TrivialRecipeProvider),
        }
    }

    /// The provider this choice names, owned.
    ///
    /// The `liquers-core` environments take `Box<dyn AsyncRecipeProvider<Self>>` where
    /// `liquers-lib`'s takes `Arc<…>`, so both shapes are offered rather than forcing an
    /// `Arc::from(Box::new(…))` at the call site.
    pub fn boxed_provider<E: Environment>(self) -> Box<dyn AsyncRecipeProvider<E>> {
        match self {
            RecipeProviderChoice::Default => Box::new(DefaultRecipeProvider),
            RecipeProviderChoice::Trivial => Box::new(TrivialRecipeProvider),
        }
    }

    /// The canonical name used in a configuration document.
    ///
    /// This is what [`Display`](std::fmt::Display) and serialization emit; the aliases accepted by
    /// [`FromStr`](std::str::FromStr) and deserialization are not returned here.
    pub fn as_str(self) -> &'static str {
        match self {
            RecipeProviderChoice::Default => "default",
            RecipeProviderChoice::Trivial => "trivial",
        }
    }
}

impl std::str::FromStr for RecipeProviderChoice {
    type Err = Error;

    /// Parses a canonical name or an accepted alias.
    ///
    /// Recognizes `default`, `trivial`, and the `trivial` aliases `none` and `no_recipes`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "default" => Ok(RecipeProviderChoice::Default),
            "trivial" | "none" | "no_recipes" => Ok(RecipeProviderChoice::Trivial),
            other => Err(Error::general_error(format!(
                "Unknown recipe provider '{}'; expected one of: default, trivial (aliases: none, no_recipes)",
                other
            ))),
        }
    }
}

impl fmt::Display for RecipeProviderChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod test {
    use crate::{
        command_metadata::{ArgumentInfo, CommandMetadata, CommandMetadataRegistry},
        error::ErrorType,
        parse::parse_key,
        plan::{ParameterValue, Plan, Step},
        query::{Key, QuerySource},
    };

    use super::RecipeList;

    #[test]
    fn empty_recipe() {
        let recipe = super::Recipe::new(
            "".to_string(),
            "title".to_string(),
            "description".to_string(),
        )
        .unwrap();
        assert_eq!(recipe.query, "".to_string());
        assert_eq!(recipe.title, "title".to_string());
        assert_eq!(recipe.description, "description".to_string());
        assert_eq!(recipe.arguments.len(), 0);
        assert_eq!(recipe.links.len(), 0);
        let plan = recipe
            .to_plan(&super::CommandMetadataRegistry::new())
            .unwrap();
        eprintln!("plan: {:?}", &plan);
        eprintln!("");
        eprintln!("plan.yaml:\n{}", serde_yaml::to_string(&plan).unwrap());
        eprintln!("");
    }
    /// A recipe must be able to override a variadic argument.
    ///
    /// Before `MultipleParameters` carried its argument name, `ParameterValue::name()` returned
    /// `None` for it, so `override_value` / `override_link` could not find the slot and `to_plan`
    /// failed with "Argument columns not found in last action". Reported by Codex review on
    /// PR #38; see specs/design/variadic-arguments-declaration/.
    #[test]
    fn recipe_overrides_a_variadic_argument() {
        let mut cr = CommandMetadataRegistry::new();
        cr.add_command(
            CommandMetadata::new("select_columns")
                .with_argument(ArgumentInfo::string_argument("columns").set_multiple()),
        );

        let recipe = super::Recipe::new(
            "select_columns".to_string(),
            "title".to_string(),
            "description".to_string(),
        )
        .unwrap()
        .with_argument("columns".to_string(), serde_json::json!(["a", "b"]));

        let plan = recipe
            .to_plan(&cr)
            .expect("a recipe override must reach a variadic argument");

        let Some(Step::Action { parameters, .. }) = plan.steps.last() else {
            panic!("expected an action step");
        };
        let ParameterValue::MultipleParameters(name, elements) = &parameters.0[0] else {
            panic!(
                "an applied override must stay a parameter list: {:?}",
                parameters.0[0]
            );
        };
        assert_eq!(name, "columns");
        assert_eq!(elements.len(), 2, "one element per array entry");
        assert_eq!(elements[0].value(), Some(serde_json::json!("a")));
        assert_eq!(elements[1].value(), Some(serde_json::json!("b")));
    }

    /// The same, for a link override.
    #[test]
    fn recipe_link_overrides_a_variadic_argument() {
        let mut cr = CommandMetadataRegistry::new();
        cr.add_command(
            CommandMetadata::new("select_columns")
                .with_argument(ArgumentInfo::string_argument("columns").set_multiple()),
        );

        let recipe = super::Recipe::new(
            "select_columns".to_string(),
            "title".to_string(),
            "description".to_string(),
        )
        .unwrap()
        .with_link("columns".to_string(), "-R/config/cols.json".to_string());

        let plan = recipe
            .to_plan(&cr)
            .expect("a recipe link override must reach a variadic argument");

        let Some(Step::Action { parameters, .. }) = plan.steps.last() else {
            panic!("expected an action step");
        };
        let ParameterValue::MultipleParameters(name, elements) = &parameters.0[0] else {
            panic!("an applied link override must stay a parameter list");
        };
        assert_eq!(name, "columns");
        assert_eq!(elements.len(), 1);
        assert!(
            elements[0].link().is_some(),
            "the element must hold the link"
        );
    }

    #[test]
    fn recipe_with_parameter() {
        let mut cr = CommandMetadataRegistry::new();
        cr.add_command(CommandMetadata::new("a").with_argument(ArgumentInfo::any_argument("b")));
        let recipe = super::Recipe::new(
            "a".to_string(),
            "title".to_string(),
            "description".to_string(),
        )
        .unwrap()
        .with_argument("b".to_string(), serde_json::json!("c"));
        let plan = recipe.to_plan(&cr).unwrap();
        eprintln!("plan.yaml:\n{}", serde_yaml::to_string(&plan).unwrap());
        eprintln!("");
        assert!(plan.len() == 1);
        if let Step::Action {
            action_name,
            parameters,
            ..
        } = &plan[0]
        {
            assert!(action_name == "a");
            assert!(parameters.0.len() == 1);
            if let ParameterValue::OverrideValue(name, value) = &parameters.0[0] {
                assert!(name == "b");
                assert!(value == &serde_json::json!("c"));
            } else {
                assert!(false);
            }
        } else {
            assert!(false);
        }
    }

    #[test]
    fn recipe_to_plan_preserves_programmatic_cwd() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmr = CommandMetadataRegistry::new();
        cmr.add_command(&CommandMetadata::new("identity"));

        let mut recipe = super::Recipe::new(
            "-R-stored/./input.txt/-/identity/result.txt".to_owned(),
            "Relative input".to_owned(),
            "Read input relative to the recipe folder".to_owned(),
        )?;
        recipe.cwd = Some("programmatic".to_owned());

        let plan = recipe.to_plan(&cmr)?;
        let Some(Step::SetCwd(cwd)) = plan.steps.first() else {
            panic!(
                "expected a recipe SetCwd prefix, got {:?}",
                plan.steps.first()
            );
        };
        assert_eq!(cwd.encode(), "programmatic");
        let Some(Step::GetResource(key)) = plan.steps.get(1) else {
            panic!(
                "expected a source-relative GetResource after the prefix, got {:?}",
                plan.steps.get(1)
            );
        };
        assert_eq!(key.encode(), "./input.txt");
        assert_eq!(
            plan.steps
                .iter()
                .filter(|step| matches!(step, Step::SetCwd(_)))
                .count(),
            1
        );
        assert_eq!(
            plan.init_steps
                .iter()
                .filter(|step| matches!(step, Step::Info(message) if message == "Recipe set CWD to 'programmatic'"))
                .count(),
            1
        );
        assert_eq!(
            plan.query.encode(),
            "-R-stored/./input.txt/-/identity/result.txt"
        );

        let keyed_plan = recipe.to_plan_for_key(&cmr, &parse_key("programmatic/result.txt")?)?;
        assert!(matches!(
            keyed_plan.steps.first(),
            Some(Step::SetCwd(cwd)) if cwd.encode() == "programmatic"
        ));
        assert!(matches!(
            keyed_plan.steps.get(1),
            Some(Step::GetResource(key)) if key.encode() == "./input.txt"
        ));
        Ok(())
    }

    #[test]
    fn recipe_prefix_info_is_exactly_once_and_precedes_query_steps(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut cmr = CommandMetadataRegistry::new();
        cmr.add_command(
            CommandMetadata::new("action").with_argument(ArgumentInfo::any_argument("use_link")),
        );

        let mut recipe = super::Recipe::new(
            "-R-cwd/../c/-/action-~X~-R/./hello.txt~E".to_owned(),
            "Ordered CWD".to_owned(),
            "Resolve an explicit CWD before a linked query".to_owned(),
        )?;
        recipe.cwd = Some("a/b".to_owned());

        let plan = recipe.to_plan(&cmr)?;
        assert!(matches!(
            plan.steps.first(),
            Some(Step::SetCwd(cwd)) if cwd.encode() == "a/b"
        ));
        assert!(matches!(
            plan.steps.get(1),
            Some(Step::SetCwd(cwd)) if cwd.encode() == "../c"
        ));
        let Some(Step::Action { parameters, .. }) = plan.steps.get(2) else {
            panic!(
                "expected the raw action at step 2, got {:?}",
                plan.steps.get(2)
            );
        };
        let Some(ParameterValue::ParameterLink(name, link, _)) = parameters.0.first() else {
            panic!(
                "expected the raw linked action parameter, got {:?}",
                parameters.0.first()
            );
        };
        assert_eq!(name, "use_link");
        assert_eq!(link.encode(), "-R/./hello.txt");
        assert_eq!(
            plan.steps
                .iter()
                .filter(|step| matches!(step, Step::SetCwd(_)))
                .count(),
            2
        );
        assert_eq!(
            plan.init_steps
                .iter()
                .filter(|step| matches!(step, Step::Info(message) if message == "Recipe set CWD to 'a/b'"))
                .count(),
            1
        );
        assert!(!plan.steps.iter().any(
            |step| matches!(step, Step::Info(message) if message == "Recipe set CWD to 'a/b'")
        ));
        Ok(())
    }

    #[test]
    fn recipe_to_plan_rejects_invalid_programmatic_cwd() -> Result<(), Box<dyn std::error::Error>> {
        let mut recipe = super::Recipe::new(
            "".to_owned(),
            "Invalid CWD".to_owned(),
            "Invalid programmatic working key".to_owned(),
        )?;
        recipe.cwd = Some("/invalid".to_owned());

        let error = recipe
            .to_plan(&CommandMetadataRegistry::new())
            .expect_err("an absolute key is invalid in Recipe::cwd");
        assert_eq!(error.error_type, ErrorType::ParseError);
        Ok(())
    }

    #[test]
    fn recipe_plan_round_trip_keeps_raw_operands_and_prefix(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut cmr = CommandMetadataRegistry::new();
        cmr.add_command(
            CommandMetadata::new("action").with_argument(ArgumentInfo::any_argument("use_link")),
        );

        let mut recipe = super::Recipe::new(
            "-R-stored/./input.txt/-/action-~X~-R/./original.txt~E/result.txt".to_owned(),
            "Round trip".to_owned(),
            "Keep source-relative plan operands".to_owned(),
        )?
        .with_link("use_link".to_owned(), "-R/../linked.txt".to_owned());
        recipe.cwd = Some("a/b".to_owned());

        let recipe_json = serde_json::to_string(&recipe)?;
        let recipe_from_json: super::Recipe = serde_json::from_str(&recipe_json)?;
        assert_eq!(recipe_from_json, recipe);
        let recipe_yaml = serde_yaml::to_string(&recipe)?;
        let recipe_from_yaml: super::Recipe = serde_yaml::from_str(&recipe_yaml)?;
        assert_eq!(recipe_from_yaml, recipe);

        let mut plan = recipe.to_plan(&cmr)?;
        plan.query.source = QuerySource::String("recipe round trip".to_owned());
        let action = plan
            .steps
            .iter_mut()
            .find(|step| matches!(step, Step::Action { .. }))
            .expect("round-trip fixture should contain an action");
        let Step::Action {
            position,
            parameters,
            ..
        } = action
        else {
            unreachable!("the preceding search selected an action");
        };
        let expected_action_position = position.clone();
        let parameter = parameters
            .0
            .iter_mut()
            .find(|parameter| {
                matches!(parameter, ParameterValue::OverrideLink(name, _) if name == "use_link")
            })
            .expect("round-trip fixture should contain the link override");
        let ParameterValue::OverrideLink(_, linked_query) = parameter else {
            unreachable!("the preceding search selected an override link");
        };
        linked_query.source = QuerySource::Other("recipe override".to_owned());
        let expected_link_position = linked_query.position();

        let plan_json = serde_json::to_string(&plan)?;
        let plan_yaml = serde_yaml::to_string(&plan)?;
        for serialized in [&plan_json, &plan_yaml] {
            assert!(!serialized.contains("cwd_cursor"));
            assert!(!serialized.contains("defaulted_to_root"));
            assert!(!serialized.contains("context_cwd"));
        }
        let decoded_plans: Vec<Plan> = vec![
            serde_json::from_str(&plan_json)?,
            serde_yaml::from_str(&plan_yaml)?,
        ];
        for decoded in decoded_plans {
            assert_eq!(
                decoded.query.encode(),
                "-R-stored/./input.txt/-/action-~X~-R/./original.txt~E/result.txt"
            );
            assert_eq!(
                decoded.query.source,
                QuerySource::String("recipe round trip".to_owned())
            );
            assert!(matches!(
                decoded.steps.first(),
                Some(Step::SetCwd(cwd)) if cwd.encode() == "a/b"
            ));
            assert!(matches!(
                decoded.steps.get(1),
                Some(Step::GetResource(key)) if key.encode() == "./input.txt"
            ));
            assert_eq!(
                decoded
                    .init_steps
                    .iter()
                    .filter(|step| matches!(step, Step::Info(message) if message == "Recipe set CWD to 'a/b'"))
                    .count(),
                1
            );

            let decoded_action = decoded
                .steps
                .iter()
                .find(|step| matches!(step, Step::Action { .. }))
                .expect("decoded plan should contain an action");
            let Step::Action {
                position,
                parameters,
                ..
            } = decoded_action
            else {
                unreachable!("the preceding search selected an action");
            };
            assert_eq!(position, &expected_action_position);
            let decoded_parameter = parameters
                .0
                .iter()
                .find(|parameter| {
                    matches!(parameter, ParameterValue::OverrideLink(name, _) if name == "use_link")
                })
                .expect("decoded plan should contain the link override");
            let ParameterValue::OverrideLink(_, linked_query) = decoded_parameter else {
                unreachable!("the preceding search selected an override link");
            };
            assert_eq!(linked_query.encode(), "-R/../linked.txt");
            assert_eq!(
                linked_query.source,
                QuerySource::Other("recipe override".to_owned())
            );
            assert_eq!(linked_query.position(), expected_link_position);
        }
        Ok(())
    }

    #[test]
    fn serialization_keeps_raw_cwd_links_positions_and_no_runtime_schema(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut cmr = CommandMetadataRegistry::new();
        cmr.add_command(
            CommandMetadata::new("action").with_argument(ArgumentInfo::any_argument("use_link")),
        );
        let mut recipe = super::Recipe::new(
            "-R-cwd/../c/-/action-~X~-R/./hello.txt~E/result.txt".to_owned(),
            "Raw ordered CWD".to_owned(),
            "Preserve source-relative plan data during serialization".to_owned(),
        )?;
        recipe.cwd = Some("a/b".to_owned());

        let recipe_json = serde_json::to_value(&recipe)?;
        let recipe_yaml = serde_yaml::to_value(&recipe)?;
        let recipe_json_text = serde_json::to_string(&recipe_json)?;
        let recipe_yaml_text = serde_yaml::to_string(&recipe_yaml)?;
        for serialized in [&recipe_json_text, &recipe_yaml_text] {
            assert!(!serialized.contains("cwd_cursor"));
            assert!(!serialized.contains("defaulted_to_root"));
            assert!(!serialized.contains("context_cwd"));
        }
        let decoded_recipes: Vec<super::Recipe> = vec![
            serde_json::from_value(recipe_json)?,
            serde_yaml::from_value(recipe_yaml)?,
        ];
        assert!(decoded_recipes.iter().all(|decoded| decoded == &recipe));

        let mut plan = recipe.to_plan(&cmr)?;
        plan.query.source = QuerySource::String("raw serialization".to_owned());
        let action = plan
            .steps
            .iter_mut()
            .find(|step| matches!(step, Step::Action { .. }))
            .expect("serialized plan should contain an action");
        let Step::Action {
            position,
            parameters,
            ..
        } = action
        else {
            unreachable!("the preceding search selected an action");
        };
        let expected_action_position = position.clone();
        let ParameterValue::ParameterLink(_, linked_query, parameter_position) =
            &mut parameters.0[0]
        else {
            panic!("expected parsed parameter link");
        };
        linked_query.source = QuerySource::Other("raw linked query".to_owned());
        let expected_parameter_position = parameter_position.clone();
        let expected_link_position = linked_query.position();

        let plan_json = serde_json::to_value(&plan)?;
        let plan_yaml = serde_yaml::to_value(&plan)?;
        let plan_json_text = serde_json::to_string(&plan_json)?;
        let plan_yaml_text = serde_yaml::to_string(&plan_yaml)?;
        for serialized in [&plan_json_text, &plan_yaml_text] {
            assert!(!serialized.contains("cwd_cursor"));
            assert!(!serialized.contains("defaulted_to_root"));
            assert!(!serialized.contains("context_cwd"));
        }

        let decoded_plans: Vec<Plan> = vec![
            serde_json::from_value(plan_json)?,
            serde_yaml::from_value(plan_yaml)?,
        ];
        for decoded in decoded_plans {
            assert_eq!(
                decoded.query.encode(),
                "-R-cwd/../c/-/action-~X~-R/./hello.txt~E/result.txt"
            );
            assert_eq!(
                decoded.query.source,
                QuerySource::String("raw serialization".to_owned())
            );
            assert!(matches!(
                decoded.steps.first(),
                Some(Step::SetCwd(cwd)) if cwd.encode() == "a/b"
            ));
            assert!(matches!(
                decoded.steps.get(1),
                Some(Step::SetCwd(cwd)) if cwd.encode() == "../c"
            ));
            assert_eq!(
                decoded
                    .init_steps
                    .iter()
                    .filter(|step| matches!(step, Step::Info(message) if message == "Recipe set CWD to 'a/b'"))
                    .count(),
                1
            );

            let action = decoded
                .steps
                .iter()
                .find(|step| matches!(step, Step::Action { .. }))
                .expect("decoded plan should contain an action");
            let Step::Action {
                position,
                parameters,
                ..
            } = action
            else {
                unreachable!("the preceding search selected an action");
            };
            assert_eq!(position, &expected_action_position);
            let ParameterValue::ParameterLink(_, linked_query, parameter_position) =
                &parameters.0[0]
            else {
                panic!("expected decoded parameter link");
            };
            assert_eq!(parameter_position, &expected_parameter_position);
            assert_eq!(linked_query.encode(), "-R/./hello.txt");
            assert_eq!(
                linked_query.source,
                QuerySource::Other("raw linked query".to_owned())
            );
            assert_eq!(linked_query.position(), expected_link_position);
        }
        Ok(())
    }

    #[test]
    fn recipefile() {
        let recipe = super::Recipe::new("a".to_string(), "test title".to_string(), "".to_string())
            .unwrap()
            .with_argument("b".to_string(), serde_json::json!("c"));
        let mut recipelist = RecipeList::new();
        recipelist.add_recipe(recipe);
        eprintln!(
            "recipes.yaml:\n{}",
            serde_yaml::to_string(&recipelist).unwrap()
        );
    }

    #[cfg(feature = "async_store")]
    #[tokio::test]
    async fn test_default_recipe_provider() {
        use crate::context::{EnvRef, Environment, SimpleEnvironment};
        use crate::metadata::Metadata;
        use crate::parse::parse_key;
        use crate::store::{AsyncMemoryStore, AsyncStore};
        use crate::value::Value;

        // Create an async memory store and populate it with recipes.yaml
        let memory_store = AsyncMemoryStore::new(&Key::new());

        // Create a recipe list
        let mut recipe_list = RecipeList::new();
        recipe_list.add_recipe(
            super::Recipe::new(
                "-R/hello/test.txt".to_string(),
                "Test Recipe".to_string(),
                "A test recipe".to_string(),
            )
            .unwrap(),
        );
        recipe_list.add_recipe(
            super::Recipe::new(
                "-R/data/another.json".to_string(),
                "Another Recipe".to_string(),
                "Another test recipe".to_string(),
            )
            .unwrap(),
        );

        // Serialize to YAML
        let yaml_content = serde_yaml::to_string(&recipe_list).unwrap();
        eprintln!("recipes.yaml content:\n{}", yaml_content);

        let mut authored_cwd_recipe = super::Recipe::new(
            "-R/data/authored.json".to_owned(),
            "Authored CWD".to_owned(),
            "CWD must be supplied by the provider".to_owned(),
        )
        .unwrap();
        authored_cwd_recipe.cwd = Some("yaml-authored".to_owned());
        let mut authored_cwd_list = RecipeList::new();
        authored_cwd_list.add_recipe(authored_cwd_recipe);
        let authored_cwd_yaml = serde_yaml::to_string(&authored_cwd_list).unwrap();

        // Store the recipes.yaml in memory at folder/recipes.yaml
        let recipes_key = parse_key("folder/recipes.yaml").unwrap();
        let metadata = Metadata::new();
        memory_store
            .set(&recipes_key, yaml_content.as_bytes(), &metadata)
            .await
            .unwrap();
        memory_store
            .set(
                &parse_key("hello/test.txt").unwrap(),
                "Hello, world!".as_bytes(),
                &metadata,
            )
            .await
            .unwrap();
        memory_store
            .set(
                &parse_key("invalid/recipes.yaml").unwrap(),
                authored_cwd_yaml.as_bytes(),
                &metadata,
            )
            .await
            .unwrap();

        // Create a SimpleEnvironment and set the async store
        let mut env = SimpleEnvironment::<Value>::new();
        env.with_async_store(Box::new(memory_store));
        let envref: EnvRef<SimpleEnvironment<Value>> = env.to_ref();

        // Create a DefaultRecipeProvider
        let provider = super::DefaultRecipeProvider;

        // Test has_recipes
        let folder_key = parse_key("folder").unwrap();
        let has_recipes =
            super::AsyncRecipeProvider::has_recipes(&provider, &folder_key, envref.clone())
                .await
                .unwrap();
        assert!(has_recipes, "Should have recipes in folder");

        // Test get_recipes
        let recipes = provider
            .get_recipes(&folder_key, envref.clone())
            .await
            .unwrap();
        assert_eq!(recipes.len(), 2, "Should have 2 recipes");

        // Test assets_with_recipes
        let assets =
            super::AsyncRecipeProvider::assets_with_recipes(&provider, &folder_key, envref.clone())
                .await
                .unwrap();
        assert_eq!(assets.len(), 2, "Should have 2 assets with recipes");

        let asset_names: Vec<String> = assets.iter().map(|a| a.name.clone()).collect();
        assert!(asset_names.contains(&"test.txt".to_string()));
        assert!(asset_names.contains(&"another.json".to_string()));

        // Test recipe
        let test_recipe_key = parse_key("folder/test.txt").unwrap();
        let recipe =
            super::AsyncRecipeProvider::recipe(&provider, &test_recipe_key, envref.clone())
                .await
                .unwrap();
        assert_eq!(recipe.title, "Test Recipe");
        assert_eq!(recipe.description, "A test recipe");

        // Verify CWD was set correctly
        assert_eq!(recipe.cwd, Some("folder".to_string()));

        let authored_cwd_error = provider
            .get_recipes(&parse_key("invalid").unwrap(), envref.clone())
            .await
            .expect_err("recipes.yaml must not author cwd");
        assert_eq!(authored_cwd_error.error_type, ErrorType::NotSupported);

        // Test recipe_opt with existing recipe
        let recipe_opt =
            super::AsyncRecipeProvider::recipe_opt(&provider, &test_recipe_key, envref.clone())
                .await
                .unwrap();
        assert!(recipe_opt.is_some());

        // Test recipe_opt with non-existing recipe
        let nonexistent_key = parse_key("folder/nonexistent.txt").unwrap();
        let recipe_opt =
            super::AsyncRecipeProvider::recipe_opt(&provider, &nonexistent_key, envref.clone())
                .await
                .unwrap();
        assert!(recipe_opt.is_none());

        // Test contains
        let contains =
            super::AsyncRecipeProvider::contains(&provider, &test_recipe_key, envref.clone())
                .await
                .unwrap();
        assert!(contains, "Should contain test.txt recipe");

        let not_contains =
            super::AsyncRecipeProvider::contains(&provider, &nonexistent_key, envref.clone())
                .await
                .unwrap();
        assert!(!not_contains, "Should not contain nonexistent.txt recipe");
    }

    #[test]
    fn test_recipe_expires_default() {
        let recipe = super::Recipe::new(
            "test".to_string(),
            "Test".to_string(),
            "Test recipe".to_string(),
        )
        .unwrap();
        assert_eq!(recipe.expires, crate::expiration::Expires::Never);
    }

    #[test]
    fn test_recipe_expires_serialization() {
        let mut recipe = super::Recipe::new(
            "test".to_string(),
            "Test".to_string(),
            "Test recipe".to_string(),
        )
        .unwrap();
        recipe.expires = "in 5 min".parse().unwrap();
        let json = serde_json::to_string(&recipe).unwrap();
        let recipe2: super::Recipe = serde_json::from_str(&json).unwrap();
        assert_eq!(recipe2.expires, recipe.expires);
    }

    /// The document default is `default`: a configuration that says nothing about recipes gets
    /// working recipes.
    #[test]
    fn recipe_provider_choice_defaults_to_default() {
        assert_eq!(
            super::RecipeProviderChoice::default(),
            super::RecipeProviderChoice::Default
        );
    }

    #[test]
    fn recipe_provider_choice_round_trips_through_yaml() {
        for choice in [
            super::RecipeProviderChoice::Default,
            super::RecipeProviderChoice::Trivial,
        ] {
            let yaml = serde_yaml::to_string(&choice).unwrap();
            assert_eq!(yaml.trim(), choice.as_str());
            let back: super::RecipeProviderChoice = serde_yaml::from_str(&yaml).unwrap();
            assert_eq!(back, choice);
        }
    }

    #[test]
    fn recipe_provider_choice_round_trips_through_json() {
        for choice in [
            super::RecipeProviderChoice::Default,
            super::RecipeProviderChoice::Trivial,
        ] {
            let json = serde_json::to_string(&choice).unwrap();
            assert_eq!(json, format!("\"{}\"", choice.as_str()));
            let back: super::RecipeProviderChoice = serde_json::from_str(&json).unwrap();
            assert_eq!(back, choice);
        }
    }

    /// A configuration document may spell the empty provider `none` or `no_recipes`; both mean
    /// `trivial`, and serialization normalizes back to the canonical name.
    #[test]
    fn recipe_provider_choice_accepts_trivial_aliases() {
        for spelling in ["trivial", "none", "no_recipes"] {
            let from_yaml: super::RecipeProviderChoice =
                serde_yaml::from_str(spelling).unwrap_or_else(|e| panic!("{}: {}", spelling, e));
            assert_eq!(from_yaml, super::RecipeProviderChoice::Trivial);

            let from_json: super::RecipeProviderChoice =
                serde_json::from_str(&format!("\"{}\"", spelling)).unwrap();
            assert_eq!(from_json, super::RecipeProviderChoice::Trivial);

            let parsed: super::RecipeProviderChoice = spelling.parse().unwrap();
            assert_eq!(parsed, super::RecipeProviderChoice::Trivial);
        }
        assert_eq!(
            serde_yaml::to_string(&super::RecipeProviderChoice::Trivial)
                .unwrap()
                .trim(),
            "trivial"
        );
    }

    #[test]
    fn recipe_provider_choice_parses_and_displays_names() {
        assert_eq!(
            "default".parse::<super::RecipeProviderChoice>().unwrap(),
            super::RecipeProviderChoice::Default
        );
        assert_eq!(super::RecipeProviderChoice::Default.to_string(), "default");
        assert_eq!(super::RecipeProviderChoice::Trivial.to_string(), "trivial");

        let err = "postgres"
            .parse::<super::RecipeProviderChoice>()
            .unwrap_err();
        assert!(
            err.to_string().contains("postgres"),
            "error should name the rejected input: {}",
            err
        );

        // An unknown name is rejected by the deserializer too, rather than silently defaulting.
        assert!(serde_json::from_str::<super::RecipeProviderChoice>("\"postgres\"").is_err());
    }

    /// The two choices are distinguished by behaviour, not by type name: `default` resolves a
    /// recipe held in the environment's store, `trivial` resolves none.
    #[cfg(feature = "async_store")]
    #[tokio::test]
    async fn recipe_provider_choice_yields_providers_that_differ_in_behaviour() {
        use crate::context::{EnvRef, Environment, SimpleEnvironment};
        use crate::metadata::Metadata;
        use crate::parse::parse_key;
        use crate::store::{AsyncMemoryStore, AsyncStore};
        use crate::value::Value;
        use std::sync::Arc;

        let memory_store = AsyncMemoryStore::new(&Key::new());
        let mut recipe_list = RecipeList::new();
        recipe_list.add_recipe(
            super::Recipe::new(
                "-R/hello/test.txt".to_string(),
                "Test Recipe".to_string(),
                "A test recipe".to_string(),
            )
            .unwrap(),
        );
        memory_store
            .set(
                &parse_key("folder/recipes.yaml").unwrap(),
                serde_yaml::to_string(&recipe_list).unwrap().as_bytes(),
                &Metadata::new(),
            )
            .await
            .unwrap();

        let mut env = SimpleEnvironment::<Value>::new();
        env.with_async_store(Box::new(memory_store));
        let envref: EnvRef<SimpleEnvironment<Value>> = env.to_ref();

        let folder_key = parse_key("folder").unwrap();
        let recipe_key = parse_key("folder/test.txt").unwrap();

        type Env = SimpleEnvironment<Value>;

        let default: Arc<dyn super::AsyncRecipeProvider<Env>> =
            super::RecipeProviderChoice::Default.provider();
        assert!(
            default
                .has_recipes(&folder_key, envref.clone())
                .await
                .unwrap(),
            "the default choice must see the recipes.yaml in the store"
        );
        let recipe = default
            .recipe(&recipe_key, envref.clone())
            .await
            .expect("the default choice must resolve a recipe from the store");
        assert_eq!(recipe.title, "Test Recipe");

        let trivial: Box<dyn super::AsyncRecipeProvider<Env>> =
            super::RecipeProviderChoice::Trivial.boxed_provider();
        assert!(
            !trivial
                .has_recipes(&folder_key, envref.clone())
                .await
                .unwrap(),
            "the trivial choice must report no recipes even when the store holds them"
        );
        assert!(
            trivial
                .recipe_opt(&recipe_key, envref.clone())
                .await
                .unwrap()
                .is_none(),
            "the trivial choice must resolve no recipe"
        );
        assert!(
            trivial.recipe(&recipe_key, envref).await.is_err(),
            "a required lookup against the trivial choice is an error"
        );
    }
}
