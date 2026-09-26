//! [`ManifestRecipeProvider`] — the recipe provider that serves chunk keys from `*.manifest.yaml`
//! files, so `-R/data/sales/daily_0042.csv` (and every explicit chunk a manifest names) is
//! evaluable the same way a `recipes.yaml` entry is, from anywhere, not only through the source.
//!
//! See `specs/design/record-streams/phase2-architecture.md`, §"Keyed chunks: naming, a recipe
//! provider, and `stored` / `cached`", §"A. Chunk keys" and §"B. A recipe provider that serves
//! chunk keys from manifests", and `specs/design/record-streams/phase4-implementation.md`, Step
//! 4.3 (including its "Lookup cost" rules, which this file's caching follows).

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;

use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::{Error, ErrorType};
use liquers_core::metadata::Version;
use liquers_core::plan::Plan;
use liquers_core::query::{Key, Query, ResourceName};
use liquers_core::recipes::{AsyncRecipeProvider, DefaultRecipeProvider, Recipe};
use liquers_core::store::AsyncStore;

use crate::manifest::ManifestSpec;
use crate::sources::ManifestSource;

// =================================================================================================
// Caching
// =================================================================================================

/// A parsed manifest plus the store version it was read at, so a later call can tell whether it is
/// still current without re-parsing.
#[derive(Debug, Clone)]
struct CachedManifest {
    source: Arc<ManifestSource>,
    version: Version,
}

// =================================================================================================
// ManifestRecipeProvider
// =================================================================================================

/// Serves recipes for a manifest's chunks — explicit and template-generated alike — so they are
/// addressable by key exactly as `recipes.yaml` entries are (phase2-architecture.md §B).
///
/// **This provider sits on the interpreter's hot path.** In the `liquers-lib` provider chain it is
/// consulted for every key `recipes.yaml` does not define — i.e. every plain `-R/…` file — and
/// each `get(key)` on the asset manager resolves the recipe more than once (`is_volatile`, then
/// evaluation). Two caches keep that cheap:
///
/// - **`manifests`** — a parsed [`ManifestSource`], keyed by its `*.manifest.yaml` file's own key,
///   alongside the store version it was read at. A cache hit costs one [`AsyncStore::get_metadata`]
///   call to confirm the version is unchanged (as cheap as a `contains`, on the stores this design
///   targets) rather than a full parse; a miss, or a version that no longer matches, costs one
///   [`AsyncStore::get`] to re-fetch and re-parse. **When the store never sets a version** on the
///   metadata it hands back (`Metadata::version` answers `None` — true of a `Metadata::new()`
///   written by hand, as opposed to one the asset manager built), there is nothing cheap to compare
///   against, and every call re-fetches and re-parses: correct, but back to the miss cost. This is
///   the "cheap signature" choice `phase4-implementation.md` leaves open — an explicit version is
///   used when the store provides one, and a full re-read is the fallback otherwise, rather than
///   inventing a directory-independent hash the store cannot actually attest to.
/// - **`folders`** — a folder's `*.manifest.yaml` filenames, so an explicit-chunk lookup does not
///   call [`AsyncStore::listdir`] on every request. This list is cached **for the life of the
///   provider**: a manifest added to (or removed from) a folder after it was first listed is not
///   noticed until a fresh `ManifestRecipeProvider` is built, because `AsyncStore` has no
///   directory-level version to check the way `manifests` checks a file's. Filed as
///   `MANIFEST-PROVIDER-FOLDER-LISTING-NEVER-REFRESHES`.
///
/// A name that does not look like a generated chunk, and whose folder holds no manifest at all,
/// costs exactly one `listdir` (cached thereafter) and no `get` — a plain file with no manifest is
/// cheap by construction, per `phase4-implementation.md`'s "a store whose `listdir` is the default
/// empty answer simply has no manifests".
#[derive(Debug, Default)]
pub struct ManifestRecipeProvider {
    manifests: scc::HashMap<Key, CachedManifest>,
    folders: scc::HashMap<Key, Arc<Vec<String>>>,
}

impl ManifestRecipeProvider {
    /// A provider with nothing cached yet.
    pub fn new() -> Self {
        ManifestRecipeProvider {
            manifests: scc::HashMap::new(),
            folders: scc::HashMap::new(),
        }
    }

    /// `folder`'s `*.manifest.yaml` filenames — cached; see the type's own doc comment for what
    /// that trades away. `Ok(Arc::new(Vec::new()))` for a folder with no manifests (including one
    /// `listdir` never returns anything for, e.g. `AsyncStore`'s default empty answer).
    async fn manifest_names(
        &self,
        folder: &Key,
        store: &Arc<dyn AsyncStore>,
    ) -> Result<Arc<Vec<String>>, Error> {
        if let Some(names) = self.folders.read_async(folder, |_, names| names.clone()).await {
            return Ok(names);
        }
        let entries = store.listdir(folder).await?;
        let names: Vec<String> = entries
            .into_iter()
            .filter(|name| name.ends_with(".manifest.yaml"))
            .collect();
        let names = Arc::new(names);
        let _ = self.folders.insert_async(folder.clone(), names.clone()).await;
        Ok(names)
    }

    /// The parsed manifest at `key`, cached and refreshed per the type's own doc comment. `Ok(None)`
    /// when the store holds nothing at `key` (never an error: a guessed template-fast-path key, or
    /// a name a caller listed a moment before it was removed, is simply absent).
    async fn get_manifest(
        &self,
        key: &Key,
        store: &Arc<dyn AsyncStore>,
    ) -> Result<Option<Arc<ManifestSource>>, Error> {
        if let Some(cached) = self.manifests.read_async(key, |_, entry| entry.clone()).await {
            match store.get_metadata(key).await {
                Ok(metadata) => {
                    if let Some(version) = metadata.version() {
                        if version == cached.version {
                            return Ok(Some(cached.source));
                        }
                    }
                    // No version to compare (the store does not track one) or it has changed:
                    // fall through and re-read below.
                }
                Err(error) if error.error_type == ErrorType::KeyNotFound => {
                    let _ = self.manifests.remove_async(key).await;
                    return Ok(None);
                }
                Err(error) => return Err(error),
            }
        }

        match store.get(key).await {
            Ok((bytes, metadata)) => {
                let spec: ManifestSpec = serde_yaml::from_slice(&bytes).map_err(|error| {
                    Error::general_error(format!("manifest '{key}': {error}")).with_key(key)
                })?;
                let source = Arc::new(ManifestSource::new(spec, Some(key.clone()))?);
                let version = metadata
                    .version()
                    .unwrap_or_else(|| Version::from_bytes(&bytes));
                let _ = self
                    .manifests
                    .upsert_async(key.clone(), CachedManifest { source: source.clone(), version })
                    .await;
                Ok(Some(source))
            }
            Err(error) if error.error_type == ErrorType::KeyNotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Refuses a chunk name that the folder's own `recipes.yaml` also defines
    /// (phase2-architecture.md §A: "a chunk name that a sibling `recipes.yaml` also defines" is a
    /// collision refused at load). Read fresh every time, via core's public
    /// `DefaultRecipeProvider::get_recipes` — only reached once a manifest has already matched
    /// `name`, so a folder with no `recipes.yaml` (the common case) pays for this exactly when it
    /// is about to answer `Some`, never on a lookup that would answer `None` anyway.
    async fn check_no_recipes_yaml_collision<E: Environment>(
        folder: &Key,
        name: &str,
        envref: &EnvRef<E>,
        manifest_key: &Key,
    ) -> Result<(), Error> {
        let recipes = DefaultRecipeProvider
            .get_recipes(folder, envref.clone())
            .await?;
        if recipes.get(name).is_some() {
            return Err(Error::general_error(format!(
                "chunk \"{name}\" is defined both by manifest '{manifest_key}' and by '{}'",
                folder.join("recipes.yaml")
            ))
            .with_key(&folder.join(name)));
        }
        Ok(())
    }
}

/// Whether `name` has the *shape* a template-generated chunk name would
/// (`<prefix>_<digits>.<extension>`), and if so, its prefix — used only to guess which single
/// manifest to check for the fast path. Deliberately looser than
/// [`crate::manifest::ChunkNaming::index_of`]'s canonical match: `daily_10.csv` matches here
/// (prefix `"daily"`), even though it is not a chunk `ChunkNaming::key` would ever produce (that
/// needs four digits) and `index_of` correctly rejects it. A wrong guess here costs nothing but
/// falling through to the explicit-chunk lookup; it is
/// never treated as an answer by itself.
fn generated_name_prefix(name: &str) -> Option<&str> {
    let (stem, _extension) = name.rsplit_once('.')?;
    let (prefix, digits) = stem.rsplit_once('_')?;
    if prefix.is_empty() || digits.is_empty() {
        return None;
    }
    if digits.bytes().all(|b| b.is_ascii_digit()) {
        Some(prefix)
    } else {
        None
    }
}

/// A template-generated chunk's recipe. A template chunk has no `arguments`/`links` of its own to
/// merge under (`ChunkTemplate` carries none — phase2-architecture.md: "Template chunks share the
/// template's, by construction"), so the manifest's are used as they are; `stored`, `cached`,
/// `expires` and `volatile` are the manifest's, and `cwd` is the manifest's folder.
fn template_chunk_recipe(manifest: &ManifestSpec, folder: &Key, query: Query) -> Recipe {
    Recipe {
        query: query.encode(),
        title: manifest.title.clone(),
        description: manifest.description.clone(),
        arguments: manifest.arguments.clone(),
        links: manifest.links.clone(),
        cwd: Some(folder.encode()),
        volatile: manifest.volatile,
        has_circular_dependencies: false,
        circular_dependency_key: None,
        expires: manifest.expires.clone(),
        stored: Some(manifest.stored),
        cached: Some(manifest.cached),
    }
}

/// An explicit chunk's recipe: the chunk's own `title`, `description`, `arguments` and `links` are
/// kept — with the manifest's shared `arguments`/`links` merged in under them, so the chunk's own
/// entry wins on a name clash — while `stored`, `cached`, `expires` and `volatile` are always the
/// manifest's (as for a template chunk), and `cwd` becomes the manifest's folder.
fn explicit_chunk_recipe(manifest: &ManifestSpec, folder: &Key, chunk: &Recipe) -> Recipe {
    let mut recipe = chunk.clone();
    recipe.cwd = Some(folder.encode());
    recipe.stored = Some(manifest.stored);
    recipe.cached = Some(manifest.cached);
    recipe.expires = manifest.expires.clone();
    recipe.volatile = manifest.volatile;
    for (name, value) in &manifest.arguments {
        recipe
            .arguments
            .entry(name.clone())
            .or_insert_with(|| value.clone());
    }
    for (name, value) in &manifest.links {
        recipe
            .links
            .entry(name.clone())
            .or_insert_with(|| value.clone());
    }
    recipe
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<E: Environment> AsyncRecipeProvider<E> for ManifestRecipeProvider {
    async fn has_recipes(&self, key: &Key, envref: EnvRef<E>) -> Result<bool, Error> {
        let store = envref.get_async_store();
        Ok(!self.manifest_names(key, &store).await?.is_empty())
    }

    /// The explicit chunks only (phase2-architecture.md §B: "generated chunk names are addressable
    /// but not listed").
    async fn assets_with_recipes(
        &self,
        key: &Key,
        envref: EnvRef<E>,
    ) -> Result<Vec<ResourceName>, Error> {
        let store = envref.get_async_store();
        let names = self.manifest_names(key, &store).await?;
        let mut seen = HashSet::new();
        let mut assets = Vec::new();
        for manifest_name in names.iter() {
            let manifest_key = key.join(manifest_name.as_str());
            let Some(source) = self.get_manifest(&manifest_key, &store).await? else {
                continue;
            };
            for chunk in &source.spec().chunks {
                if let Some(filename) = chunk.filename()? {
                    if seen.insert(filename.encode().to_string()) {
                        assets.push(filename);
                    }
                }
            }
        }
        Ok(assets)
    }

    async fn recipe_plan(&self, key: &Key, envref: EnvRef<E>) -> Result<Plan, Error> {
        let recipe = self.recipe(key, envref.clone()).await?;
        let cmr = envref.get_command_metadata_registry();
        recipe.to_plan_for_key(cmr, key).map_err(|error| error.with_key(key))
    }

    async fn recipe(&self, key: &Key, envref: EnvRef<E>) -> Result<Recipe, Error> {
        self.recipe_opt(key, envref).await?.ok_or_else(|| {
            Error::general_error(format!("No recipe found for key {}", key)).with_key(key)
        })
    }

    /// Serves an explicit or template chunk's recipe. See the type's own doc comment for the cost
    /// of each branch.
    async fn recipe_opt(&self, key: &Key, envref: EnvRef<E>) -> Result<Option<Recipe>, Error> {
        let Some(name) = key.filename().map(|resource_name| resource_name.encode().to_string())
        else {
            return Ok(None);
        };
        let folder = key.parent();
        let store = envref.get_async_store();

        // Fast path: a name shaped like a generated chunk checks exactly one manifest.
        if let Some(prefix) = generated_name_prefix(&name) {
            let manifest_key = folder.join(format!("{prefix}.manifest.yaml"));
            if let Some(source) = self.get_manifest(&manifest_key, &store).await? {
                if let (Some(naming), Some(template)) =
                    (source.naming(), source.spec().template.as_ref())
                {
                    if let Some(index) = naming.index_of(&name) {
                        if index as usize >= source.spec().chunks.len() {
                            let query = template.query_at(index, Some(&name))?;
                            let recipe = template_chunk_recipe(source.spec(), &folder, query);
                            Self::check_no_recipes_yaml_collision(
                                &folder,
                                &name,
                                &envref,
                                &manifest_key,
                            )
                            .await?;
                            return Ok(Some(recipe));
                        }
                    }
                }
            }
            // No matching template chunk (wrong extension, non-canonical padding, or no such
            // manifest at all): fall through to the explicit-chunk lookup below, exactly as a name
            // that never looked generated would.
        }

        // Explicit-chunk lookup: every manifest in the folder, by its own filename-keyed chunks.
        let names = self.manifest_names(&folder, &store).await?;
        for manifest_name in names.iter() {
            let manifest_key = folder.join(manifest_name.as_str());
            let Some(source) = self.get_manifest(&manifest_key, &store).await? else {
                continue;
            };
            for chunk in &source.spec().chunks {
                if let Some(filename) = chunk.filename()? {
                    if filename.encode() == name {
                        let recipe = explicit_chunk_recipe(source.spec(), &folder, chunk);
                        Self::check_no_recipes_yaml_collision(
                            &folder,
                            &name,
                            &envref,
                            &manifest_key,
                        )
                        .await?;
                        return Ok(Some(recipe));
                    }
                }
            }
        }
        Ok(None)
    }

    /// Matches the same way [`Self::recipe_opt`] does, without enumerating — a template's
    /// generated names are unbounded, which is exactly what the default `contains`
    /// (`RECIPE-CONTAINS-DEFAULT-ASSUMES-ENUMERABILITY`) cannot answer for.
    async fn contains(&self, key: &Key, envref: EnvRef<E>) -> Result<bool, Error> {
        Ok(self.recipe_opt(key, envref).await?.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::context::SimpleEnvironment;
    use liquers_core::metadata::Metadata;
    use liquers_core::parse::parse_key;
    use liquers_core::recipes::RecipeList;
    use liquers_core::store::AsyncMemoryStore;
    use liquers_core::value::Value;

    type TestEnv = SimpleEnvironment<Value>;

    async fn set_yaml(store: &AsyncMemoryStore, key: &Key, yaml: &str, version: u128) {
        let mut metadata = Metadata::new();
        metadata
            .set_version(Some(Version::new(version)))
            .expect("set_version");
        store
            .set(key, yaml.as_bytes(), &metadata)
            .await
            .expect("set");
    }

    fn env_with_store(store: AsyncMemoryStore) -> EnvRef<TestEnv> {
        let mut env = TestEnv::new();
        env.with_async_store(Box::new(store));
        env.to_ref()
    }

    const DAILY_MANIFEST: &str = "
manifest: record-stream
chunks:
  - query: ns-fixture/fixture_rows-0-1/orders_eu.csv
    title: Orders (EU)
    arguments:
      region: eu
template:
  query: ns-fixture/fixture_rows
  first_offset: 1000
  step: 10
  batch_size: 10
arguments:
  source: warehouse
links:
  owner: -R/config/owner.txt
";

    #[tokio::test]
    async fn serves_an_explicit_keyed_chunk_with_cwd_flags_and_merged_arguments(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let store = AsyncMemoryStore::new(&Key::new());
        set_yaml(
            &store,
            &parse_key("data/sales/daily.manifest.yaml")?,
            DAILY_MANIFEST,
            1,
        )
        .await;
        let envref = env_with_store(store);
        let provider = ManifestRecipeProvider::new();

        let key = parse_key("data/sales/orders_eu.csv")?;
        let recipe = AsyncRecipeProvider::recipe_opt(&provider, &key, envref.clone())
            .await?
            .expect("explicit chunk recipe");

        assert_eq!(recipe.cwd, Some("data/sales".to_string()));
        assert_eq!(recipe.title, "Orders (EU)");
        assert_eq!(recipe.stored, Some(true));
        assert_eq!(recipe.cached, Some(true));
        // The chunk's own argument is kept, and the manifest's shared one is merged in beside it.
        assert_eq!(
            recipe.arguments.get("region"),
            Some(&serde_json::json!("eu"))
        );
        assert_eq!(
            recipe.arguments.get("source"),
            Some(&serde_json::json!("warehouse"))
        );
        assert_eq!(recipe.links.get("owner"), Some(&"-R/config/owner.txt".to_string()));
        Ok(())
    }

    #[tokio::test]
    async fn explicit_chunks_own_argument_wins_over_the_shared_one() -> Result<(), Box<dyn std::error::Error>> {
        let yaml = "
manifest: record-stream
chunks:
  - query: ns-fixture/fixture_rows-0-1/orders_eu.csv
    arguments:
      source: override
arguments:
  source: warehouse
";
        let store = AsyncMemoryStore::new(&Key::new());
        set_yaml(&store, &parse_key("data/sales/daily.manifest.yaml")?, yaml, 1).await;
        let envref = env_with_store(store);
        let provider = ManifestRecipeProvider::new();

        let key = parse_key("data/sales/orders_eu.csv")?;
        let recipe = AsyncRecipeProvider::recipe_opt(&provider, &key, envref.clone())
            .await?
            .expect("explicit chunk recipe");
        assert_eq!(
            recipe.arguments.get("source"),
            Some(&serde_json::json!("override"))
        );
        Ok(())
    }

    #[tokio::test]
    async fn serves_a_template_chunk_at_the_global_index() -> Result<(), Box<dyn std::error::Error>> {
        let store = AsyncMemoryStore::new(&Key::new());
        set_yaml(
            &store,
            &parse_key("data/sales/daily.manifest.yaml")?,
            DAILY_MANIFEST,
            1,
        )
        .await;
        let envref = env_with_store(store);
        let provider = ManifestRecipeProvider::new();

        // One explicit chunk precedes the template, so `daily_0010.csv` is global index 10 —
        // offset `first_offset + step * index = 1000 + 10*10 = 1100`, not `1000 + 10*9`.
        let key = parse_key("data/sales/daily_0010.csv")?;
        let recipe = AsyncRecipeProvider::recipe_opt(&provider, &key, envref.clone())
            .await?
            .expect("template chunk recipe");

        assert_eq!(recipe.cwd, Some("data/sales".to_string()));
        assert_eq!(recipe.stored, Some(true));
        assert_eq!(recipe.cached, Some(true));
        let expected_query =
            liquers_core::parse::parse_query("ns-fixture/fixture_rows-1100-10/daily_0010.csv")?
                .encode();
        assert_eq!(recipe.query, expected_query);
        // Template chunks carry the manifest's shared arguments/links as they are, not merged
        // under anything of their own.
        assert_eq!(
            recipe.arguments.get("source"),
            Some(&serde_json::json!("warehouse"))
        );
        Ok(())
    }

    #[tokio::test]
    async fn shared_arguments_reach_every_template_chunk_recipe(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let store = AsyncMemoryStore::new(&Key::new());
        set_yaml(
            &store,
            &parse_key("data/sales/daily.manifest.yaml")?,
            DAILY_MANIFEST,
            1,
        )
        .await;
        let envref = env_with_store(store);
        let provider = ManifestRecipeProvider::new();

        for name in ["daily_0010.csv", "daily_0011.csv", "daily_0042.csv"] {
            let key = parse_key(format!("data/sales/{name}"))?;
            let recipe = AsyncRecipeProvider::recipe_opt(&provider, &key, envref.clone())
                .await?
                .unwrap_or_else(|| panic!("{name} should resolve to a template chunk"));
            assert_eq!(
                recipe.arguments.get("source"),
                Some(&serde_json::json!("warehouse")),
                "{name} did not carry the manifest's shared argument"
            );
            assert_eq!(
                recipe.links.get("owner"),
                Some(&"-R/config/owner.txt".to_string()),
                "{name} did not carry the manifest's shared link"
            );
            assert!(!recipe.volatile);
            assert_eq!(recipe.stored, Some(true));
        }
        Ok(())
    }

    #[tokio::test]
    async fn contains_answers_for_a_template_name_far_beyond_any_listing(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let store = AsyncMemoryStore::new(&Key::new());
        set_yaml(
            &store,
            &parse_key("data/sales/daily.manifest.yaml")?,
            DAILY_MANIFEST,
            1,
        )
        .await;
        let envref = env_with_store(store);
        let provider = ManifestRecipeProvider::new();

        // An index no listing would ever enumerate — `contains` must not need to.
        let key = parse_key("data/sales/daily_999999.csv")?;
        assert!(AsyncRecipeProvider::contains(&provider, &key, envref.clone()).await?);
        Ok(())
    }

    #[tokio::test]
    async fn assets_with_recipes_lists_only_explicit_chunks() -> Result<(), Box<dyn std::error::Error>> {
        let store = AsyncMemoryStore::new(&Key::new());
        set_yaml(
            &store,
            &parse_key("data/sales/daily.manifest.yaml")?,
            DAILY_MANIFEST,
            1,
        )
        .await;
        let envref = env_with_store(store);
        let provider = ManifestRecipeProvider::new();

        let folder = parse_key("data/sales")?;
        let assets =
            AsyncRecipeProvider::assets_with_recipes(&provider, &folder, envref.clone()).await?;
        let names: Vec<String> = assets.iter().map(|name| name.encode().to_string()).collect();
        assert_eq!(names, vec!["orders_eu.csv".to_string()]);
        Ok(())
    }

    #[tokio::test]
    async fn a_non_canonical_generated_looking_name_is_none() -> Result<(), Box<dyn std::error::Error>> {
        let store = AsyncMemoryStore::new(&Key::new());
        set_yaml(
            &store,
            &parse_key("data/sales/daily.manifest.yaml")?,
            DAILY_MANIFEST,
            1,
        )
        .await;
        let envref = env_with_store(store);
        let provider = ManifestRecipeProvider::new();

        // Shaped like a generated name, but not zero-padded to four digits: `index_of` rejects it,
        // and it is not an explicit chunk either.
        let key = parse_key("data/sales/daily_10.csv")?;
        assert_eq!(
            AsyncRecipeProvider::recipe_opt(&provider, &key, envref.clone()).await?,
            None
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_wrong_extension_is_none() -> Result<(), Box<dyn std::error::Error>> {
        let store = AsyncMemoryStore::new(&Key::new());
        set_yaml(
            &store,
            &parse_key("data/sales/daily.manifest.yaml")?,
            DAILY_MANIFEST,
            1,
        )
        .await;
        let envref = env_with_store(store);
        let provider = ManifestRecipeProvider::new();

        let key = parse_key("data/sales/daily_0010.json")?;
        assert_eq!(
            AsyncRecipeProvider::recipe_opt(&provider, &key, envref.clone()).await?,
            None
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_plain_file_with_no_manifest_is_none_cheaply() -> Result<(), Box<dyn std::error::Error>> {
        let store = AsyncMemoryStore::new(&Key::new());
        // No manifest anywhere in the store: `listdir` answers empty for every folder.
        let envref = env_with_store(store);
        let provider = ManifestRecipeProvider::new();

        let key = parse_key("data/sales/readme.txt")?;
        assert_eq!(
            AsyncRecipeProvider::recipe_opt(&provider, &key, envref.clone()).await?,
            None
        );
        // Once cached, a second lookup in the same folder still finds nothing, without erroring.
        let key2 = parse_key("data/sales/another.txt")?;
        assert_eq!(
            AsyncRecipeProvider::recipe_opt(&provider, &key2, envref.clone()).await?,
            None
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_chunk_name_the_sibling_recipes_yaml_also_defines_is_refused(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let store = AsyncMemoryStore::new(&Key::new());
        set_yaml(
            &store,
            &parse_key("data/sales/daily.manifest.yaml")?,
            DAILY_MANIFEST,
            1,
        )
        .await;
        let mut recipes = RecipeList::new();
        recipes.add_recipe(Recipe::new(
            "-R/other/source.csv/orders_eu.csv".to_string(),
            "Also here".to_string(),
            String::new(),
        )?);
        let recipes_yaml = serde_yaml::to_string(&recipes)?;
        set_yaml(
            &store,
            &parse_key("data/sales/recipes.yaml")?,
            &recipes_yaml,
            1,
        )
        .await;
        let envref = env_with_store(store);
        let provider = ManifestRecipeProvider::new();

        let key = parse_key("data/sales/orders_eu.csv")?;
        let result = AsyncRecipeProvider::recipe_opt(&provider, &key, envref.clone()).await;
        assert!(result.is_err(), "a name defined by both should be refused");
        let message = result.unwrap_err().message.clone();
        assert!(message.contains("orders_eu.csv"));
        assert!(message.contains("manifest") && message.contains("recipes.yaml"));
        Ok(())
    }

    #[tokio::test]
    async fn a_manifest_edit_with_a_new_stored_version_is_picked_up(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let store = AsyncMemoryStore::new(&Key::new());
        let manifest_key = parse_key("data/sales/daily.manifest.yaml")?;
        set_yaml(&store, &manifest_key, DAILY_MANIFEST, 1).await;
        let envref = env_with_store(store);
        let provider = ManifestRecipeProvider::new();

        let key = parse_key("data/sales/orders_eu.csv")?;
        let first = AsyncRecipeProvider::recipe_opt(&provider, &key, envref.clone())
            .await?
            .expect("chunk exists before the edit");
        assert_eq!(first.title, "Orders (EU)");

        // Edit the manifest in place, with a new stored version: the title changes and a second
        // chunk appears.
        let edited = "
manifest: record-stream
chunks:
  - query: ns-fixture/fixture_rows-0-1/orders_eu.csv
    title: Orders (EU, revised)
  - query: ns-fixture/fixture_rows-1-1/orders_us.csv
    title: Orders (US)
";
        let store = envref.get_async_store();
        let mut metadata = Metadata::new();
        metadata.set_version(Some(Version::new(2)))?;
        store.set(&manifest_key, edited.as_bytes(), &metadata).await?;

        let updated = AsyncRecipeProvider::recipe_opt(&provider, &key, envref.clone())
            .await?
            .expect("chunk still exists after the edit");
        assert_eq!(updated.title, "Orders (EU, revised)");

        let new_chunk_key = parse_key("data/sales/orders_us.csv")?;
        let new_chunk = AsyncRecipeProvider::recipe_opt(&provider, &new_chunk_key, envref.clone())
            .await?
            .expect("the newly added chunk should be visible once re-read");
        assert_eq!(new_chunk.title, "Orders (US)");
        Ok(())
    }
}
