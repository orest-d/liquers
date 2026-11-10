---
title: The environment
status: ready
sources: [guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md, reference/ENVIRONMENT_CONFIG.md, reference/PROJECT_OVERVIEW.md]
reviewed: 2026-11-10
---
# 8. The environment

PURPOSE: The reader can build an environment with the commands, stores and recipes an application needs, configure it from a file, and choose how it executes.

The *environment* holds everything that queries need and share: the commands, the store, the
recipe providers, the asset manager with its cache, and the registry of value types. A program
normally has one. This chapter is about building it.

## 8.1 The builder

An environment is made in two phases. Everything that changes it happens on a *builder*.
`build()` consumes the builder and returns an `EnvRef`, a shared reference to an environment
that can no longer be changed:

```rust
use liquers_core::assets::AssetManager;
use liquers_core::cache_strategy::CacheStrategy;
use liquers_core::environment_builder::AssetManagerOptions;
use liquers_lib::environment::default_environment_builder;

fn hello() -> Result<Value, Error> {
    Ok(Value::from("world"))
}

let options = AssetManagerOptions::default()
    .with_job_capacity(4) // at most four evaluations at a time
    .with_query_cache_strategy(CacheStrategy::Result); // keep results, not intermediate steps

let mut builder = default_environment_builder::<Value, ()>()
    .with_async_store(Arc::new(AsyncMemoryStore::new(&Key::new())))
    .with_asset_manager_options(options)
    .with_default_namespaces(["rec"]);
register_library(&mut builder.command_registry)?;
let cr = &mut builder.command_registry;
register_command!(cr, fn hello() -> result)?;

// Look at the command metadata before building; build() refuses it if it has errors.
let report = builder.validate();
assert!(!report.has_errors());

let env = builder.build()?; // started and ready
assert!(env.get_asset_manager().is_started());
assert!(env.command_metadata_snapshot().commands.iter().any(|c| c.name == "hello"));
```

| What | How |
|---|---|
| Commands | `&mut builder.command_registry`, a public field, because `register_command!` needs a mutable reference |
| The store | `.with_async_store(store)`, or `.with_store_config(config, factory)` |
| Everything a document can say | `.with_config(config, factory)` (section 8.2) |
| Recipe providers | `.with_recipes(…)`, `.with_appended_recipe_provider(provider)` (section 7.9) |
| Asset manager options | `.with_asset_manager_options(options)` (section 8.3) |
| Namespaces searched without `ns` | `.with_default_namespaces(["rec"])` |
| Value types of your own | `.with_type_registry(registry)` |

The setters take the builder and return it, so they chain. They cannot fail. Whatever can be
wrong, a store that cannot be built, a variable that is not set, command metadata that
contradicts itself, is reported by `build()`.

**What `build()` guarantees.** When it returns, the environment is ready: the asset manager is
started and the versions of all commands are known to the dependency manager. There is no
moment in which you hold a reference to a half-built environment, and no need to wait before
the first query.

**The `EnvRef`.** `EnvRef<E>` is an `Arc` around the environment with the methods you call:
`evaluate`, `get_asset_manager`, `get_async_store`, `get_recipe_provider`, `get_type_registry`,
`command_metadata_snapshot`. Clone it freely and give a clone to every task that needs one.

**Which builder.** There are two starting points, and they differ in one thing:

| Start with | Recipes |
|---|---|
| `liquers_lib::environment::default_environment_builder::<Value, ()>()` | `recipes.yaml` files, and table manifests with the `records` feature |
| `liquers_core::environment_builder::EnvironmentBuilder::<Value>::new()` | none, until you configure them |

`liquers-core` has no opinion about recipes, so its bare builder resolves none, and nothing
tells you. Use the builder of `liquers-lib` unless you mean that. The second type parameter, `()`
in the examples, is the type of the *payload* that an evaluation can carry (chapter 11).

## 8.2 Configuration documents

One file configures a program built on Liquers: the store, the recipes, the asset manager, and
the sections of the parts you use, such as the `server` section of the web server.

```yaml
# liquers.yaml
store:
  stores:
    - type: filesystem
      prefix: data
      config:
        path: ${LIQUERS_DATA}
recipes: default
assets:
  job_capacity: 8
default_namespaces: [rec]
command_registry: commands.yaml
server:
  port: 8000
```

Read it as a `ConfigDocument`. Each part of your program takes its section, and `finish()`
returns the environment's configuration once nothing is left over:

```rust
    use liquers_core::environment_config::ConfigDocument;
    use liquers_core::store_factory::default_store_factory;
    use liquers_lib::environment::default_environment_builder;

    #[derive(serde::Deserialize)]
    struct Report {
        title: String,
    }

    let mut doc = ConfigDocument::from_yaml(
        "
store:
  stores:
    - {type: memory, prefix: data}
recipes: default
assets:
  job_capacity: 8
default_namespaces: [rec]
report:
  title: Monthly sales
",
    )?;
    let report: Option<Report> = doc.take_section("report")?; // a section of your own
    let config = doc.finish()?; // an unknown section would fail here

    let mut builder = default_environment_builder::<Value, ()>()
        .with_config(config, Box::new(default_store_factory()));
    register_library(&mut builder.command_registry)?;
    let env = builder.build()?;

    env.get_async_store().set(&parse_key("data/sales.csv")?, SALES, &Metadata::new()).await?;
    // `default_namespaces: [rec]` lets the query leave out `ns-rec`.
    let rows = env.evaluate("-R/data/sales.csv/-/to_record/nrows").await?.get().await?;
    assert_eq!(rows.try_into_string()?, "3");
    assert_eq!(report.expect("the section").title, "Monthly sales");
```

`ConfigDocument::load(path)` reads a `.yaml`, `.yml` or `.json` file, and a `.toml` file with the
`toml` feature of `liquers-core`; `from_yaml` and `from_json` read text. A section nobody took is
an error that names it and lists the known sections, so a typo such as `sever:` never leaves a
setting at its default without notice. A section of your own is any type that implements
`serde::Deserialize`, and `take_section` returns `None` when the file has no such section. The
web server takes its section the same way, with `ServerConfig::from_document(&mut doc)`
(chapter 12).

The sections of the environment itself:

| Section | Says |
|---|---|
| `store` | the mounts (section 6.2) |
| `recipes` | where recipes come from: `default`, `trivial` for none, or a list such as `[recipes_yaml, manifest]` |
| `assets` | the asset manager options (section 7.10), in lower case: `query_cache_strategy: result` |
| `default_namespaces` | namespaces that queries may use without `ns` |
| `command_registry` | a file with labels, documentation and argument hints for commands |

Commands are always registered in code: a document cannot name a Rust function. The command
metadata file adds descriptions to commands that the code registers. `doc.command_registry_path()`
gives its path, resolved against the folder of the document. Read it as a
`CommandMetadataRegistry` and add its commands to
`builder.command_registry.command_metadata_registry` before `build()`. An entry that names no
registered command fails the build.

Two behaviours are quiet enough to be worth knowing. A document that says nothing about
`recipes` gets the default list, not an empty one. And `default_namespaces` changes what a query
means: `-R/data/sales.csv/-/to_record/nrows` works in the example and nowhere else. Recipes and
queries that are meant to travel keep their `ns`.

The kinds of error tell you what is wrong: a document that does not parse is a `ParseError`, a
missing setting or an unset variable is a `ConfigurationError`, and a store type that this build
does not have is `NotSupported`.

## 8.3 Execution models and manager options

**Queued or inline.** The asset manager comes in two *kinds*, chosen by a type parameter of the
builder:

| Kind | Evaluates | Needs a Tokio runtime |
|---|---|---|
| `Queued` | in background tasks, at most `job_capacity` at a time | yes |
| `Inline` | in the task that calls `get()` | no |

`Queued` is the default on every native platform, and what a server or an application wants:
`evaluate` returns at once, results are computed in the background, and a long evaluation does
not hold up the caller. It starts its tasks when the environment is built. This is why `build()`
needs a runtime although it is not `async`, and why calling it outside one panics.

`Inline` spawns nothing. It is the default in the browser, where there are no threads, and it is
useful natively where there is no runtime:

```rust
use liquers_core::{
    context::Environment,
    environment_builder::{EnvironmentBuilder, Inline},
    error::Error,
};
use liquers_lib::value::Value;

// No Tokio runtime: the Inline kind evaluates in the caller's task and spawns nothing.
fn main() -> Result<(), Error> {
    let mut builder = EnvironmentBuilder::<Value, (), Inline>::new();
    builder.command_registry.register_command(
        liquers_core::command_metadata::CommandKey::new_name("hello"),
        |_state, _arguments, _context| Ok(Value::from("world")),
    )?;
    let env = builder.build()?;

    let state = futures::executor::block_on(async { env.evaluate("hello").await?.get().await })?;
    assert_eq!(state.try_into_string()?, "world");
    Ok(())
```

The kind is a type and not a setting, because the two produce different environment types. A
program that wants to choose at run time writes both branches.

**Options.** `AssetManagerOptions` carries the settings of section 7.10. Each has a setter
(`with_job_capacity`, `with_query_cache_strategy`, …), and a kind refuses an option it cannot
honour when the environment is built: `job_capacity` on `Inline` is an error and not a silently
ignored wish.

**Capacity.** `job_capacity` limits how many assets are evaluated at once. A synchronous command
occupies a thread of the runtime while it runs, so on a runtime with few threads many
synchronous commands at once can starve the rest of your program. Make long commands `async`
where the work is waiting, and give the runtime enough worker threads where it is computing.

## 8.4 An environment of your own

`DefaultEnvironment<Value>` is one implementation of the trait `Environment`. A program that
embeds Liquers in a larger system, with global services of its own that commands must reach, can
implement the trait itself and use the same commands, stores and asset managers. The Python
package and the browser module do exactly that. It is a larger job than anything else in this
guide. Start from the `GenericEnvironment` of `liquers-core` and its documentation in the
[API reference](https://orest-d.github.io/liquers/overview.html#api-reference). The usual needs
are met without it: commands capture what they need (section 5.10), and a section of the
configuration document holds your settings.

The same in Python: [8. The environment](../python/08-environment.md).
