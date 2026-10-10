//! [`ManifestRecipeProvider`] — the recipe provider that serves chunk keys from `*.manifest.yaml`
//! files, so `-R/data/sales/daily_0042.csv` (and every explicit chunk a manifest names) is
//! evaluable the same way a `recipes.yaml` entry is, from anywhere, not only through the source.
//!
//! See `specs/design/record-streams/phase2-architecture.md`, §"Keyed chunks: naming, a recipe
//! provider, and `stored` / `cached`", §"A. Chunk keys" and §"B. A recipe provider that serves
//! chunk keys from manifests", and `specs/design/record-streams/phase4-implementation.md`, Step
//! 4.3 (including its "Lookup cost" rules, which this file's caching follows).

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
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
///   call [`AsyncStore::listdir`] on every request. `AsyncStore` has no directory-level version
///   to check the way `manifests` checks a file's, so this list is **event-driven**: the asset
///   manager calls [`AsyncRecipeProvider::directory_changed`] after every write or removal it
///   mediates (and the HTTP Store API does so through it), and that drops the listings of the
///   directory and its subtree. A change made behind Liquers' back (directly on disk) is not
///   noticed until [`ManifestRecipeProvider::clear_cache`] is called or the provider is rebuilt.
///   A listing fill that races an invalidation is never kept (see `generation`).
///
/// A name that does not look like a generated chunk, and whose folder holds no manifest at all,
/// costs exactly one `listdir` (cached thereafter) and no `get` — a plain file with no manifest is
/// cheap by construction, per `phase4-implementation.md`'s "a store whose `listdir` is the default
/// empty answer simply has no manifests".
#[derive(Debug, Default)]
pub struct ManifestRecipeProvider {
    manifests: scc::HashMap<Key, CachedManifest>,
    folders: scc::HashMap<Key, Arc<Vec<String>>>,
    /// Bumped by every invalidation. A folder listing read while it moved is not kept: it may
    /// predate the write that caused the invalidation.
    generation: AtomicU64,
}

impl ManifestRecipeProvider {
    /// A provider with nothing cached yet.
    pub fn new() -> Self {
        ManifestRecipeProvider {
            manifests: scc::HashMap::new(),
            folders: scc::HashMap::new(),
            generation: AtomicU64::new(0),
        }
    }

    /// Drops every cached manifest and folder listing, for a host reacting to a change made behind
    /// Liquers' back (directly on disk). Changes made through Liquers need no call.
    pub async fn clear_cache(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.folders.clear_async().await;
        self.manifests.clear_async().await;
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
        let generation_before = self.generation.load(Ordering::SeqCst);
        let entries = match store.listdir(folder).await {
            Ok(entries) => entries,
            // A folder the store refuses holds no manifests. Not cached: the refusal belongs to
            // the store's configuration, not to the folder's contents.
            Err(error) if error.error_type == ErrorType::KeyNotSupported => {
                return Ok(Arc::new(Vec::new()))
            }
            Err(error) => return Err(error),
        };
        let names: Vec<String> = entries
            .into_iter()
            .filter(|name| name.ends_with(".manifest.yaml"))
            .collect();
        let names = Arc::new(names);
        self.cache_listing(folder, names.clone(), generation_before).await;
        Ok(names)
    }

    /// Keeps `names` as `folder`'s listing unless an invalidation ran since `generation_before`
    /// was read. Insert first, then re-check: an invalidation is "bump, then retain", so either
    /// its bump precedes the re-check (this fill removes its own entry) or follows it (the
    /// insert already happened, so its retain removes the entry).
    async fn cache_listing(&self, folder: &Key, names: Arc<Vec<String>>, generation_before: u64) {
        let _ = self.folders.insert_async(folder.clone(), names.clone()).await;
        if self.generation.load(Ordering::SeqCst) != generation_before {
            let _ = self
                .folders
                .remove_if_async(folder, |cached| Arc::ptr_eq(cached, &names))
                .await;
        }
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
                Err(error) if is_absent(&error) => {
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
            Err(error) if is_absent(&error) => Ok(None),
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
        let recipes = DefaultRecipeProvider::new()
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

/// Whether a store error means "no manifest here": the key is absent, or the store refuses it as
/// unsupported — a store router does so for any key no member covers, and a key the store cannot
/// hold cannot hold a manifest.
fn is_absent(error: &Error) -> bool {
    matches!(error.error_type, ErrorType::KeyNotFound | ErrorType::KeyNotSupported)
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
        cached: Some(manifest.cached.into()),
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
    recipe.cached = Some(manifest.cached.into());
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

    /// Drops the cached folder listings of `dir` and its subtree. Parsed manifests stay: each hit
    /// is version-checked, and a removed manifest is evicted by that check.
    async fn directory_changed(&self, dir: &Key) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.folders
            .retain_async(|folder, _| !folder.has_key_prefix(dir))
            .await;
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
        assert_eq!(recipe.cached, Some(liquers_core::cache_strategy::CacheStrategy::All));
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
        assert_eq!(recipe.cached, Some(liquers_core::cache_strategy::CacheStrategy::All));
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
    async fn can_make_answers_for_a_template_name_far_beyond_any_listing(
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

        // An index no listing would ever enumerate — `can_make` must not need to.
        let key = parse_key("data/sales/daily_999999.csv")?;
        assert!(AsyncRecipeProvider::can_make(&provider, &key, envref.clone()).await?);
        assert!(!AsyncRecipeProvider::contains(&provider, &key, envref.clone()).await?);
        Ok(())
    }

    #[tokio::test]
    async fn contains_reports_only_explicit_chunks() -> Result<(), Box<dyn std::error::Error>> {
        let store = AsyncMemoryStore::new(&Key::new());
        set_yaml(&store, &parse_key("data/sales/daily.manifest.yaml")?, DAILY_MANIFEST, 1).await;
        let envref = env_with_store(store);
        let provider = ManifestRecipeProvider::new();
        let explicit = parse_key("data/sales/orders_eu.csv")?;
        let generated = parse_key("data/sales/daily_0042.csv")?;
        assert!(AsyncRecipeProvider::contains(&provider, &explicit, envref.clone()).await?);
        assert!(!AsyncRecipeProvider::contains(&provider, &generated, envref.clone()).await?);
        assert!(AsyncRecipeProvider::can_make(&provider, &explicit, envref.clone()).await?);
        Ok(())
    }

    /// Delegates to an in-memory store and counts `listdir` calls and `get` calls of manifest files.
    struct CountingStore {
        inner: AsyncMemoryStore,
        listdirs: std::sync::atomic::AtomicUsize,
        gets: std::sync::atomic::AtomicUsize,
    }

    impl CountingStore {
        fn new() -> Self {
            CountingStore {
                inner: AsyncMemoryStore::new(&Key::new()),
                listdirs: Default::default(),
                gets: Default::default(),
            }
        }
        fn listdirs(&self) -> usize {
            self.listdirs.load(Ordering::SeqCst)
        }
        fn gets(&self) -> usize {
            self.gets.load(Ordering::SeqCst)
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl AsyncStore for CountingStore {
        async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
            if key.encode().ends_with(".manifest.yaml") {
                self.gets.fetch_add(1, Ordering::SeqCst);
            }
            self.inner.get(key).await
        }
        async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
            self.inner.get_metadata(key).await
        }
        async fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
            self.inner.set(key, data, metadata).await
        }
        async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
            self.inner.set_metadata(key, metadata).await
        }
        async fn contains(&self, key: &Key) -> Result<bool, Error> {
            self.inner.contains(key).await
        }
        async fn remove(&self, key: &Key) -> Result<(), Error> {
            self.inner.remove(key).await
        }
        async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
            self.listdirs.fetch_add(1, Ordering::SeqCst);
            self.inner.listdir(key).await
        }
    }

    fn counting_env(store: Arc<CountingStore>) -> EnvRef<TestEnv> {
        struct Shared(Arc<CountingStore>);
        #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
        #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
        impl AsyncStore for Shared {
            async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
                self.0.get(key).await
            }
            async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
                self.0.get_metadata(key).await
            }
            async fn set(&self, key: &Key, d: &[u8], m: &Metadata) -> Result<(), Error> {
                self.0.set(key, d, m).await
            }
            async fn set_metadata(&self, key: &Key, m: &Metadata) -> Result<(), Error> {
                self.0.set_metadata(key, m).await
            }
            async fn contains(&self, key: &Key) -> Result<bool, Error> {
                self.0.contains(key).await
            }
            async fn remove(&self, key: &Key) -> Result<(), Error> {
                self.0.remove(key).await
            }
            async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
                self.0.listdir(key).await
            }
        }
        let mut env = TestEnv::new();
        env.with_async_store(Box::new(Shared(store)));
        env.to_ref()
    }

    async fn can_make_key(
        provider: &ManifestRecipeProvider,
        envref: &EnvRef<TestEnv>,
        key: &str,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        Ok(AsyncRecipeProvider::can_make(provider, &parse_key(key)?, envref.clone()).await?)
    }

    #[tokio::test]
    async fn a_manifest_added_after_listing_is_seen_after_directory_changed(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let store = Arc::new(CountingStore::new());
        let envref = counting_env(store.clone());
        let provider = ManifestRecipeProvider::new();
        assert!(!can_make_key(&provider, &envref, "data/sales/orders_eu.csv").await?);
        set_yaml_dyn(&envref, "data/sales/weekly.manifest.yaml", DAILY_MANIFEST.to_string()).await;
        AsyncRecipeProvider::<TestEnv>::directory_changed(&provider, &parse_key("data/sales")?).await;
        assert!(can_make_key(&provider, &envref, "data/sales/orders_eu.csv").await?);
        Ok(())
    }

    async fn set_yaml_dyn(envref: &EnvRef<TestEnv>, key: &str, yaml: String) {
        let mut metadata = Metadata::new();
        metadata.set_version(Some(Version::from_bytes(yaml.as_bytes()))).expect("version");
        envref
            .get_async_store()
            .set(&parse_key(key).expect("key"), yaml.as_bytes(), &metadata)
            .await
            .expect("set");
    }

    #[tokio::test]
    async fn a_manifest_added_without_notice_is_not_seen() -> Result<(), Box<dyn std::error::Error>> {
        let store = Arc::new(CountingStore::new());
        let envref = counting_env(store.clone());
        let provider = ManifestRecipeProvider::new();
        assert!(!can_make_key(&provider, &envref, "data/sales/orders_eu.csv").await?);
        set_yaml_dyn(&envref, "data/sales/weekly.manifest.yaml", DAILY_MANIFEST.to_string()).await;
        // Out-of-band: the cached listing is still served — the decided contract.
        assert!(!can_make_key(&provider, &envref, "data/sales/orders_eu.csv").await?);
        provider.clear_cache().await;
        assert!(can_make_key(&provider, &envref, "data/sales/orders_eu.csv").await?);
        Ok(())
    }

    #[tokio::test]
    async fn a_removed_manifest_stops_resolving_after_directory_changed(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let store = Arc::new(CountingStore::new());
        let envref = counting_env(store.clone());
        set_yaml_dyn(&envref, "data/sales/daily.manifest.yaml", DAILY_MANIFEST.to_string()).await;
        let provider = ManifestRecipeProvider::new();
        assert!(can_make_key(&provider, &envref, "data/sales/orders_eu.csv").await?);
        envref.get_async_store().remove(&parse_key("data/sales/daily.manifest.yaml")?).await?;
        AsyncRecipeProvider::<TestEnv>::directory_changed(&provider, &parse_key("data/sales")?).await;
        assert!(!can_make_key(&provider, &envref, "data/sales/orders_eu.csv").await?);
        Ok(())
    }

    #[tokio::test]
    async fn directory_changed_drops_the_subtree() -> Result<(), Box<dyn std::error::Error>> {
        let store = Arc::new(CountingStore::new());
        let envref = counting_env(store.clone());
        let provider = ManifestRecipeProvider::new();
        for key in ["data/a/x_0001.csv", "data/b/x_0001.csv", "other/x_0001.csv"] {
            can_make_key(&provider, &envref, key).await?;
        }
        assert_eq!(store.listdirs(), 3);
        AsyncRecipeProvider::<TestEnv>::directory_changed(&provider, &parse_key("data")?).await;
        for key in ["data/a/x_0001.csv", "data/b/x_0001.csv", "other/x_0001.csv"] {
            can_make_key(&provider, &envref, key).await?;
        }
        // data/a and data/b re-listed; other/ still cached.
        assert_eq!(store.listdirs(), 5);
        Ok(())
    }

    #[tokio::test]
    async fn clear_cache_drops_everything() -> Result<(), Box<dyn std::error::Error>> {
        let store = Arc::new(CountingStore::new());
        let envref = counting_env(store.clone());
        set_yaml_dyn(&envref, "data/sales/daily.manifest.yaml", DAILY_MANIFEST.to_string()).await;
        let provider = ManifestRecipeProvider::new();
        assert!(can_make_key(&provider, &envref, "data/sales/orders_eu.csv").await?);
        let (lists, gets) = (store.listdirs(), store.gets());
        provider.clear_cache().await;
        assert!(can_make_key(&provider, &envref, "data/sales/orders_eu.csv").await?);
        assert!(store.listdirs() > lists || store.gets() > gets);
        assert_eq!(store.gets(), gets + 1, "the manifest is parsed again");
        Ok(())
    }

    #[tokio::test]
    async fn an_unchanged_folder_is_listed_once() -> Result<(), Box<dyn std::error::Error>> {
        let store = Arc::new(CountingStore::new());
        let envref = counting_env(store.clone());
        let provider = ManifestRecipeProvider::new();
        for _ in 0..3 {
            can_make_key(&provider, &envref, "data/sales/plain.txt").await?;
        }
        assert_eq!(store.listdirs(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn a_non_manifest_write_does_not_reparse_the_manifest(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let store = Arc::new(CountingStore::new());
        let envref = counting_env(store.clone());
        set_yaml_dyn(&envref, "data/sales/daily.manifest.yaml", DAILY_MANIFEST.to_string()).await;
        let provider = ManifestRecipeProvider::new();
        assert!(can_make_key(&provider, &envref, "data/sales/orders_eu.csv").await?);
        let (lists, gets) = (store.listdirs(), store.gets());
        set_yaml_dyn(&envref, "data/sales/other.csv", "a,b".to_string()).await;
        AsyncRecipeProvider::<TestEnv>::directory_changed(&provider, &parse_key("data/sales")?).await;
        assert!(can_make_key(&provider, &envref, "data/sales/orders_eu.csv").await?);
        assert_eq!(store.listdirs(), lists + 1);
        assert_eq!(store.gets(), gets);
        Ok(())
    }

    #[tokio::test]
    async fn a_fill_racing_an_invalidation_is_not_kept() -> Result<(), Box<dyn std::error::Error>> {
        let store = Arc::new(CountingStore::new());
        let envref = counting_env(store.clone());
        let provider = ManifestRecipeProvider::new();
        let folder = parse_key("data/sales")?;
        // A fill that read its generation, then an invalidation ran before it inserted.
        let before = provider.generation.load(Ordering::SeqCst);
        AsyncRecipeProvider::<TestEnv>::directory_changed(&provider, &folder).await;
        provider.cache_listing(&folder, Arc::new(vec![]), before).await;
        assert!(provider.folders.read_async(&folder, |_, _| ()).await.is_none());
        can_make_key(&provider, &envref, "data/sales/plain.txt").await?;
        assert_eq!(store.listdirs(), 1);
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

    /// Refuses every key, as a store router does for a key no member covers.
    struct RefusingStore;

    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl AsyncStore for RefusingStore {
        async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
            Err(Error::key_not_supported(key, "refusing"))
        }
        async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
            Err(Error::key_not_supported(key, "refusing"))
        }
        async fn set_metadata(&self, key: &Key, _metadata: &Metadata) -> Result<(), Error> {
            Err(Error::key_not_supported(key, "refusing"))
        }
        async fn contains(&self, key: &Key) -> Result<bool, Error> {
            Err(Error::key_not_supported(key, "refusing"))
        }
        async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
            Err(Error::key_not_supported(key, "refusing"))
        }
    }

    /// A folder the store refuses holds no manifests: both lookup paths — the template fast path
    /// (`get_manifest`) and the explicit-chunk listing (`manifest_names`) — answer "no recipe"
    /// rather than failing the query, so a plain `-R/` resource under such a store still resolves.
    #[tokio::test]
    async fn an_unsupported_key_has_no_manifest_recipe() -> Result<(), Box<dyn std::error::Error>> {
        let mut env = TestEnv::new();
        env.with_async_store(Box::new(RefusingStore));
        let envref = env.to_ref();
        let provider = ManifestRecipeProvider::new();

        for key in ["data/input.txt", "data/daily_0001.csv"] {
            let key = parse_key(key)?;
            assert!(AsyncRecipeProvider::recipe_opt(&provider, &key, envref.clone()).await?.is_none());
            assert!(!AsyncRecipeProvider::contains(&provider, &key, envref.clone()).await?);
        }
        Ok(())
    }
}
