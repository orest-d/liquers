//! Cost of evaluating a chain of keyed recipes
//! (`specs/design/dependency-chain-analysis-cost/`, Phase 3 I6 / I7).
//!
//! `data/recipes.yaml` holds `l0.txt = make_text` and `l{i}.txt = upper(l{i-1}.txt)`; the links are
//! evaluated one at a time, as in the issue `EVALUATING-A-LONG-DEPENDENCY-CHAIN-GETS-SUPER-LINEARLY-SLOW`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use liquers_core::{
    command_metadata::CommandKey,
    context::{EnvRef, Environment, SimpleEnvironment},
    error::Error,
    metadata::Metadata,
    parse::parse_key,
    plan::Plan,
    query::{Key, ResourceName},
    recipes::{AsyncRecipeProvider, DefaultRecipeProvider, Recipe, RecipeList},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};

type TestEnv = SimpleEnvironment<Value>;

/// Delegates to [`DefaultRecipeProvider`], counting `recipe_opt` calls.
struct CountingProvider {
    inner: DefaultRecipeProvider,
    recipe_opt_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl AsyncRecipeProvider<TestEnv> for CountingProvider {
    async fn has_recipes(&self, key: &Key, envref: EnvRef<TestEnv>) -> Result<bool, Error> {
        self.inner.has_recipes(key, envref).await
    }
    async fn assets_with_recipes(
        &self,
        key: &Key,
        envref: EnvRef<TestEnv>,
    ) -> Result<Vec<ResourceName>, Error> {
        self.inner.assets_with_recipes(key, envref).await
    }
    async fn recipe_plan(&self, key: &Key, envref: EnvRef<TestEnv>) -> Result<Plan, Error> {
        self.inner.recipe_plan(key, envref).await
    }
    async fn recipe(&self, key: &Key, envref: EnvRef<TestEnv>) -> Result<Recipe, Error> {
        self.inner.recipe(key, envref).await
    }
    async fn recipe_opt(&self, key: &Key, envref: EnvRef<TestEnv>) -> Result<Option<Recipe>, Error> {
        self.recipe_opt_calls.fetch_add(1, Ordering::Relaxed);
        self.inner.recipe_opt(key, envref).await
    }
}

/// A fresh environment over a store holding the recipes of an `n`-link chain (`l0..=ln`).
async fn chain_env(n: usize) -> Result<(EnvRef<TestEnv>, Arc<AtomicUsize>), Box<dyn std::error::Error>> {
    let mut rl = RecipeList::new();
    rl.add_recipe(Recipe::new("make_text/l0.txt".into(), "l0".into(), String::new())?);
    for i in 1..=n {
        rl.add_recipe(Recipe::new(
            format!("-R/data/l{}.txt/-/upper/l{i}.txt", i - 1),
            format!("l{i}"),
            String::new(),
        )?);
    }
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(&parse_key("data/recipes.yaml")?, serde_yaml::to_string(&rl)?.as_bytes(), &Metadata::new())
        .await?;

    let mut env = TestEnv::new();
    env.command_registry.register_command(
        CommandKey::new_name("upper"),
        |state: &State<Value>, _args, _ctx| -> Result<Value, Error> {
            Ok(Value::from(state.try_into_string()?.to_uppercase()))
        },
    )?;
    env.command_registry.register_command(
        CommandKey::new_name("make_text"),
        |_state, _args, _ctx| -> Result<Value, Error> { Ok(Value::from("text")) },
    )?;
    env.with_async_store(Box::new(store));
    let calls = Arc::new(AtomicUsize::new(0));
    env.with_recipe_provider(Box::new(CountingProvider {
        inner: DefaultRecipeProvider,
        recipe_opt_calls: calls.clone(),
    }));
    Ok((env.to_ref(), calls))
}

/// Evaluate `l0..=ln` one at a time; the elapsed time and the total `recipe_opt` calls.
async fn evaluate_chain(n: usize) -> Result<(Duration, usize), Box<dyn std::error::Error>> {
    let (envref, calls) = chain_env(n).await?;
    let start = Instant::now();
    for i in 0..=n {
        let asset = envref.evaluate(&format!("-R/data/l{i}.txt")).await?;
        asset.get().await?;
    }
    Ok((start.elapsed(), calls.load(Ordering::Relaxed)))
}

fn chain_sizes() -> Vec<usize> {
    std::env::var("CHAIN_SIZES")
        .ok()
        .map(|s| s.split(',').filter_map(|n| n.trim().parse().ok()).collect())
        .unwrap_or_else(|| vec![10, 20, 40])
}

/// I6: the scaling benchmark. Ignored by default: run with
/// `cargo test -p liquers-core --test dependency_chain_scaling -- --ignored --nocapture`.
#[tokio::test]
#[ignore]
async fn chain_evaluation_scales() -> Result<(), Box<dyn std::error::Error>> {
    for n in chain_sizes() {
        let (elapsed, lookups) = evaluate_chain(n).await?;
        eprintln!(
            "chain of {n:>4} links: {:>9.3} s, {lookups:>8} recipe lookups",
            elapsed.as_secs_f64()
        );
    }
    Ok(())
}
