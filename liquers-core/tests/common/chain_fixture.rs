//! Keyed-recipe chains with counting commands, on either asset-manager kind
//! (`specs/design/dependency-resolved-once-per-evaluation/`, Phase 3).
//!
//! Recipes live in `data/recipes.yaml` of a memory store and are served by the
//! `DefaultRecipeProvider`, so a recipe's query ends with its file name (`-R/data/l0.txt/-/upper/l1.txt`
//! defines `data/l1.txt`). Every command counts its runs in [`Counters`].

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use liquers_core::{
    command_metadata::{ArgumentInfo, CommandKey},
    context::{EnvRef, Environment, GenericEnvironment},
    environment_builder::{AssetManagerKind, AssetManagerOptions, EnvironmentBuilder},
    error::Error,
    metadata::Metadata,
    parse::{parse_key, parse_query},
    query::Key,
    recipes::{DefaultRecipeProvider, Recipe, RecipeList},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};

pub type Env<K> = GenericEnvironment<Value, (), K>;

/// A gate that stays open once opened, so no waiter is ever stranded.
#[derive(Clone, Default)]
pub struct Latch {
    open: Arc<AtomicBool>,
    notify: Arc<tokio::sync::Notify>,
}

impl Latch {
    pub fn open(&self) {
        self.open.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub async fn wait(&self) {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.open.load(Ordering::SeqCst) {
                return;
            }
            notified.await;
        }
    }
}

/// Run counts of the fixture commands, and the latch the gated ones wait on.
#[derive(Clone, Default)]
pub struct Counters {
    pub fail: Arc<AtomicUsize>,
    pub make_text: Arc<AtomicUsize>,
    pub upper: Arc<AtomicUsize>,
    pub join: Arc<AtomicUsize>,
    pub tick: Arc<AtomicUsize>,
    pub y: Arc<AtomicUsize>,
    /// Entries into `gated_fail` / `gated_text`, counted before they wait.
    pub gated: Arc<AtomicUsize>,
    pub latch: Latch,
}

pub fn count(counter: &Arc<AtomicUsize>) -> usize {
    counter.load(Ordering::SeqCst)
}

/// One recipe: its query (ending with the file name), and whether the recipe is volatile.
pub struct R {
    pub query: String,
    pub volatile: bool,
}

pub fn r(query: impl Into<String>) -> R {
    R { query: query.into(), volatile: false }
}

pub fn volatile(query: impl Into<String>) -> R {
    R { query: query.into(), volatile: true }
}

/// `l0.txt = <root>`, `l{i}.txt = -R/data/l{i-1}.txt/-/upper` for `i` in `1..=n`.
pub fn chain(root: &str, n: usize) -> Vec<R> {
    let mut recipes = vec![r(format!("{root}/l0.txt"))];
    for i in 1..=n {
        recipes.push(r(format!("-R/data/l{}.txt/-/upper/l{i}.txt", i - 1)));
    }
    recipes
}

/// `r.txt = fail`, `a0`/`b0 = upper(r)`, `a{i}`/`b{i} = join(a{i-1}, b{i-1})`.
pub fn lattice(n: usize) -> Vec<R> {
    let mut recipes = vec![
        r("fail/r.txt"),
        r("-R/data/r.txt/-/upper/a0.txt"),
        r("-R/data/r.txt/-/upper/b0.txt"),
    ];
    for i in 1..=n {
        for x in ["a", "b"] {
            recipes.push(r(format!(
                "-R/data/a{p}.txt/-/join-~X~-R/data/b{p}.txt~E/{x}{i}.txt",
                p = i - 1
            )));
        }
    }
    recipes
}

/// Writes `recipes` to a fresh memory store as `data/recipes.yaml`.
pub async fn recipe_store(recipes: &[R]) -> Result<Arc<AsyncMemoryStore>, Box<dyn std::error::Error>> {
    let mut list = RecipeList::new();
    for (i, recipe) in recipes.iter().enumerate() {
        let mut entry = Recipe::new(recipe.query.clone(), format!("recipe {i}"), String::new())?;
        entry.volatile = recipe.volatile;
        list.add_recipe(entry);
    }
    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    store
        .set(
            &parse_key("data/recipes.yaml")?,
            serde_yaml::to_string(&list)?.as_bytes(),
            &Metadata::new(),
        )
        .await?;
    Ok(store)
}

/// An environment of kind `K` over `store`, with the fixture commands registered.
pub fn environment<K: AssetManagerKind>(
    store: Arc<AsyncMemoryStore>,
    capacity: Option<usize>,
    counters: &Counters,
) -> Result<EnvRef<Env<K>>, Box<dyn std::error::Error>> {
    let mut options = AssetManagerOptions::default();
    if let Some(capacity) = capacity {
        options = options.with_job_capacity(capacity);
    }
    let mut builder = EnvironmentBuilder::<Value, (), K>::new()
        .with_asset_manager_options(options)
        .with_async_store(store)
        .with_recipe_provider(Arc::new(DefaultRecipeProvider::new()));
    register(&mut builder, counters)?;
    Ok(builder.build()?)
}

/// `environment` over a fresh store holding `recipes`.
pub async fn env_with<K: AssetManagerKind>(
    recipes: &[R],
    capacity: Option<usize>,
) -> Result<(EnvRef<Env<K>>, Counters), Box<dyn std::error::Error>> {
    let counters = Counters::default();
    let store = recipe_store(recipes).await?;
    let envref = environment::<K>(store, capacity, &counters)?;
    Ok((envref, counters))
}

fn register<K: AssetManagerKind>(
    builder: &mut EnvironmentBuilder<Value, (), K>,
    counters: &Counters,
) -> Result<(), Error> {
    let cr = &mut builder.command_registry;

    let c = counters.fail.clone();
    cr.register_command(CommandKey::new_name("fail"), move |_state, _args, _ctx| -> Result<Value, Error> {
        c.fetch_add(1, Ordering::SeqCst);
        Err(Error::general_error("leaf failed".to_string()))
    })?;

    let c = counters.make_text.clone();
    cr.register_command(CommandKey::new_name("make_text"), move |_state, _args, _ctx| -> Result<Value, Error> {
        c.fetch_add(1, Ordering::SeqCst);
        Ok(Value::from("text"))
    })?;

    let c = counters.upper.clone();
    cr.register_command(
        CommandKey::new_name("upper"),
        move |state: &State<Value>, _args, _ctx| -> Result<Value, Error> {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(Value::from(state.try_into_string()?.to_uppercase()))
        },
    )?;

    let c = counters.join.clone();
    cr.register_command(
        CommandKey::new_name("join"),
        move |state: &State<Value>, args, _ctx| -> Result<Value, Error> {
            c.fetch_add(1, Ordering::SeqCst);
            let other: String = args.get(0, "other")?;
            Ok(Value::from(format!("{}+{}", state.try_into_string()?, other)))
        },
    )?
    .with_argument(ArgumentInfo::string_argument("other"));

    let c = counters.tick.clone();
    cr.register_command(CommandKey::new_name("tick"), move |_state, _args, _ctx| -> Result<Value, Error> {
        let n = c.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(Value::from(format!("tick{n}")))
    })?
    .volatile = true;

    let c = counters.y.clone();
    cr.register_command(CommandKey::new_name("y_cmd"), move |_state, _args, _ctx| -> Result<Value, Error> {
        c.fetch_add(1, Ordering::SeqCst);
        Ok(Value::from("y"))
    })?;

    let (c, f, latch) = (counters.gated.clone(), counters.fail.clone(), counters.latch.clone());
    cr.register_async_command(CommandKey::new_name("gated_fail"), move |_state, _args, _ctx| {
        let (c, f, latch) = (c.clone(), f.clone(), latch.clone());
        Box::pin(async move {
            c.fetch_add(1, Ordering::SeqCst);
            latch.wait().await;
            f.fetch_add(1, Ordering::SeqCst);
            Err(Error::general_error("leaf failed".to_string()))
        })
    })?;

    let (c, latch) = (counters.gated.clone(), counters.latch.clone());
    cr.register_async_command(CommandKey::new_name("gated_text"), move |_state, _args, _ctx| {
        let (c, latch) = (c.clone(), latch.clone());
        Box::pin(async move {
            c.fetch_add(1, Ordering::SeqCst);
            latch.wait().await;
            Ok(Value::from("text"))
        })
    })?;

    // `loop_cmd` requests its own key: a cycle visible only at run time.
    cr.register_async_command(CommandKey::new_name("loop_cmd"), move |_state, _args, ctx| {
        Box::pin(async move {
            let state = ctx.get_dependency_state(&parse_query("-R/data/k.txt")?).await?;
            Ok(Value::from(state.try_into_string()?))
        })
    })?;

    // `read_twice`: requests `l0` twice in one evaluation and reports both outcomes.
    cr.register_async_command(CommandKey::new_name("read_twice"), move |_state, _args, ctx| {
        Box::pin(async move {
            let query = parse_query("-R/data/l0.txt")?;
            let first = ctx.get_dependency_state(&query).await;
            let second = ctx.get_dependency_state(&query).await;
            Ok(Value::from(format!(
                "{}|{}",
                outcome_text(first.and_then(|s| s.value_state())),
                outcome_text(second.and_then(|s| s.value_state()))
            )))
        })
    })?;
    Ok(())
}

fn outcome_text(result: Result<State<Value>, Error>) -> String {
    match result {
        Ok(state) => format!("ok:{}", state.try_into_string().unwrap_or_default()),
        Err(error) => format!("err:{error}"),
    }
}

/// Evaluates `query` and returns its text or its error message, on either manager: the queued
/// manager returns a failed asset, the inline manager returns its error.
pub async fn outcome<E: Environment<Value = Value>>(
    envref: &EnvRef<E>,
    query: &str,
) -> Result<String, String> {
    let asset = envref.evaluate(query).await.map_err(|e| e.to_string())?;
    let state = asset.get().await.map_err(|e| e.to_string())?;
    let state = state.value_state().map_err(|e| e.to_string())?;
    state.try_into_string().map_err(|e| e.to_string())
}
