---
title: Recipes, assets and caching
status: ready
sources: [reference/ASSETS.md, reference/ASSET_LIFECYCLE.md, reference/RECIPES_AND_PLANS.md, guides/DEPENDENCY_CONSISTENCY_GUIDE.md, guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md, guides/COMMAND_DESIGN_GUIDE.md]
reviewed: 2026-11-10
---
# 7. Recipes, assets and caching

PURPOSE: The reader knows when a result is computed, cached, reused and recomputed, and can watch, cancel, replace and configure that from Rust.

This chapter is about what happens between asking for a query and receiving its value. The first
four sections describe the model in the words shared by all three guides. The rest shows the
Rust side of it: the asset reference, the asset manager, recipes, and the settings.

{{> ../_shared/assets.md}}

## 7.5 The asset reference

`env.evaluate(query)` returns an `AssetRef`. It is a handle to the asset, cheap to clone and safe
to send to another task. The asset behind it lives in the *asset manager* of the environment.

```rust
use liquers_core::assets::AssetNotificationMessage;

async fn slow_report(_state: State<Value>, context: Context<CommandEnvironment>) -> Result<Value, Error> {
    context.info("collecting")?;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    Ok(Value::from("the report"))
}

let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
let cr = &mut builder.command_registry;
register_command!(cr, async fn slow_report(state, context) -> result)?;
let env = builder.build()?;

let asset = env.evaluate("slow_report/report.txt").await?; // returns at once
assert!(!asset.status().await.is_finished());
let mut notifications = asset.subscribe_to_notifications().await;

let state = asset.get().await?; // waits
assert_eq!(state.try_into_string()?, "the report");
assert_eq!(asset.status().await, Status::Ready);
assert!(matches!(
    *notifications.borrow_and_update(),
    AssetNotificationMessage::JobFinished | AssetNotificationMessage::StatusChanged(Status::Ready)
));

// The same query gives the same asset, not a second computation.
let again = env.evaluate("slow_report/report.txt").await?;
assert_eq!(again.id(), asset.id());

// The value as bytes with its metadata, as a server would send it.
let (bytes, metadata) = asset.get_binary().await?;
assert_eq!(bytes.as_slice(), b"the report");
assert_eq!(metadata.get_media_type(), "text/plain");
```

| `AssetRef` method | Does |
|---|---|
| `get()` | waits until the asset is finished and returns its state |
| `get_binary()` | the same, as bytes in the asset's data format, with the metadata |
| `status()` | the status now, without waiting |
| `poll_state()` | the state if there is one now, else `None` |
| `get_metadata()`, `get_asset_info()` | the metadata now, or a summary of it; they work while the asset is being computed, so this is how you read the log of a running evaluation |
| `subscribe_to_notifications()` | a `tokio::sync::watch` receiver of `AssetNotificationMessage`: status changes, log lines, progress |
| `cancel()` | asks a running evaluation to stop |
| `expire()` | declares the value stale |
| `id()` | a number that identifies the asset within this environment |

The notifications are what a user interface or a server uses to show progress without asking
again and again. A `watch` channel keeps only the latest message, which is what a display wants.

All of these are `async`, also the ones that do not wait for the value, because an asset is
shared between tasks and reading it takes a lock.

## 7.6 Keeping results fresh

The asset manager is where values are written when the write should be noticed:

```rust
use liquers_core::assets::AssetManager;

fn shout(text: &str) -> Result<Value, Error> {
    Ok(Value::from(text.to_uppercase()))
}

let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
let cr = &mut builder.command_registry;
register_command!(cr, fn shout(text) -> result)?;
let env = builder.build()?;
let assets = env.get_asset_manager();
let key = parse_key("data/note.txt")?;

assets.set_state(&key, State::new().with_data(Value::from("milk"))).await?;
let first = env.evaluate("-R/data/note.txt/-/shout").await?;
assert_eq!(first.get().await?.try_into_string()?, "MILK");

// A write through the asset manager makes everything made from the key out of date.
assets.set_state(&key, State::new().with_data(Value::from("bread"))).await?;
assert_eq!(first.status().await, Status::Expired);
let second = env.evaluate("-R/data/note.txt/-/shout").await?;
assert_eq!(second.get().await?.try_into_string()?, "BREAD");

// Throwing a result away by hand.
second.expire().await?;
assert!(second.is_expired().await);
```

`set_state` stores a value at a key, and `set_binary` stores bytes with metadata. Both go through
the *dependency manager*, the part of the asset manager that knows which asset was made from
which. Every asset made from the key becomes `Expired`, and so does everything made from those.
Nothing is computed at that moment. An expired asset is computed again when somebody next asks
for its query.

What counts as "made from" is recorded while an asset is evaluated: the keys its query read, the
queries its commands asked for through the context (section 5.4), the links in its arguments, the
recipe that defined it, and the version of every command that ran. The record is the
`get_dependencies()` of its metadata, and it is stored with the result. After a restart Liquers
therefore still knows what a stored result depends on, and checks it when the result is read: a
stored result whose input has changed in the meantime is not used.

Three more things end the validity of an asset, and all three are properties of commands
(section 5.6): a `volatile` command, an `expires` time, and a changed `version`.

`asset.expire()` does the same by hand for one asset and everything made from it. Use it when
you know something Liquers cannot know, for instance that a remote service has new data.

## 7.7 Cancelling

`cancel()` asks an evaluation to stop:

```rust
async fn forever(_state: State<Value>) -> Result<Value, Error> {
    tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
    Ok(Value::none())
}

let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
let cr = &mut builder.command_registry;
register_command!(cr, async fn forever(state) -> result)?;
let env = builder.build()?;

let asset = env.evaluate("forever").await?;
tokio::time::sleep(std::time::Duration::from_millis(20)).await; // let it start
asset.cancel().await?;
assert_eq!(asset.status().await, Status::Cancelled);
let state = asset.get().await?;
let error = state.value().err().expect("no value");
assert_eq!(error.error_type, ErrorType::Cancelled);
```

Cancelling is a request. An async command stops at its next `.await`. A synchronous one stops
only if it checks (section 5.7), and if it finishes first, its result stands: cancelling exists
to save work, not to throw away work that is done. `cancel()` returns `Ok` in every case, so read
the status afterwards. A cancelled asset has the status `Cancelled` and no value, and nothing is
stored. Asking for the query again starts a new evaluation.

Cancelling an asset does not cancel what it depends on, because a dependency may be shared with
other assets. In the other direction it does spread: an asset that waits for a cancelled
dependency ends `Cancelled` itself.

## 7.8 Recipes in practice

A `recipes.yaml` in a folder of the store declares results that appear in that folder
(section 7.4). From Rust you meet them through the asset manager, which treats stored files and
recipes alike:

```rust
    use liquers_core::assets::AssetManager;

    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    let recipes = b"
recipes:
  - query: -R/./sales.csv/-/ns-rec/to_record/eq-region-EU/eu.csv
    title: Sales in the EU
    description: The rows of sales.csv with region EU
  - query: -R/./sales.csv/-/ns-rec/to_record/nrows/count.txt
    title: Number of sales
    expires: in 1 h
";
    store.set(&parse_key("data/sales.csv")?, SALES, &Metadata::new()).await?;
    store.set(&parse_key("data/recipes.yaml")?, recipes, &Metadata::new()).await?;
    let env = library_environment(store.clone())?;
    let assets = env.get_asset_manager();

    // The folder lists what is stored and what can be made.
    let names = assets.listdir(&parse_key("data")?).await?;
    assert!(names.contains(&"eu.csv".to_string()) && names.contains(&"count.txt".to_string()));
    let info = assets.get_asset_info(&parse_key("data/eu.csv")?).await?;
    assert_eq!(info.status, Status::Recipe); // known, not made yet
    assert_eq!(info.title, "Sales in the EU");

    // Asking for the key makes it and stores it.
    let count = assets.get(&parse_key("data/count.txt")?).await?.get().await?;
    assert_eq!(count.try_into_string()?, "3");
    assert!(store.contains(&parse_key("data/count.txt")?).await?);

    // The recipe behind a key.
    let recipe = assets.recipe_opt(&parse_key("data/eu.csv")?).await?.expect("a recipe");
    assert_eq!(recipe.title, "Sales in the EU");
```

`assets.get(&key)` is the asset for a key. It is what `-R/<key>` evaluates to: the stored file if
there is one and it is still valid, otherwise the result of the recipe. The recipe's result is
written to the store under its key, with its metadata, so the next program that reads the key
finds a file. That is the difference between a recipe and a query: a query's result lives in the
asset manager's memory, a recipe's result is a file.

The recipes in the example use `-R/./sales.csv`, a key relative to the folder of the
`recipes.yaml`. A folder with its data and its recipes can then be copied or moved as a whole.

A recipe may also carry a `pipeline`, which runs before its query and can pass structured
values that a query cannot hold (section 3.10).

## 7.9 Recipes from code

Where recipes come from is decided by the *recipe providers* of the environment. A provider
answers one question: is there a recipe for this key? The environment asks all its providers,
and a key may be claimed by one only.

`liquers-lib` comes with two. `recipes_yaml` reads the `recipes.yaml` files. `manifest` serves
the chunks of large tables (chapter 9). An environment made with `default_environment_builder`
has both.

When your recipes follow a pattern, "for every `x.csv` there is an `x.summary.txt`", write a
provider instead of listing them. It implements the trait `AsyncRecipeProvider`:

```rust
// Sketch: a recipe provider. The trait has four more methods of the same kind.
#[async_trait]
impl AsyncRecipeProvider<CommandEnvironment> for Summaries {
    async fn recipe_opt(
        &self,
        key: &Key,
        _envref: EnvRef<CommandEnvironment>,
    ) -> Result<Option<Recipe>, Error> {
        match key.to_string().strip_suffix(".summary.txt") {
            Some(base) => Ok(Some(Recipe::new(
                format!("-R/{base}.csv/-/ns-rec/to_record/describe/summary.txt"),
                "Summary".to_string(),
                String::new(),
            )?)),
            None => Ok(None), // not mine: answer quickly
        }
    }
}
```

Add it to the builder with `with_appended_recipe_provider(Arc::new(Summaries))`. A provider is
asked often, so it should answer quickly for keys that are not its own. To let a configuration
document place your provider in its `recipes` list, describe it in a `RecipeProviderTypeMap` and
give that to `with_recipe_provider_factory`, in the same way as a store type (section 6.6).

## 7.10 Settings that change caching

By default everything is kept: the result of every query and of every prefix of it. That suits
interactive work. A service may want less, and the `AssetManagerOptions` of the builder
(section 8.3) say how much:

| Option | Values | Meaning |
|---|---|---|
| `query_cache_strategy` | `All`, `Result`, `None` | what is kept of a query that is evaluated directly |
| `recipe_cache_strategy` | `All`, `Result`, `None` | the same for recipes that do not say `cached:` themselves |
| `job_capacity` | a number | how many evaluations run at the same time |
| `verify_versions` | `OnRead`, `Off` | whether a stored result is checked against its inputs when it is read |
| `dependency_audit` | `Explicit`, `OnLoad` | whether the dependency record of a stored result is checked when it is loaded |
| `external_change` | `UserInput`, `Corrupted` | what a stored result becomes when its file was changed from outside |

`All` keeps the result and the intermediate steps, `Result` only the result, `None` nothing. Two
combinations are common. A public service on which anybody may run a query, but only recipes are
approved, keeps recipes with `All` and queries with `None`: ad-hoc queries reuse what recipes
cached and add nothing. A small machine keeps recipes with `Result`.

The strategies decide whether a value is kept, not how much memory the cache may use. There is
no size limit yet, so a long-running service should choose its strategies with that in mind.

A single command can opt out with `cached: false` (section 5.6), and a single recipe with its
`cached` field.
