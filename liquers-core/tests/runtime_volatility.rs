//! Volatility discovered while a command runs: a volatile value a command reads through its
//! `Context`, or the command's own `Context::set_volatile`, makes its asset finish volatile.
//! Design: `specs/design/runtime-volatility/` (Phase 3). Every scenario runs on both built-in
//! managers: `DefaultAssetManager` (queued) and `ImmediateAssetManager` (inline).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use liquers_core::{
    assets::AssetManager,
    command_metadata::{ArgumentInfo, CommandKey},
    commands::CommandRegistry,
    context::{EnvRef, Environment, ImmediateEnvironment, SimpleEnvironment},
    error::Error,
    metadata::{DependencyKey, Metadata, Status},
    parse::{parse_key, parse_query},
    query::Key,
    recipes::DefaultRecipeProvider,
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Registers the commands of Phase 3:
/// `count` (volatile, `n0`, `n1`, …), `plain`, `depend_on(q)`, `read_counted` (reads the key
/// `n.txt`), `submit_wait(q)`, `evaluate_get(q)`, `apply_to(q)`, `submit_only(q)`,
/// `probe_volatile(q)` and `declare_volatile`.
fn register<E: Environment<Value = Value>>(cr: &mut CommandRegistry<E>) -> Result<(), Error> {
    let counter = Arc::new(AtomicUsize::new(0));
    cr.register_command(CommandKey::new_name("count"), move |_, _, _| {
        Ok(Value::from(format!("n{}", counter.fetch_add(1, Ordering::SeqCst))))
    })?
    .volatile = true;
    cr.register_command(CommandKey::new_name("plain"), |_, _, _| Ok(Value::from("plain")))?;
    cr.register_async_command(CommandKey::new_name("depend_on"), |_, args, ctx| {
        Box::pin(async move {
            let query: String = args.get(0, "query")?;
            let state = ctx.get_dependency_state(&parse_query(&query)?).await?;
            Ok(Value::from(state.try_into_string()?))
        })
    })?
    .with_argument(ArgumentInfo::string_argument("query"));
    cr.register_async_command(CommandKey::new_name("read_counted"), |_, _, ctx| {
        Box::pin(async move {
            let state = ctx.get_dependency_state(&parse_query("-R/n.txt")?).await?;
            Ok(Value::from(state.try_into_string()?))
        })
    })?;
    cr.register_async_command(CommandKey::new_name("submit_wait"), |_, args, ctx| {
        Box::pin(async move {
            let query: String = args.get(0, "query")?;
            let asset = ctx.submit(&parse_query(&query)?).await?;
            let state = ctx.wait_for_dependency(&asset).await?;
            Ok(Value::from(state.try_into_string()?))
        })
    })?
    .with_argument(ArgumentInfo::string_argument("query"));
    cr.register_async_command(CommandKey::new_name("evaluate_get"), |_, args, ctx| {
        Box::pin(async move {
            let query: String = args.get(0, "query")?;
            let asset = ctx.evaluate(&parse_query(&query)?).await?;
            // Bypasses `wait_for_dependency` on purpose.
            Ok(Value::from(asset.get().await?.try_into_string()?))
        })
    })?
    .with_argument(ArgumentInfo::string_argument("query"));
    cr.register_async_command(CommandKey::new_name("apply_to"), |_, args, ctx| {
        Box::pin(async move {
            let query: String = args.get(0, "query")?;
            let asset = ctx.apply(&parse_query(&query)?, State::new()).await?;
            Ok(Value::from(asset.get().await?.try_into_string()?))
        })
    })?
    .with_argument(ArgumentInfo::string_argument("query"));
    cr.register_async_command(CommandKey::new_name("submit_only"), |_, args, ctx| {
        Box::pin(async move {
            let query: String = args.get(0, "query")?;
            let _asset = ctx.submit(&parse_query(&query)?).await?;
            Ok(Value::from("submitted"))
        })
    })?
    .with_argument(ArgumentInfo::string_argument("query"));
    cr.register_async_command(CommandKey::new_name("probe_volatile"), |_, args, ctx| {
        Box::pin(async move {
            let query: String = args.get(0, "query")?;
            let before = ctx.is_volatile();
            ctx.get_dependency_state(&parse_query(&query)?).await?;
            Ok(Value::from(format!("{}/{}", before, ctx.is_volatile())))
        })
    })?
    .with_argument(ArgumentInfo::string_argument("query"));
    cr.register_async_command(CommandKey::new_name("declare_volatile"), |_, _, ctx| {
        Box::pin(async move {
            ctx.set_volatile("declared by the command").await?;
            ctx.set_volatile("declared twice").await?;
            Ok(Value::from("declared"))
        })
    })?;
    Ok(())
}

/// A store whose `recipes.yaml` maps `depend_on-count` to the key `n.txt`.
async fn store() -> Result<AsyncMemoryStore, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &parse_key("recipes.yaml")?,
            b"recipes:\n  - query: depend_on-count/n.txt\n",
            &Metadata::new(),
        )
        .await?;
    Ok(store)
}

async fn default_env() -> Result<EnvRef<SimpleEnvironment<Value>>, Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    register(&mut env.command_registry)?;
    env.with_async_store(Box::new(store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider::new()));
    Ok(env.to_ref())
}

async fn inline_env() -> Result<EnvRef<ImmediateEnvironment<Value>>, Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register(&mut env.command_registry)?;
    env.with_async_store(Box::new(store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider::new()));
    Ok(env.to_ref())
}

/// Evaluates `query` and returns its text, status and asset id.
async fn eval<E: Environment<Value = Value>>(
    envref: &EnvRef<E>,
    query: &str,
) -> Result<(String, Status, u64), Box<dyn std::error::Error>> {
    let asset = envref
        .get_asset_manager()
        .get_asset(&parse_query(query)?)
        .await?;
    let text = asset.get().await?.try_into_string()?;
    Ok((text, asset.status().await, asset.id()))
}

/// The messages of the asset's log entries that start with `Volatile:`.
async fn volatile_log<E: Environment<Value = Value>>(
    envref: &EnvRef<E>,
    query: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let asset = envref
        .get_asset_manager()
        .get_asset(&parse_query(query)?)
        .await?;
    asset.get().await?;
    let record = asset
        .get_metadata()
        .await?
        .metadata_record()
        .ok_or("legacy metadata")?;
    Ok(record
        .log
        .iter()
        .filter(|e| e.message.starts_with("Volatile:"))
        .map(|e| e.message.clone())
        .collect())
}

macro_rules! on_both_managers {
    ($scenario:ident, $default:ident, $inline:ident) => {
        #[tokio::test]
        async fn $default() -> TestResult {
            $scenario(default_env().await?).await
        }
        #[tokio::test]
        async fn $inline() -> TestResult {
            $scenario(inline_env().await?).await
        }
    };
}

/// AC-1: a volatile value read through the context makes the query volatile, and it is not
/// served again.
async fn rv01<E: Environment<Value = Value>>(envref: EnvRef<E>) -> TestResult {
    let (first, status, _) = eval(&envref, "depend_on-count").await?;
    assert_eq!((first.as_str(), status), ("n0", Status::Volatile));
    let (second, status, _) = eval(&envref, "depend_on-count").await?;
    assert_eq!((second.as_str(), status), ("n1", Status::Volatile));
    Ok(())
}
on_both_managers!(
    rv01,
    rv01_runtime_dependency_makes_query_volatile_default,
    rv01_runtime_dependency_makes_query_volatile_inline
);

/// AC-2: a trailing `v` no longer serves a stale prefix.
async fn rv02<E: Environment<Value = Value>>(envref: EnvRef<E>) -> TestResult {
    let (first, _, _) = eval(&envref, "depend_on-count").await?;
    let (second, status, _) = eval(&envref, "depend_on-count/v").await?;
    assert_ne!(first, second, "the prefix is recomputed");
    assert_eq!(status, Status::Volatile);
    Ok(())
}
on_both_managers!(
    rv02,
    rv02_trailing_v_recomputes_the_prefix_default,
    rv02_trailing_v_recomputes_the_prefix_inline
);

/// AC-3: a keyed asset is stored with `Status::Volatile`, carries no version, and is recomputed.
async fn rv03<E: Environment<Value = Value>>(envref: EnvRef<E>) -> TestResult {
    let key = parse_key("n.txt")?;
    let manager = envref.get_asset_manager();
    let first = manager.get(&key).await?;
    let first_text = first.get().await?.try_into_string()?;
    assert_eq!(first.status().await, Status::Volatile);
    assert!(
        first.get_metadata().await?.version().is_none(),
        "a volatile asset has no version, so it is not a dependency-graph node"
    );
    let stored = envref.get_async_store().get_metadata(&key).await?;
    assert_eq!(stored.status(), Status::Volatile, "the stored copy is not loadable");
    let second = manager.get(&key).await?;
    let second_text = second.get().await?.try_into_string()?;
    assert_ne!(second.id(), first.id(), "the volatile asset is not served again");
    assert_ne!(second_text, first_text, "the key is recomputed");
    Ok(())
}
on_both_managers!(
    rv03,
    rv03_keyed_runtime_volatile_is_stored_volatile_and_recomputed_default,
    rv03_keyed_runtime_volatile_is_stored_volatile_and_recomputed_inline
);

/// AC-4: reading a key that turned volatile at run time makes the reader volatile too.
async fn rv04<E: Environment<Value = Value>>(envref: EnvRef<E>) -> TestResult {
    let (first, status, _) = eval(&envref, "read_counted").await?;
    assert_eq!(status, Status::Volatile);
    let (second, status, _) = eval(&envref, "read_counted").await?;
    assert_eq!(status, Status::Volatile);
    assert_ne!(first, second);
    Ok(())
}
on_both_managers!(
    rv04,
    rv04_runtime_volatility_is_transitive_default,
    rv04_runtime_volatility_is_transitive_inline
);

/// AC-5: every context entry point passes volatility on, including waiting with
/// `AssetRef::get` (`apply_ad_hoc` is covered in `tests/apply_ad_hoc.rs`).
async fn rv05<E: Environment<Value = Value>>(envref: EnvRef<E>) -> TestResult {
    for query in ["submit_wait-count", "evaluate_get-count", "apply_to-count"] {
        let (_, status, _) = eval(&envref, query).await?;
        assert_eq!(status, Status::Volatile, "{query}");
    }
    Ok(())
}
on_both_managers!(
    rv05,
    rv05_every_context_entry_point_passes_volatility_default,
    rv05_every_context_entry_point_passes_volatility_inline
);

/// AC-6: a non-volatile run-time dependency changes nothing.
async fn rv06<E: Environment<Value = Value>>(envref: EnvRef<E>) -> TestResult {
    let asset = envref
        .get_asset_manager()
        .get_asset(&parse_query("depend_on-plain")?)
        .await?;
    assert_eq!(asset.get().await?.try_into_string()?, "plain");
    assert_eq!(asset.status().await, Status::Ready);
    let plain = DependencyKey::from(&parse_query("plain")?);
    let deps = asset.get_metadata().await?.get_dependencies().to_vec();
    assert!(deps.iter().any(|d| d.key == plain), "recorded as today: {deps:?}");
    assert!(volatile_log(&envref, "depend_on-plain").await?.is_empty());
    Ok(())
}
on_both_managers!(
    rv06,
    rv06_non_volatile_dependency_stays_ready_default,
    rv06_non_volatile_dependency_stays_ready_inline
);

/// AC-7: the cause is logged once.
async fn rv07<E: Environment<Value = Value>>(envref: EnvRef<E>) -> TestResult {
    let log = volatile_log(&envref, "depend_on-count").await?;
    assert_eq!(log.len(), 1, "{log:?}");
    assert!(log[0].contains("count"), "{log:?}");
    Ok(())
}
on_both_managers!(
    rv07,
    rv07_volatile_cause_is_logged_default,
    rv07_volatile_cause_is_logged_inline
);

/// AC-8: `Context::is_volatile` turns true once a volatile dependency has been read.
async fn rv08<E: Environment<Value = Value>>(envref: EnvRef<E>) -> TestResult {
    assert_eq!(eval(&envref, "probe_volatile-count").await?.0, "false/true");
    assert_eq!(eval(&envref, "probe_volatile-plain").await?.0, "false/false");
    Ok(())
}
on_both_managers!(
    rv08,
    rv08_context_is_volatile_after_volatile_dependency_default,
    rv08_context_is_volatile_after_volatile_dependency_inline
);

/// AC-9: a command declares its result volatile; the reason is logged once.
async fn rv09<E: Environment<Value = Value>>(envref: EnvRef<E>) -> TestResult {
    let (text, status, first_id) = eval(&envref, "declare_volatile").await?;
    assert_eq!((text.as_str(), status), ("declared", Status::Volatile));
    let (_, _, second_id) = eval(&envref, "declare_volatile").await?;
    assert_ne!(first_id, second_id, "not served again");
    let log = volatile_log(&envref, "declare_volatile").await?;
    assert_eq!(log, vec!["Volatile: declared by the command".to_string()]);
    Ok(())
}
on_both_managers!(
    rv09,
    rv09_command_can_declare_volatile_default,
    rv09_command_can_declare_volatile_inline
);

/// AC-6: a non-volatile dependency submitted and never waited for leaves the parent `Ready`.
async fn rv10<E: Environment<Value = Value>>(envref: EnvRef<E>) -> TestResult {
    let (text, status, _) = eval(&envref, "submit_only-plain").await?;
    assert_eq!((text.as_str(), status), ("submitted", Status::Ready));
    Ok(())
}
on_both_managers!(
    rv10,
    rv10_unwaited_non_volatile_dependency_keeps_parent_ready_default,
    rv10_unwaited_non_volatile_dependency_keeps_parent_ready_inline
);
