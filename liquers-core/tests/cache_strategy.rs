//! Caching strategies and the command-level `cached: false` (`specs/design/plan-policy/`).
//!
//! Every scenario is written once, generically over the asset-manager kind, and run against both
//! — `Queued` (`DefaultAssetManager`) and `Inline` (`ImmediateAssetManager`) — because the
//! registration decision is made in each manager separately.
//!
//! **Observing what ran.** Every fixture command takes a `tag` argument and counts its calls per
//! `<command>-<tag>` in a shared map, so tests running in parallel threads never read each
//! other's counts. Queries are chains such as `seed-a/t1-a/t3-a`.
//!
//! **Observing what is kept.** `AssetManager::lookup_query_asset` answers whether a query asset is
//! registered, without creating one.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use async_trait::async_trait;
use liquers_core::{
    assets::AssetManager,
    cache_strategy::CacheStrategy,
    command_metadata::{ArgumentInfo, CommandKey},
    commands::CommandRegistry,
    context::{EnvRef, Environment, GenericEnvironment},
    environment_builder::{AssetManagerKind, AssetManagerOptions, EnvironmentBuilder, Inline, Queued},
    error::Error,
    metadata::Metadata,
    parse::{parse_key, parse_query},
    plan::Plan,
    query::{Key, ResourceName},
    recipes::{AsyncRecipeProvider, Recipe},
    state::State,
    store::AsyncMemoryStore,
    value::Value,
};

// ---------------------------------------------------------------------------
// Counting fixture commands
// ---------------------------------------------------------------------------

fn counts() -> &'static Mutex<HashMap<String, usize>> {
    static COUNTS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
    COUNTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// How many times `<command>-<tag>` ran.
fn ran(command: &str, tag: &str) -> usize {
    counts()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&format!("{command}-{tag}"))
        .copied()
        .unwrap_or(0)
}

/// Registers a counting command that appends its own name to the input text. `wide` is the
/// command registered with `cached: false`, standing for a cheap command with a large output.
fn register_counting<E: Environment<Value = Value>>(
    cr: &mut CommandRegistry<E>,
    name: &'static str,
    cached: Option<bool>,
) -> Result<(), Error> {
    let metadata = cr.register_command(
        CommandKey::new_name(name),
        move |state: &State<Value>, args, _context| -> Result<Value, Error> {
            let tag: String = args.get(0, "tag")?;
            *counts()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .entry(format!("{name}-{tag}"))
                .or_insert(0) += 1;
            // `seed` heads a chain, where the state is empty.
            let input = if name == "seed" {
                String::new()
            } else {
                state.try_into_string().unwrap_or_default()
            };
            Ok(Value::from(format!("{input}{name}")))
        },
    )?;
    metadata.with_argument(ArgumentInfo::string_argument("tag"));
    metadata.cached = cached;
    Ok(())
}

/// `cat-<link>`: appends the value of a link parameter, so a link's own caching can be observed.
fn register_cat<E: Environment<Value = Value>>(cr: &mut CommandRegistry<E>) -> Result<(), Error> {
    let metadata = cr.register_command(
        CommandKey::new_name("cat"),
        |state: &State<Value>, args, _context| -> Result<Value, Error> {
            let other: String = args.get(0, "other")?;
            let input = state.try_into_string().unwrap_or_default();
            Ok(Value::from(format!("{input}+{other}")))
        },
    )?;
    metadata.with_argument(ArgumentInfo::string_argument("other"));
    Ok(())
}

// ---------------------------------------------------------------------------
// A recipe provider serving fixed recipes
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct FixedRecipes {
    recipes: Arc<HashMap<Key, Recipe>>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<E: Environment> AsyncRecipeProvider<E> for FixedRecipes {
    async fn has_recipes(&self, _key: &Key, _envref: EnvRef<E>) -> Result<bool, Error> {
        Ok(false)
    }

    async fn assets_with_recipes(
        &self,
        _key: &Key,
        _envref: EnvRef<E>,
    ) -> Result<Vec<ResourceName>, Error> {
        Ok(Vec::new())
    }

    async fn recipe_plan(&self, key: &Key, envref: EnvRef<E>) -> Result<Plan, Error> {
        let recipe = self.recipes.get(key).cloned().ok_or_else(|| Error::key_not_found(key))?;
        recipe.to_plan_for_key(envref.get_command_metadata_registry(), key)
    }

    async fn recipe(&self, key: &Key, _envref: EnvRef<E>) -> Result<Recipe, Error> {
        self.recipes.get(key).cloned().ok_or_else(|| Error::key_not_found(key))
    }

    async fn recipe_opt(&self, key: &Key, _envref: EnvRef<E>) -> Result<Option<Recipe>, Error> {
        Ok(self.recipes.get(key).cloned())
    }
}

fn recipe(query: &str, cached: Option<CacheStrategy>) -> Result<Recipe, Error> {
    let mut recipe = Recipe::new(query.to_string(), String::new(), String::new())?;
    recipe.cached = cached;
    Ok(recipe)
}

type Env<K> = GenericEnvironment<Value, (), K>;

fn env<K: AssetManagerKind>(
    options: AssetManagerOptions,
    recipes: Vec<(Key, Recipe)>,
) -> Result<EnvRef<Env<K>>, Error> {
    let mut builder = EnvironmentBuilder::<Value, (), K>::new()
        .with_asset_manager_options(options)
        .with_async_store(Arc::new(AsyncMemoryStore::new(&Key::new())))
        .with_recipe_provider(Arc::new(FixedRecipes {
            recipes: Arc::new(recipes.into_iter().collect()),
        }));
    {
        let cr = &mut builder.command_registry;
        for name in ["seed", "t1", "t3", "t4"] {
            register_counting(cr, name, None)?;
        }
        register_counting(cr, "wide", Some(false))?;
        register_cat(cr)?;
    }
    builder.build()
}

async fn eval<E: Environment<Value = Value>>(envref: &EnvRef<E>, query: &str) -> Result<String, Error> {
    let asset = envref.get_asset_manager().get_asset(&parse_query(query)?).await?;
    asset.get().await?.try_into_string()
}

async fn eval_key<E: Environment<Value = Value>>(envref: &EnvRef<E>, key: &str) -> Result<String, Error> {
    let asset = envref.get_asset_manager().get(&parse_key(key)?).await?;
    asset.get().await?.try_into_string()
}

fn registered<E: Environment>(envref: &EnvRef<E>, query: &str) -> bool {
    match parse_query(query) {
        Ok(query) => envref.get_asset_manager().lookup_query_asset(&query).is_some(),
        Err(_) => false,
    }
}

fn log_of(metadata: &Metadata) -> Vec<String> {
    match metadata {
        Metadata::MetadataRecord(record) => record.log.iter().map(|e| e.message.clone()).collect(),
        Metadata::LegacyMetadata(_) => Vec::new(),
    }
}

fn opts() -> AssetManagerOptions {
    AssetManagerOptions::default()
}

// ---------------------------------------------------------------------------
// Scenarios (written once, run on both managers)
// ---------------------------------------------------------------------------

/// AC-1: `wide` declares `cached: false`, so its output is never a boundary — the plan cuts at
/// `seed/t1` and runs `wide` inline. A second query sharing that prefix reuses it.
async fn uncached_command_runs_inline<K: AssetManagerKind>(tag: &str) -> Result<(), Error> {
    let envref = env::<K>(opts(), vec![])?;
    let first = format!("seed-{tag}/t1-{tag}/wide-{tag}/t3-{tag}");
    assert_eq!(eval(&envref, &first).await?, "seedt1widet3");
    assert!(registered(&envref, &format!("seed-{tag}/t1-{tag}")), "the prefix is kept");
    assert!(
        !registered(&envref, &format!("seed-{tag}/t1-{tag}/wide-{tag}")),
        "the uncached command's output is not a boundary"
    );
    let second = format!("seed-{tag}/t1-{tag}/wide-{tag}/t4-{tag}");
    assert_eq!(eval(&envref, &second).await?, "seedt1widet4");
    assert_eq!(ran("seed", tag), 1);
    assert_eq!(ran("t1", tag), 1, "the kept prefix is reused");
    assert_eq!(ran("wide", tag), 2, "the inline command runs for each consumer");
    Ok(())
}

/// AC-2, AC-10: an ad-hoc query ending with an uncached command is evaluated on every request,
/// is not registered, is not volatile, and says why.
async fn uncached_command_ending_an_adhoc_query_is_not_reused<K: AssetManagerKind>(
    tag: &str,
) -> Result<(), Error> {
    let envref = env::<K>(opts(), vec![])?;
    let query = format!("seed-{tag}/wide-{tag}");
    eval(&envref, &query).await?;
    let asset = envref.get_asset_manager().get_asset(&parse_query(&query)?).await?;
    let state = asset.get().await?;
    assert_eq!(ran("wide", tag), 2);
    assert!(!registered(&envref, &query));
    assert!(!state.metadata.is_volatile(), "not cached is not volatile");
    let info = asset.get_asset_info().await?;
    assert_eq!(info.cached, Some(false), "AssetInfo shows it is not kept");
    let log = log_of(&asset.get_metadata().await?);
    assert!(
        log.iter().any(|m| m.contains("Not cached for reuse") && m.contains("wide")),
        "the reason is logged: {log:?}"
    );
    Ok(())
}

/// AC-3: `query_cache_strategy: result` keeps an ad-hoc query's result but no intermediate;
/// `none` keeps neither.
async fn query_strategy_result_and_none<K: AssetManagerKind>(tag: &str) -> Result<(), Error> {
    let envref = env::<K>(opts().with_query_cache_strategy(CacheStrategy::Result), vec![])?;
    let query = format!("seed-{tag}/t1-{tag}/t3-{tag}");
    eval(&envref, &query).await?;
    assert!(registered(&envref, &query), "result keeps the result");
    assert!(!registered(&envref, &format!("seed-{tag}/t1-{tag}")), "...and no intermediate");

    let none = format!("{tag}n");
    let envref = env::<K>(opts().with_query_cache_strategy(CacheStrategy::None), vec![])?;
    let query = format!("seed-{none}/t1-{none}/t3-{none}");
    eval(&envref, &query).await?;
    assert!(!registered(&envref, &query), "none keeps no result");
    assert!(!registered(&envref, &format!("seed-{none}/t1-{none}")));
    let asset = envref.get_asset_manager().get_asset(&parse_query(&query)?).await?;
    asset.get().await?;
    assert_eq!(asset.get_asset_info().await?.cached, Some(false));
    assert!(log_of(&asset.get_metadata().await?)
        .iter()
        .any(|m| m.contains("query cache strategy 'none'")));
    Ok(())
}

/// AC-4: under `none`, an intermediate a recipe already cached is reused, and a missing one is
/// computed without being kept. The plan is the same either way.
async fn existing_intermediate_is_reused<K: AssetManagerKind>(tag: &str) -> Result<(), Error> {
    let key = format!("warm-{tag}.txt");
    let recipes = vec![(
        parse_key(&key)?,
        recipe(&format!("seed-{tag}/t1-{tag}/t3-{tag}"), Some(CacheStrategy::All))?,
    )];
    let envref = env::<K>(opts().with_query_cache_strategy(CacheStrategy::None), recipes)?;
    eval_key(&envref, &key).await?;
    assert!(registered(&envref, &format!("seed-{tag}/t1-{tag}")), "the recipe cached it");
    assert_eq!(ran("t1", tag), 1);

    eval(&envref, &format!("seed-{tag}/t1-{tag}/t4-{tag}")).await?;
    assert_eq!(ran("t1", tag), 1, "an existing intermediate is reused under none");
    assert_eq!(ran("t4", tag), 1);

    let cold = format!("{tag}c");
    eval(&envref, &format!("seed-{cold}/t1-{cold}/t4-{cold}")).await?;
    assert_eq!(ran("t1", &cold), 1, "a missing one is computed once");
    assert!(!registered(&envref, &format!("seed-{cold}/t1-{cold}")), "...and not kept");
    assert!(!registered(&envref, &format!("seed-{cold}")));
    Ok(())
}

/// AC-5: a recipe's own `cached:` decides its result and its intermediates; `default` and an
/// absent field mean the manager's `recipe_cache_strategy` (here `result`).
async fn recipe_strategy_values<K: AssetManagerKind>(tag: &str) -> Result<(), Error> {
    let cases: [(&str, Option<CacheStrategy>, bool, bool); 4] = [
        ("all", Some(CacheStrategy::All), true, true),
        ("res", Some(CacheStrategy::Result), true, false),
        ("non", Some(CacheStrategy::None), false, false),
        ("def", None, true, false),
    ];
    let mut recipes = Vec::new();
    for (case, cached, _, _) in &cases {
        let t = format!("{tag}{case}");
        recipes.push((
            parse_key(&format!("r-{t}.txt"))?,
            recipe(&format!("seed-{t}/t1-{t}/t3-{t}"), *cached)?,
        ));
    }
    let envref = env::<K>(opts().with_recipe_cache_strategy(CacheStrategy::Result), recipes)?;
    let manager = envref.get_asset_manager();
    for (case, _, keeps_result, keeps_intermediates) in cases {
        let t = format!("{tag}{case}");
        let key = parse_key(&format!("r-{t}.txt"))?;
        eval_key(&envref, &format!("r-{t}.txt")).await?;
        assert_eq!(
            manager.lookup_key_asset(&key).is_some(),
            keeps_result,
            "result kept for {case}"
        );
        assert_eq!(
            registered(&envref, &format!("seed-{t}/t1-{t}")),
            keeps_intermediates,
            "intermediates kept for {case}"
        );
    }
    Ok(())
}

/// AC-6: a recipe with strategy `all` caches every boundary it creates even though ad-hoc
/// queries keep nothing; an ad-hoc query then reuses them and adds no asset of its own.
async fn boundary_follows_its_creator<K: AssetManagerKind>(tag: &str) -> Result<(), Error> {
    let key = format!("report-{tag}.txt");
    let recipes = vec![(
        parse_key(&key)?,
        recipe(&format!("seed-{tag}/t1-{tag}/t3-{tag}"), None)?,
    )];
    let options = opts()
        .with_recipe_cache_strategy(CacheStrategy::All)
        .with_query_cache_strategy(CacheStrategy::None);
    let envref = env::<K>(options, recipes)?;
    eval_key(&envref, &key).await?;
    assert!(registered(&envref, &format!("seed-{tag}/t1-{tag}")), "the recipe's boundary");
    assert!(registered(&envref, &format!("seed-{tag}")), "...and the boundary's own boundary");

    let guest = format!("seed-{tag}/t1-{tag}/t4-{tag}");
    eval(&envref, &guest).await?;
    assert_eq!(ran("t1", tag), 1, "the guest reuses the recipe's intermediate");
    assert!(!registered(&envref, &guest), "and keeps nothing of its own");
    Ok(())
}

/// AC-7: a recipe's keyed result follows the recipe's strategy, not its last command's flag.
async fn keyed_result_ignores_the_command_flag<K: AssetManagerKind>(
    tag: &str,
) -> Result<(), Error> {
    let key = format!("wide-{tag}.txt");
    let recipes = vec![(parse_key(&key)?, recipe(&format!("seed-{tag}/wide-{tag}"), None)?)];
    let envref = env::<K>(opts(), recipes)?;
    eval_key(&envref, &key).await?;
    eval_key(&envref, &key).await?;
    assert_eq!(ran("wide", tag), 1, "the keyed result is kept and reused");
    assert!(envref.get_asset_manager().lookup_key_asset(&parse_key(&key)?).is_some());
    Ok(())
}

/// AC-8: with `cut_predecessors: false` no boundary exists, so a cached prefix is not reused and
/// the result is unchanged.
async fn cut_predecessors_false_expands<K: AssetManagerKind>(tag: &str) -> Result<(), Error> {
    let envref = env::<K>(opts().with_cut_predecessors(false), vec![])?;
    let prefix = format!("seed-{tag}/t1-{tag}");
    eval(&envref, &prefix).await?;
    assert!(registered(&envref, &prefix));
    let full = format!("{prefix}/t3-{tag}");
    assert_eq!(eval(&envref, &full).await?, "seedt1t3");
    assert_eq!(ran("t1", tag), 2, "the expanded plan recomputes the prefix");
    // The switch's `Step::Info` is a planning diagnostic, which does not reach the asset log
    // today (`PLANNING-DIAGNOSTICS-NEVER-REACH-THE-ASSET-LOG`); the recomputation above is the
    // observable effect.
    Ok(())
}

/// AC-2 for links: a link parameter whose query ends with an uncached command is not kept,
/// whatever the strategy, while the query holding it is.
async fn uncached_link_is_not_registered<K: AssetManagerKind>(tag: &str) -> Result<(), Error> {
    let envref = env::<K>(opts(), vec![])?;
    let link = format!("seed-{tag}/wide-{tag}");
    for n in 1..=2 {
        let outer = format!("seed-{tag}{n}/cat-~X~{link}~E");
        assert_eq!(eval(&envref, &outer).await?, "seed+seedwide");
    }
    assert_eq!(ran("wide", tag), 2, "the uncached link is evaluated for each consumer");
    assert!(!registered(&envref, &link));
    Ok(())
}

/// A keyed asset that already exists — a value installed by `set_state` — is reused whatever the
/// recipe strategy, and a key with no recipe (plain data) is not subject to it at all: the
/// strategy governs recipe evaluations. (PR #102 review.)
async fn existing_keyed_asset_is_reused_under_recipe_none<K: AssetManagerKind>(
    tag: &str,
) -> Result<(), Error> {
    let envref = env::<K>(opts().with_recipe_cache_strategy(CacheStrategy::None), vec![])?;
    let manager = envref.get_asset_manager();
    let key = parse_key(&format!("src-{tag}.txt"))?;
    manager
        .set_state(&key, State::new().with_data(Value::from("installed")))
        .await?;
    let installed = manager.lookup_key_asset(&key).ok_or(Error::key_not_found(&key))?;
    let fetched = manager.get(&key).await?;
    assert_eq!(fetched.id(), installed.id(), "the live keyed asset is reused");
    assert_eq!(fetched.get().await?.try_into_string()?, "installed");
    Ok(())
}

// ---------------------------------------------------------------------------
// One test per scenario and manager
// ---------------------------------------------------------------------------

macro_rules! both_managers {
    ($scenario:ident, $default:ident, $immediate:ident, $tag:literal) => {
        #[tokio::test]
        async fn $default() -> Result<(), Error> {
            $scenario::<Queued>(concat!($tag, "q")).await
        }
        #[tokio::test]
        async fn $immediate() -> Result<(), Error> {
            $scenario::<Inline>(concat!($tag, "i")).await
        }
    };
}

both_managers!(uncached_command_runs_inline, uncached_command_runs_inline_default, uncached_command_runs_inline_immediate, "a1");
both_managers!(
    uncached_command_ending_an_adhoc_query_is_not_reused,
    uncached_command_ending_an_adhoc_query_is_not_reused_default,
    uncached_command_ending_an_adhoc_query_is_not_reused_immediate,
    "a2"
);
both_managers!(query_strategy_result_and_none, query_strategy_result_and_none_default, query_strategy_result_and_none_immediate, "a3");
both_managers!(existing_intermediate_is_reused, existing_intermediate_is_reused_default, existing_intermediate_is_reused_immediate, "a4");
both_managers!(recipe_strategy_values, recipe_strategy_values_default, recipe_strategy_values_immediate, "a5");
both_managers!(boundary_follows_its_creator, boundary_follows_its_creator_default, boundary_follows_its_creator_immediate, "a6");
both_managers!(
    keyed_result_ignores_the_command_flag,
    keyed_result_ignores_the_command_flag_default,
    keyed_result_ignores_the_command_flag_immediate,
    "a7"
);
both_managers!(cut_predecessors_false_expands, cut_predecessors_false_expands_default, cut_predecessors_false_expands_immediate, "a8");
both_managers!(uncached_link_is_not_registered, uncached_link_is_not_registered_default, uncached_link_is_not_registered_immediate, "a9");
both_managers!(
    existing_keyed_asset_is_reused_under_recipe_none,
    existing_keyed_asset_is_reused_under_recipe_none_default,
    existing_keyed_asset_is_reused_under_recipe_none_immediate,
    "a10"
);
