//! Value accessors in an environment: the store, keyed-asset and asset roots, and the query steps.
//!
//! Design: `specs/design/settable-value-accessor/` (Phase 3, core integration tests). Events go
//! through a `watch` channel, which keeps only the latest, so the tests wait for an event of a
//! kind with [`wait_for`] rather than counting events.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use liquers_core::{
    accessor::{
        asset_accessor, key_accessor, store_accessor, AccessorEvent, AccessorHub, ExpiryPolicy,
        KeyRoot, SetOptions, StoreView, ValueAccessor,
    },
    assets::AssetManager,
    command_metadata::CommandKey,
    context::{EnvRef, Environment, SimpleEnvironment},
    error::ErrorType,
    metadata::{Metadata, MetadataRecord, ProgressEntry, Status},
    parse::{parse_key, parse_query},
    query::{Key, Query},
    recipes::{DefaultRecipeProvider, Recipe, RecipeList},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::{Value, ValueInterface},
};
use tokio::sync::{watch, Notify};

type Env = SimpleEnvironment<Value>;

/// Fails fast instead of hanging.
async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(10), future)
        .await
        .expect("did not finish within 10 s")
}

/// Waits until the latest event satisfies `wanted`.
async fn wait_for(
    receiver: &mut watch::Receiver<AccessorEvent>,
    wanted: impl Fn(&AccessorEvent) -> bool,
) -> AccessorEvent {
    within(async {
        loop {
            {
                let event = receiver.borrow_and_update();
                if wanted(&event) {
                    return event.clone();
                }
            }
            receiver.changed().await.expect("the accessor was dropped");
        }
    })
    .await
}

fn is_changed(event: &AccessorEvent) -> bool {
    matches!(event, AccessorEvent::Changed(_))
}

fn text(value: &str) -> State<Value> {
    State::from_value_and_metadata(Value::from(value), Arc::new(Metadata::new()))
}

async fn get_text(accessor: &Arc<dyn ValueAccessor<Value>>) -> String {
    within(accessor.get())
        .await
        .expect("get")
        .state
        .try_into_string()
        .expect("text")
}

fn text_record() -> MetadataRecord {
    let mut record = MetadataRecord::new();
    record.type_identifier = "Text".to_string();
    record.type_name = "text".to_string();
    record.data_format = Some("txt".to_string());
    record
}

/// An environment with `make_text`, `upper` and a counting `count`, the given root-level recipes,
/// and `x.txt` (a `Source`, "stored") in the store.
async fn env_with(recipes: &[(&str, &str)]) -> (EnvRef<Env>, Arc<AtomicUsize>) {
    let counter = Arc::new(AtomicUsize::new(0));
    let mut env: Env = SimpleEnvironment::new();
    env.command_registry
        .register_command(CommandKey::new_name("make_text"), |_, _, _| {
            Ok(Value::from("generated"))
        })
        .unwrap();
    env.command_registry
        .register_command(CommandKey::new_name("upper"), |state: &State<Value>, _, _| {
            Ok(Value::from(state.try_into_string()?.to_uppercase()))
        })
        .unwrap();
    let c = counter.clone();
    env.command_registry
        .register_command(CommandKey::new_name("count"), move |_, _, _| {
            Ok(Value::from(format!("{}", c.fetch_add(1, Ordering::SeqCst) + 1)))
        })
        .unwrap();
    let store = AsyncMemoryStore::new(&Key::new());
    let mut list = RecipeList::new();
    for (query, title) in recipes {
        list.add_recipe(Recipe::new(query.to_string(), title.to_string(), String::new()).unwrap());
    }
    store
        .set(
            &parse_key("recipes.yaml").unwrap(),
            serde_yaml::to_string(&list).unwrap().as_bytes(),
            &Metadata::new(),
        )
        .await
        .unwrap();
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider::new()));
    let envref = env.to_ref();
    envref
        .get_asset_manager()
        .set_binary(&parse_key("x.txt").unwrap(), b"stored", text_record())
        .await
        .unwrap();
    (envref, counter)
}

fn key(text: &str) -> Key {
    parse_key(text).unwrap()
}

fn address(text: &str) -> Query {
    parse_query(text).unwrap()
}

#[tokio::test]
async fn outside_writer_is_detected() {
    let (envref, _) = env_with(&[]).await;
    let accessor = key_accessor(envref.clone(), key("x.txt"), address("-R/x.txt"));
    assert_eq!(get_text(&accessor).await, "stored");
    let mut events = accessor.subscribe();
    envref
        .get_asset_manager()
        .set_state(&key("x.txt"), text("outside"))
        .await
        .unwrap();
    wait_for(&mut events, is_changed).await;
    assert_eq!(get_text(&accessor).await, "outside");
}

#[tokio::test]
async fn store_root_sees_a_direct_write_on_refresh() {
    let (envref, _) = env_with(&[]).await;
    let accessor = store_accessor(
        envref.clone(),
        key("x.txt"),
        StoreView::Value,
        address("-R/x.txt"),
    );
    assert_eq!(get_text(&accessor).await, "stored");
    let mut events = accessor.subscribe();
    envref
        .get_async_store()
        .set(
            &key("x.txt"),
            b"direct",
            &Metadata::MetadataRecord(text_record()),
        )
        .await
        .unwrap();
    within(accessor.refresh()).await.unwrap();
    assert!(is_changed(&events.borrow_and_update()));
    assert_eq!(get_text(&accessor).await, "direct");
}

#[tokio::test]
async fn expiry_and_removal_are_reported() {
    let (envref, _) = env_with(&[("make_text/t.txt", "T")]).await;
    let manager = envref.get_asset_manager();

    let made = key_accessor(envref.clone(), key("t.txt"), address("-R/t.txt"));
    assert_eq!(get_text(&made).await, "generated");
    let mut events = made.subscribe();
    within(manager.expire(&key("t.txt"))).await.unwrap();
    wait_for(&mut events, |e| *e == AccessorEvent::Status(Status::Expired)).await;

    let source = key_accessor(envref.clone(), key("x.txt"), address("-R/x.txt"));
    assert_eq!(get_text(&source).await, "stored");
    let mut events = source.subscribe();
    within(manager.remove(&key("x.txt"))).await.unwrap();
    wait_for(&mut events, |e| *e == AccessorEvent::Removed).await;
    assert_eq!(within(source.info()).await.unwrap().status, Status::None);
    let error = within(source.get()).await.unwrap_err();
    assert_eq!(error.error_type, ErrorType::AssetNotFound);
    // The root accessor stays usable: a set creates the key again.
    within(source.set(Value::from("again"), SetOptions::default()))
        .await
        .unwrap();
    assert_eq!(get_text(&source).await, "again");
    assert_eq!(within(source.info()).await.unwrap().status, Status::Source);
}

#[tokio::test]
async fn keyed_asset_set_overrides_sources_or_creates() {
    let (envref, _) = env_with(&[
        ("make_text/t.txt", "T"),
        ("-R/t.txt/-/upper/u.txt", "U"),
        ("make_text/t2.txt", "T2"),
    ])
    .await;
    let manager = envref.get_asset_manager();

    // With a recipe: Override, and the dependent expires.
    let dependent = within(manager.get(&key("u.txt"))).await.unwrap();
    assert_eq!(within(dependent.get()).await.unwrap().try_into_string().unwrap(), "GENERATED");
    let made = key_accessor(envref.clone(), key("t.txt"), address("-R/t.txt"));
    assert_eq!(get_text(&made).await, "generated");
    within(made.set(Value::from("mine"), SetOptions::default()))
        .await
        .unwrap();
    assert_eq!(within(made.info()).await.unwrap().status, Status::Override);
    assert_eq!(get_text(&made).await, "mine");
    let u = within(manager.get(&key("u.txt"))).await.unwrap();
    assert_eq!(within(u.get()).await.unwrap().try_into_string().unwrap(), "MINE");

    // Without a recipe: Source.
    let source = key_accessor(envref.clone(), key("x.txt"), address("-R/x.txt"));
    within(source.set(Value::from("changed"), SetOptions::default()))
        .await
        .unwrap();
    assert_eq!(within(source.info()).await.unwrap().status, Status::Source);

    // A missing key: status None, created as Source; create-only on an existing key conflicts.
    let fresh = key_accessor(envref.clone(), key("new.txt"), address("-R/new.txt"));
    assert_eq!(within(fresh.info()).await.unwrap().status, Status::None);
    within(fresh.set_if(Value::from("created"), None, SetOptions::default()))
        .await
        .unwrap();
    assert_eq!(get_text(&fresh).await, "created");
    assert_eq!(within(fresh.info()).await.unwrap().status, Status::Source);
    let conflict = within(fresh.set_if(Value::from("again"), None, SetOptions::default()))
        .await
        .unwrap_err();
    assert_eq!(conflict.error_type, ErrorType::StatusConflict);

    // A set after an expiry stores Override, not the Expired status read back.
    let expiring = key_accessor(envref.clone(), key("t2.txt"), address("-R/t2.txt"));
    assert_eq!(get_text(&expiring).await, "generated");
    within(manager.expire(&key("t2.txt"))).await.unwrap();
    within(expiring.set(Value::from("after expiry"), SetOptions::default()))
        .await
        .unwrap();
    assert_eq!(within(expiring.info()).await.unwrap().status, Status::Override);

    // Any other asset is read-only.
    let query_asset = within(envref.evaluate("make_text")).await.unwrap();
    let read_only = asset_accessor(query_asset, address("make_text"));
    assert_eq!(get_text(&read_only).await, "generated");
    assert!(!read_only.capabilities().write);
    let refused = within(read_only.set(Value::from("no"), SetOptions::default()))
        .await
        .unwrap_err();
    assert_eq!(refused.error_type, ErrorType::NotSupported);
    let applied = within(manager.apply(
        Recipe::new("upper".to_string(), String::new(), String::new()).unwrap(),
        text("abc"),
        None,
    ))
    .await
    .unwrap();
    let unmanaged = asset_accessor(applied, address("upper"));
    assert_eq!(get_text(&unmanaged).await, "ABC");
    assert!(within(unmanaged.set(Value::from("no"), SetOptions::default()))
        .await
        .is_err());
}

#[tokio::test]
async fn expired_asset_get_re_evaluates() {
    let (envref, counter) = env_with(&[("count/c.txt", "C")]).await;
    let manager = envref.get_asset_manager();
    let accessor = key_accessor(envref.clone(), key("c.txt"), address("-R/c.txt"));
    assert_eq!(get_text(&accessor).await, "1");
    let mut events = accessor.subscribe();

    within(manager.expire(&key("c.txt"))).await.unwrap();
    wait_for(&mut events, |e| *e == AccessorEvent::Status(Status::Expired)).await;
    let drawn = within(accessor.info()).await.unwrap().fingerprint;
    assert_eq!(get_text(&accessor).await, "2");
    // `Changed` is sent, but later status events may replace it in the channel: a holder compares
    // fingerprints.
    assert_ne!(within(accessor.info()).await.unwrap().fingerprint, drawn);

    // A second expiry is seen too: the root follows the new asset.
    within(manager.expire(&key("c.txt"))).await.unwrap();
    wait_for(&mut events, |e| *e == AccessorEvent::Status(Status::Expired)).await;
    assert_eq!(get_text(&accessor).await, "3");

    // With the reload policy, the new value arrives with no get.
    let reloading = AccessorHub::new(
        Arc::new(KeyRoot::new(&envref, key("c.txt"))),
        address("-R/c.txt"),
        ExpiryPolicy::Reload,
    );
    assert_eq!(get_text(&reloading).await, "3");
    let drawn = within(reloading.info()).await.unwrap().fingerprint;
    within(manager.expire(&key("c.txt"))).await.unwrap();
    within(async {
        while reloading.info().await.unwrap().fingerprint == drawn {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert_eq!(counter.load(Ordering::SeqCst), 4, "reloaded with no get");
    assert_eq!(get_text(&reloading).await, "4");
}

#[tokio::test]
async fn store_views_and_a_missing_key() {
    let (envref, _) = env_with(&[]).await;
    let store = envref.get_async_store();
    let mut record = MetadataRecord::new();
    record.title = "A document".to_string();
    store
        .set(
            &key("doc.json"),
            br#"{"a":1}"#,
            &Metadata::MetadataRecord(record),
        )
        .await
        .unwrap();
    let view = |view: StoreView, k: &str| {
        store_accessor(envref.clone(), key(k), view, address(&format!("-R/{k}")))
    };

    // The three views of a stored document.
    let binary = view(StoreView::Binary, "doc.json");
    let metadata = view(StoreView::Metadata, "doc.json");
    let value = view(StoreView::Value, "doc.json");
    let bytes = within(binary.get()).await.unwrap().state.value().unwrap();
    assert_eq!(bytes.try_into_bytes().unwrap(), br#"{"a":1}"#.to_vec());
    let meta = within(metadata.get()).await.unwrap().state.value().unwrap();
    assert!(matches!(&*meta, Value::Metadata(_)));
    let object = within(value.get()).await.unwrap().state.value().unwrap();
    assert_eq!(
        object.try_into_json_value().unwrap(),
        serde_json::json!({"a": 1})
    );

    // A value-view set keeps the title; a metadata-view set keeps the bytes.
    within(value.set(
        Value::from_object(
            [("a".to_string(), Value::from(2))].into_iter().collect(),
        )
        .unwrap(),
        SetOptions::default(),
    ))
    .await
    .unwrap();
    let stored = store.get_metadata(&key("doc.json")).await.unwrap();
    assert_eq!(stored.title(), "A document");
    // The value-view set changed the metadata too: this handle reads it again before its own set.
    within(metadata.get()).await.unwrap();
    let mut record = stored.metadata_record().unwrap();
    record.title = "Renamed".to_string();
    within(metadata.set(Value::from_metadata(record), SetOptions::default()))
        .await
        .unwrap();
    assert_eq!(
        store.get_metadata(&key("doc.json")).await.unwrap().title(),
        "Renamed"
    );
    assert_eq!(
        store.get_bytes(&key("doc.json")).await.unwrap(),
        br#"{"a":2}"#.to_vec()
    );

    // A missing key: status None, not found, created through the binary view.
    let missing_binary = view(StoreView::Binary, "blob.bin");
    assert_eq!(within(missing_binary.info()).await.unwrap().status, Status::None);
    assert_eq!(
        within(missing_binary.get()).await.unwrap_err().error_type,
        ErrorType::KeyNotFound
    );
    let missing_metadata = view(StoreView::Metadata, "blob.bin");
    let refused = within(missing_metadata.set(
        Value::from_metadata(MetadataRecord::new()),
        SetOptions::default(),
    ))
    .await
    .unwrap_err();
    assert_eq!(refused.error_type, ErrorType::KeyNotFound);
    assert!(!store.contains(&key("blob.bin")).await.unwrap());
    within(missing_binary.set(Value::from_bytes(vec![1, 2, 3]), SetOptions::default()))
        .await
        .unwrap();
    assert_eq!(within(missing_binary.info()).await.unwrap().status, Status::Source);
    for view_kind in [StoreView::Binary, StoreView::Metadata, StoreView::Value] {
        assert!(within(view(view_kind, "blob.bin").get()).await.is_ok());
    }
}

#[tokio::test]
async fn progress_and_log_are_relayed() {
    let notify = Arc::new(Notify::new());
    let mut env: Env = SimpleEnvironment::new();
    let n = notify.clone();
    env.command_registry
        .register_async_command(CommandKey::new_name("slow"), move |_, _, context| {
            let notify = n.clone();
            Box::pin(async move {
                context.progress(ProgressEntry::new("primary".to_string(), 1, 2))?;
                context.secondary_progress(ProgressEntry::new("secondary".to_string(), 3, 4))?;
                context.info("working")?;
                notify.notified().await;
                Ok(Value::from("done"))
            })
        })
        .unwrap();
    let store = AsyncMemoryStore::new(&Key::new());
    let mut list = RecipeList::new();
    list.add_recipe(Recipe::new("slow/s.txt".to_string(), "S".to_string(), String::new()).unwrap());
    store
        .set(
            &key("recipes.yaml"),
            serde_yaml::to_string(&list).unwrap().as_bytes(),
            &Metadata::new(),
        )
        .await
        .unwrap();
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider::new()));
    let envref = env.to_ref();

    let accessor = key_accessor(envref.clone(), key("s.txt"), address("-R/s.txt"));
    let mut events = accessor.subscribe();
    let getter = {
        let accessor = accessor.clone();
        tokio::spawn(async move { accessor.get().await })
    };
    // The relayed events: any of progress, log or status shows the asset is being followed.
    wait_for(&mut events, |e| {
        matches!(
            e,
            AccessorEvent::PrimaryProgress(_)
                | AccessorEvent::SecondaryProgress(_)
                | AccessorEvent::Log(_)
                | AccessorEvent::Status(_)
        )
    })
    .await;
    within(async {
        loop {
            let info = accessor.info().await.unwrap();
            if info.log_len > 0
                && info.primary_progress.as_ref().map(|p| p.done) == Some(1)
                && info.secondary_progress.as_ref().map(|p| p.done) == Some(3)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    let log = within(accessor.log(0)).await.unwrap();
    assert!(log.iter().any(|entry| entry.message == "working"));
    let drawn = within(accessor.info()).await.unwrap().fingerprint;
    notify.notify_one();
    let snapshot = within(getter).await.unwrap().unwrap();
    assert_eq!(snapshot.state.try_into_string().unwrap(), "done");
    // A holder that missed `Changed` in a burst sees it in the fingerprint.
    assert_ne!(within(accessor.info()).await.unwrap().fingerprint, drawn);
}

#[tokio::test]
async fn accessor_does_not_keep_the_environment_alive() {
    let (envref, _) = env_with(&[]).await;
    let weak = Arc::downgrade(&envref.0);
    let keyed = key_accessor(envref.clone(), key("x.txt"), address("-R/x.txt"));
    let stored = store_accessor(
        envref.clone(),
        key("x.txt"),
        StoreView::Value,
        address("-R/x.txt"),
    );
    assert_eq!(get_text(&keyed).await, "stored");
    assert_eq!(get_text(&stored).await, "stored");
    drop(envref);
    within(async {
        while weak.upgrade().is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(within(keyed.get()).await.is_err());
    assert!(within(stored.get()).await.is_err());
}

// ---------------------------------------------------------------------------------------------
// Recipe, query, action and argument steps
// ---------------------------------------------------------------------------------------------

mod steps {
    use super::*;
    use liquers_core::accessor::steps::{ActionStep, ArgumentStep, QueryFieldStep, RecipeStep};
    use liquers_core::accessor::{memory_accessor, ProjectionStep};
    use liquers_core::command_metadata::{ArgumentGUIInfo, ArgumentType, CommandMetadataRegistry};
    use liquers_core::context::Context;
    use liquers_core::error::Error;
    use liquers_macro::register_command;

    type CommandEnvironment = Env;

    fn from_csv(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("table"))
    }
    fn is_in(_state: &State<Value>, _column: String, _values: Vec<String>) -> Result<Value, Error> {
        Ok(Value::from("filtered"))
    }
    fn head(_state: &State<Value>, n: i64) -> Result<Value, Error> {
        Ok(Value::from(format!("head {n}")))
    }

    const QUERY: &str = "-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/head-10/top.csv";

    fn recipes_yaml() -> String {
        format!(
            "title: Reports\nrecipes:\n- query: {QUERY}\n  title: Top\n- query: -R/data/sales.csv/-/ns-pl/from_csv/head-3/few.csv\n  title: Few\n  arguments:\n    n: 5\n"
        )
    }

    async fn env() -> EnvRef<Env> {
        let mut env: Env = SimpleEnvironment::new();
        let cr = &mut env.command_registry;
        register_command!(cr, fn from_csv(state) -> result namespace: "pl").unwrap();
        register_command!(cr, fn is_in(state, column: String, values: Vec<String> multiple) -> result namespace: "pl").unwrap();
        register_command!(cr, fn head(state, n: i64 = 10 (gui: IntegerField)) -> result namespace: "pl").unwrap();
        env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
        env.with_recipe_provider(Box::new(DefaultRecipeProvider::new()));
        let envref = env.to_ref();
        let mut record = text_record();
        record.title = "the report recipes".to_string();
        envref
            .get_asset_manager()
            .set_binary(&key("reports/recipes.yaml"), recipes_yaml().as_bytes(), record)
            .await
            .unwrap();
        envref
    }

    fn step(s: impl ProjectionStep<Value> + 'static) -> Arc<dyn ProjectionStep<Value>> {
        Arc::new(s)
    }

    /// `recipe-<i>/query/action-<a>/argument-<arg>` of the stored `reports/recipes.yaml`.
    fn chain(
        envref: &EnvRef<Env>,
        recipe: usize,
        action: usize,
        argument: &str,
    ) -> (Arc<dyn ValueAccessor<Value>>, Arc<dyn ValueAccessor<Value>>) {
        let registry: Arc<CommandMetadataRegistry> = envref.command_metadata_snapshot();
        let root = key_accessor(
            envref.clone(),
            key("reports/recipes.yaml"),
            address("-R-key/reports/recipes.yaml/-/ns-acc/accessor"),
        );
        let part = root
            .project(step(RecipeStep(recipe)), address("recipe"))
            .unwrap()
            .project(step(QueryFieldStep), address("query"))
            .unwrap()
            .project(step(ActionStep::new(action, registry.clone())), address("action"))
            .unwrap()
            .project(step(ArgumentStep::new(argument, registry)), address("argument"))
            .unwrap();
        (root, part)
    }

    async fn stored_yaml(envref: &EnvRef<Env>) -> serde_yaml::Value {
        let bytes = envref
            .get_async_store()
            .get_bytes(&key("reports/recipes.yaml"))
            .await
            .unwrap();
        serde_yaml::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn recipe_argument_projection_reads_part_and_hints() {
        let envref = env().await;
        let (_root, n) = chain(&envref, 0, 3, "n");
        assert_eq!(*within(n.get()).await.unwrap().state.value().unwrap(), Value::from(10i64));
        let part = within(n.describe()).await.unwrap();
        assert_eq!(part.type_identifier, "I64");
        let argument = part.argument.expect("an argument part");
        assert_eq!(argument.argument_type, ArgumentType::Integer);
        assert_eq!(argument.gui_info, ArgumentGUIInfo::IntegerField);
        let command = part.command.expect("the command");
        assert_eq!((command.namespace.as_str(), command.name.as_str()), ("pl", "head"));
        assert!(!part.overridden);
        // The second recipe's `arguments` overrides its `n`.
        let (_root, overridden) = chain(&envref, 1, 2, "n");
        assert_eq!(
            *within(overridden.get()).await.unwrap().state.value().unwrap(),
            Value::from(3i64)
        );
        assert!(within(overridden.describe()).await.unwrap().overridden);
    }

    #[tokio::test]
    async fn recipe_argument_set_writes_through() {
        let envref = env().await;
        let (root, n) = chain(&envref, 0, 3, "n");
        within(n.get()).await.unwrap();
        within(n.set(Value::from(20i64), SetOptions::default()))
            .await
            .unwrap();
        let yaml = stored_yaml(&envref).await;
        assert_eq!(yaml["title"], serde_yaml::Value::from("Reports"));
        assert_eq!(
            yaml["recipes"][0]["query"],
            serde_yaml::Value::from(QUERY.replace("head-10", "head-20"))
        );
        assert_eq!(yaml["recipes"][0]["title"], serde_yaml::Value::from("Top"));
        assert_eq!(yaml["recipes"][1]["title"], serde_yaml::Value::from("Few"));
        assert_eq!(*within(n.get()).await.unwrap().state.value().unwrap(), Value::from(20i64));
        assert_eq!(within(root.info()).await.unwrap().status, Status::Source);
        // The recipe provider reads the edited recipe.
        let recipe = envref
            .get_recipe_provider()
            .recipe_opt(&key("reports/top.csv"), envref.clone())
            .await
            .unwrap()
            .expect("the edited recipe");
        assert!(recipe.query.contains("head-20"));
    }

    #[tokio::test]
    async fn refused_argument_set_writes_nothing() {
        let envref = env().await;
        let before = envref
            .get_async_store()
            .get_metadata(&key("reports/recipes.yaml"))
            .await
            .unwrap()
            .version();
        let (root, n) = chain(&envref, 0, 3, "n");
        within(n.get()).await.unwrap();
        // A value of the wrong type.
        assert!(within(n.set(Value::from("abc"), SetOptions::default())).await.is_err());
        // An argument the command does not have.
        let (_root, bogus) = chain(&envref, 0, 3, "bogus");
        assert!(within(bogus.set(Value::from(1i64), SetOptions::default())).await.is_err());
        // An action part that is not one action.
        let action = root
            .project(step(RecipeStep(0)), address("recipe"))
            .unwrap()
            .project(step(QueryFieldStep), address("query"))
            .unwrap()
            .project(
                step(ActionStep::new(3, envref.command_metadata_snapshot())),
                address("action"),
            )
            .unwrap();
        within(action.get()).await.unwrap();
        let two = Value::from_query(&address("ns-pl/head-1/head-2"));
        assert!(within(action.set(two, SetOptions::default())).await.is_err());
        let after = envref
            .get_async_store()
            .get_metadata(&key("reports/recipes.yaml"))
            .await
            .unwrap()
            .version();
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn query_steps_work_on_any_query_root() {
        let envref = env().await;
        let registry = envref.command_metadata_snapshot();
        let query = memory_accessor(
            Some(State::from_value_and_metadata(
                Value::from_query(&address(QUERY)),
                Arc::new(Metadata::new()),
            )),
            Query::new(),
        );
        let head = query
            .project(step(ActionStep::new(3, registry.clone())), address("action"))
            .unwrap();
        let n = head
            .project(step(ArgumentStep::new("n", registry.clone())), address("argument"))
            .unwrap();
        assert_eq!(*within(n.get()).await.unwrap().state.value().unwrap(), Value::from(10i64));
        let values = query
            .project(step(ActionStep::new(2, registry.clone())), address("action"))
            .unwrap()
            .project(step(ArgumentStep::new("values", registry)), address("argument"))
            .unwrap();
        assert_eq!(
            within(values.get()).await.unwrap().state.value().unwrap().try_into_json_value().unwrap(),
            serde_json::json!(["EU", "US"])
        );
        within(n.set(Value::from(20i64), SetOptions::default()))
            .await
            .unwrap();
        let root = within(query.get()).await.unwrap().state.value().unwrap();
        assert_eq!(root.try_into_query().unwrap().encode(), QUERY.replace("head-10", "head-20"));
    }
}
