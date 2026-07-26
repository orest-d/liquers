# Liquers

Liquers is a query-driven data transformation framework for Rust. Its public API
models URL-compatible queries, typed values and metadata, registered commands,
recipes, execution plans, asynchronously evaluated assets, and key-addressed
storage.

> [!NOTE]
> The workspace is currently at version `0.1.0` and its public API is evolving.
> Documents under `specs/` may describe current behavior, proposed behavior, or
> implementation history. The Rust public API and its tests are the authoritative
> reference when a specification has no explicit status.

## API reference

Generate the Rust API reference from a source checkout:

```shell
cargo doc --workspace --no-deps
```

In a disk-constrained environment, generate one crate at a time:

```shell
cargo doc -p liquers-core --no-deps
cargo doc -p liquers-store --no-deps
cargo doc -p liquers-axum --no-deps
cargo doc -p liquers-lib --no-deps --no-default-features
```

The generated workspace index is `target/doc/index.html`.

Current supporting references:

| Subject | Reference |
|---|---|
| Architecture and terminology | [Project overview](specs/PROJECT_OVERVIEW.md) |
| Command registration API | [Command registration guide](specs/COMMAND_REGISTRATION_GUIDE.md) |
| Asset model and lifecycle | [Assets specification](specs/ASSETS.md), [lifecycle map](specs/ASSET_LIFECYCLE.md) |
| Store configuration | [Store configuration specification](specs/STORE_CONFIG_FSD.md) |
| HTTP surface | [Axum reference](liquers-axum/README.md), [web API specification](specs/WEB_API_SPECIFICATION.md) |
| Polars commands | [Polars command library](specs/POLARS_COMMAND_LIBRARY.md) |
| Image commands | [Image command library](specs/IMAGE_COMMAND_LIBRARY.md) |
| Documentation gaps and progress | [API documentation analysis](specs/api-docs-analysis/README.md) |

## Workspace API surface

| Crate | Public API responsibility |
|---|---|
| [`liquers-core`](liquers-core/) | Query syntax tree and parsing, values, state and metadata, command execution, recipes, plans, environments, assets, stores, dependencies, expiration, and errors |
| [`liquers-macro`](liquers-macro/) | `register_command!` and `command_version` procedural macros |
| [`liquers-store`](liquers-store/) | Declarative store configuration, store-router construction, and OpenDAL-backed stores |
| [`liquers-lib`](liquers-lib/) | Extended value types and command libraries for images, Polars, egui, and web UI |
| [`liquers-axum`](liquers-axum/) | Axum router builders and HTTP/WebSocket representation types |
| [`liquers-py`](liquers-py/) | Python bindings for selected core query, metadata, plan, recipe, dependency, expiration, and error types |

The direct workspace dependencies are:

| Crate | Depends on workspace crates |
|---|---|
| `liquers-core` | None |
| `liquers-macro` | None at macro compile time; generated code references `liquers-core` |
| `liquers-store` | `liquers-core` |
| `liquers-lib` | `liquers-core`, `liquers-macro` |
| `liquers-axum` | `liquers-core`, `liquers-store` |
| `liquers-py` | `liquers-core` |

`liquers-core` defines the extension traits. The other crates provide macros,
implementations, integrations, or bindings around those traits.

## Core concept index

| Concept | Primary public API | Module |
|---|---|---|
| Query | `Query`, `QuerySegment`, `QuerySource`, `TryToQuery` | `liquers_core::query` |
| Query parsing | `parse_query`, `parse_key`, `parse_simple_template` | `liquers_core::parse` |
| Keys and resource names | `Key`, `ResourceName`, `ResourceQuerySegment` | `liquers_core::query` |
| Actions | `ActionRequest`, `ActionParameter`, `SegmentHeader` | `liquers_core::query` |
| Values | `Value`, `ValueInterface`, `DefaultValueSerializer` | `liquers_core::value` |
| State | `State<V>` | `liquers_core::state` |
| Metadata | `Metadata`, `MetadataRecord`, `Status`, `AssetInfo`, `LogEntry`, `ProgressEntry`, `Version` | `liquers_core::metadata` |
| Command metadata | `CommandMetadata`, `CommandKey`, `ArgumentInfo`, `CommandMetadataRegistry` | `liquers_core::command_metadata` |
| Command execution | `CommandExecutor`, `CommandRegistry`, `CommandArguments` | `liquers_core::commands` |
| Command registration | `register_command!`, `command_version` | `liquers_macro` |
| Recipes | `Recipe`, `RecipeList`, `AsyncRecipeProvider` | `liquers_core::recipes` |
| Plans | `Plan`, `PlanBuilder`, `Step`, `ParameterValue` | `liquers_core::plan` |
| Interpretation | `make_plan`, `finalize_plan`, `apply_plan`, `evaluate` | `liquers_core::interpreter` |
| Environment | `Environment`, `EnvRef`, `SimpleEnvironment`, `ImmediateEnvironment` | `liquers_core::context` |
| Command context | `Context`, `Session`, `User` | `liquers_core::context` |
| Assets | `AssetRef`, `AssetManager`, `DefaultAssetManager`, `ImmediateAssetManager`, `EvalMode` | `liquers_core::assets` |
| Asset messages | `AssetServiceMessage`, `AssetNotificationMessage` | `liquers_core::assets` |
| Stores | `AsyncStore`, `AsyncStoreRouter`, `AsyncMemoryStore`, `AsyncFileStore` | `liquers_core::store` |
| Store configuration | `StoreConfig`, `StoreRouterConfig`, `StoreRouterBuilder` | `liquers_store` |
| Dependencies | `DependencyRelation`, `PlanDependency` | `liquers_core::dependencies` |
| Expiration | `Expires`, `ExpirationTime` | `liquers_core::expiration` |
| Errors | `Error`, `ErrorType` | `liquers_core::error` |
| Media types | `file_extension_to_media_type`, `media_type_to_extension` | `liquers_core::media_type` |

This table identifies API locations; it is not a substitute for the type and method
contracts in rustdoc.

## Core type relationships

### Value containment

```text
ValueInterface
      |
      v
State<V> = value + Metadata
      |
      v
AssetRef<E> = asynchronous/shared handle to a state in an Environment
```

An asset may represent work that has not started, queued or active work, a persisted
resource, or a terminal result. `Metadata::status()` describes its lifecycle state.

### Execution

```text
Query
  -> Recipe
  -> Plan
  -> interpreter
  -> registered CommandExecutor
  -> State<Environment::Value>
```

`EnvRef::evaluate` resolves a query through the environment's `AssetManager` and
returns an `AssetRef`. `AssetRef::get` waits for a terminal state. The lower-level
functions in `liquers_core::interpreter` expose plan construction and application
directly.

### Runtime services

The `Environment` trait binds the associated value, command executor, session,
payload, asset manager, store, and recipe provider into one concrete runtime type.
`Context` is created for command execution and provides access to the environment,
current asset, dependency recording, logging, progress, and optional payload.

## Environment implementations

| Type | Target and execution model |
|---|---|
| `liquers_core::context::SimpleEnvironment<V>` | Native environment backed by `DefaultAssetManager` and its job queue |
| `liquers_core::context::ImmediateEnvironment<V>` | Spawn-free environment backed by `ImmediateAssetManager` |
| `liquers_lib::environment::DefaultEnvironment<V, P>` | Extended environment; selects the threaded manager on native targets and immediate manager on WASM |

`Environment::to_ref` consumes the configured environment, initializes its asset
manager, and returns `EnvRef<E>`.

## Feature and target availability

| Crate | Feature | Default | API effect |
|---|---|---:|---|
| `liquers-core` | `async_store` | Yes | Enables asynchronous store integration |
| `liquers-core` | `tokio_exec` | No | Enables `async_store`, `futures`, and `async-trait`; no additional API is currently gated on this feature directly |
| `liquers-lib` | `image-support` | Yes | Enables image-processing commands that require `imageproc` |
| `liquers-lib` | `egui` | Yes | Enables egui values, widgets, and commands |
| `liquers-lib` | `polars` | Yes | Enables Polars DataFrame values and commands |
| `liquers-lib` | `webui` | No | Enables browser/web UI modules and WASM bindings |
| `liquers-store` | `toml` | No | Enables TOML store configuration parsing |

Some modules and enum variants are conditionally compiled. Always generate rustdoc
with the feature set used by the application when checking API availability.

On `wasm32-unknown-unknown`, Liquers uses conditional `Send`/`Sync` marker traits
and spawn-free asset evaluation. Native and WASM builds therefore expose related
interfaces but do not have identical scheduling behavior.

## Integration surfaces

### HTTP

`liquers-axum` exports:

- `QueryApiBuilder`
- `StoreApiBuilder`
- `AssetsApiBuilder`
- `RecipesApiBuilder`
- `ApiResponse`, `BinaryResponse`, `DataEntry`, `ErrorDetail`
- `SerializationFormat`

Some registered asset mutation and listing operations currently return unsupported
or not-implemented responses. Consult the implementation and capability
documentation before treating every registered route as functional.

### Extended values

`liquers-lib::value::Value` combines simple values with feature-dependent extended
values. `ExtValue` includes images and UI elements and may include Polars and egui
variants depending on enabled features.

### Python

`liquers-py` exposes only a selected subset of the Rust API. Availability in
`liquers-core` does not imply that a corresponding Python class or function exists.

## Reference maintenance

Public API documentation should:

1. Describe current behavior and mark proposals separately.
2. State invariants, errors, side effects, concurrency behavior, and feature gates.
3. Link related types in rustdoc.
4. Use compile-tested examples where an example clarifies a contract.
5. Keep generated rustdoc free of broken intra-doc links.

The runnable [`hello_world`](liquers-core/examples/hello_world.rs) example is kept
as an API integration check. A separate user guide can build on the reference once
the concept and method contracts are sufficiently complete.

## Build and validation

Focused validation commands:

```shell
cargo test -p liquers-core --lib
cargo run -p liquers-core --example hello_world
cargo doc -p liquers-core --no-deps
```

The default `liquers-lib` feature set produces a large build, primarily due to
Polars. See the [development guide](CLAUDE.md#building-and-testing) before running
full-workspace builds in a disk-constrained environment.

Liquers is licensed under the
[GNU Affero General Public License, version 3](LICENSE).
