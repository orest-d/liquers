//! The `acc` commands: build value accessors from queries, and project them to parts.
//!
//! An accessor command returns an `ExtValue::Accessor`: a live handle, not data. Each command is
//! cached as an asset, so one query is one shared handle, and none is ever stored (the `Accessor`
//! type declares no formats). The root commands take a key (`-R-key/…`); the projection commands
//! take an accessor as their state and return an accessor of a part of it. A projection whose part
//! moves (a step inserted before its action, a recipe removed) is invalidated and expires its own
//! asset, so the next evaluation of the query builds a fresh one.
//!
//! # Main components
//!
//! - [`register_accessor_commands!`](crate::register_accessor_commands): registers the eleven
//!   commands in namespace `acc`.
//! - Roots: [`accessor`] (a keyed asset: a set is an `Override` with a recipe, a `Source` without)
//!   and [`store`] (the stored data, view `binary`, `metadata` or `value`).
//! - Projections: [`recipe`], [`recipe_file`], [`query`], [`pipeline`], [`action`], [`argument`], [`field`],
//!   [`item`].
//! - [`get`] (volatile): the accessor's current value.
//!
//! # Where it is used
//!
//! - Queries such as
//!   `-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query/action-3/argument-n`, the
//!   `/api/acc` routes of `liquers-axum`, and the Python and JavaScript `Accessor` wrappers.
//!
//! # Related concepts
//!
//! - `liquers_core::accessor`: the contract, the roots and the steps these commands use.
//! - [`crate::value::ExtValue`]: the `Accessor` variant they return.
//!
//! # Examples
//!
//! ```
//! # use liquers_core::{context::{Context, Environment}, parse::parse_query};
//! # use liquers_core::command_metadata::{ArgumentType, CommandDefinition, CommandParameterValue};
//! # use liquers_core::value::ValueInterface;
//! # use liquers_lib::{environment::{CommandRegistryAccess, DefaultEnvironment}, value::Value};
//! # type CommandEnvironment = DefaultEnvironment<Value>;
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut env = CommandEnvironment::new();
//! let cr = env.get_mut_command_registry();
//! liquers_lib::register_accessor_commands!(cr)?;
//! # env.with_async_store(Box::new(liquers_core::store::AsyncMemoryStore::new(&Default::default())));
//! let envref = env.to_ref();
//! let query = parse_query("-R-key/notes.txt/-/ns-acc/accessor")?;
//! let notes = envref.evaluate(&query).await?.get().await?.value()?.try_into_accessor()?;
//! notes.set(Value::from("hello"), Default::default()).await?; // creates the key
//! assert_eq!(notes.get().await?.state.try_into_string()?, "hello");
//! # Ok(()) }
//! ```
//!
//! - [Tests](https://github.com/orest-d/liquers/blob/main/liquers-lib/tests/value_accessor_commands.rs):
//!   every command, through `evaluate`.

use std::sync::Arc;

use liquers_core::accessor::steps::{
    ActionStep, ArgumentStep, FieldStep, ItemStep, PipelineFieldStep, QueryFieldStep, RecipeFileStep,
    RecipeStep,
};
use liquers_core::accessor::{
    key_accessor, store_accessor, InvalidationSink, ProjectionStep, StoreView, ValueAccessor,
};
use liquers_core::assets::WeakAssetRef;
use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::query::Query;
use liquers_core::state::State;
use liquers_core::value::ValueInterface;

use crate::value::Value;

/// The query of the asset a command is evaluating, which names the accessor it returns.
async fn own_address<E: Environment<Value = Value>>(context: &Context<E>) -> Query {
    match context.get_asset_ref().get_asset_info().await {
        Ok(info) => info.query.unwrap_or_default(),
        Err(_) => Query::new(),
    }
}

fn accessor_of(state: &State<Value>) -> Result<Arc<dyn ValueAccessor<Value>>, Error> {
    state.value()?.try_into_accessor()
}

/// Expires the asset that holds a projection once the projection is invalidated, so the next
/// evaluation of its query builds a fresh one. Held weakly: the sink never keeps the asset.
struct ExpireOnInvalidation<E: Environment> {
    asset: WeakAssetRef<E>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<E: Environment> InvalidationSink for ExpireOnInvalidation<E> {
    async fn invalidated(&self) {
        if let Some(asset) = self.asset.upgrade() {
            if let Err(e) = asset.expire().await {
                eprintln!("an invalidated accessor could not expire its asset: {e}");
            }
        }
    }
}

async fn project<E: Environment<Value = Value>>(
    state: State<Value>,
    step: Arc<dyn ProjectionStep<Value>>,
    context: &Context<E>,
) -> Result<Value, Error> {
    let parent = accessor_of(&state)?;
    let part = parent.project(step, own_address(context).await)?;
    part.set_invalidation_sink(Arc::new(ExpireOnInvalidation {
        asset: context.get_asset_ref().downgrade(),
    }));
    Value::from_accessor(part)
}

/// An accessor of the keyed asset whose key is the state (`-R-key/…`).
pub async fn accessor<E: Environment<Value = Value>>(
    state: State<Value>,
    context: Context<E>,
) -> Result<Value, Error> {
    let key = state.value()?.try_into_key()?;
    let address = own_address(&context).await;
    Value::from_accessor(key_accessor(context.get_envref(), key, address))
}

/// An accessor of the stored data under the key that is the state, through `view` (`binary`,
/// `metadata` or `value`). It never evaluates a recipe.
pub async fn store<E: Environment<Value = Value>>(
    state: State<Value>,
    view: String,
    context: Context<E>,
) -> Result<Value, Error> {
    let key = state.value()?.try_into_key()?;
    let view = StoreView::parse(&view)?;
    let address = own_address(&context).await;
    Value::from_accessor(store_accessor(context.get_envref(), key, view, address))
}

/// Recipe `index` of the recipe list the state accessor reads.
pub async fn recipe<E: Environment<Value = Value>>(
    state: State<Value>,
    index: usize,
    context: Context<E>,
) -> Result<Value, Error> {
    project(state, Arc::new(RecipeStep(index)), &context).await
}

/// The recipe with filename `filename` of the recipe list the state accessor reads, wherever it
/// sits in the list.
pub async fn recipe_file<E: Environment<Value = Value>>(
    state: State<Value>,
    filename: String,
    context: Context<E>,
) -> Result<Value, Error> {
    project(state, Arc::new(RecipeFileStep(filename)), &context).await
}

/// The query of the recipe the state accessor reads.
pub async fn query<E: Environment<Value = Value>>(
    state: State<Value>,
    context: Context<E>,
) -> Result<Value, Error> {
    project(state, Arc::new(QueryFieldStep), &context).await
}

/// The pipeline of the recipe the state accessor reads.
pub async fn pipeline<E: Environment<Value = Value>>(
    state: State<Value>,
    context: Context<E>,
) -> Result<Value, Error> {
    project(state, Arc::new(PipelineFieldStep), &context).await
}

/// Element `index` of the query or pipeline the state accessor reads (0 is a query's resource).
pub async fn action<E: Environment<Value = Value>>(
    state: State<Value>,
    index: usize,
    context: Context<E>,
) -> Result<Value, Error> {
    let registry = context.get_envref().command_metadata_snapshot();
    project(state, Arc::new(ActionStep::new(index, registry)), &context).await
}

/// Argument `name` of the action the state accessor reads.
pub async fn argument<E: Environment<Value = Value>>(
    state: State<Value>,
    name: String,
    context: Context<E>,
) -> Result<Value, Error> {
    let registry = context.get_envref().command_metadata_snapshot();
    project(state, Arc::new(ArgumentStep::new(name, registry)), &context).await
}

/// Field `name` of the document (YAML, JSON or an object) the state accessor reads.
pub async fn field<E: Environment<Value = Value>>(
    state: State<Value>,
    name: String,
    context: Context<E>,
) -> Result<Value, Error> {
    project(state, Arc::new(FieldStep::new(name)), &context).await
}

/// Item `index` of the list the state accessor reads.
pub async fn item<E: Environment<Value = Value>>(
    state: State<Value>,
    index: usize,
    context: Context<E>,
) -> Result<Value, Error> {
    project(state, Arc::new(ItemStep(index)), &context).await
}

/// The current value of the state accessor (volatile: read on every evaluation).
pub async fn get(state: State<Value>) -> Result<Value, Error> {
    let snapshot = accessor_of(&state)?.get().await?;
    Ok(snapshot.state.value()?.as_ref().clone())
}

/// Registers the `acc` commands.
#[macro_export]
macro_rules! register_accessor_commands {
    ($cr:expr) => {{
        use liquers_macro::register_command;
        use $crate::accessor::*;
        #[allow(unused_imports)]
        use $crate::value::Value;

        register_command!($cr,
            async fn accessor(state, context) -> result
            namespace: "acc"
            label: "Accessor"
            doc: "A settable accessor of the keyed asset given as state (-R-key/…)"
        )?;
        register_command!($cr,
            async fn store(state, view: String = "value", context) -> result
            namespace: "acc"
            label: "Store accessor"
            doc: "A settable accessor of the stored data (view: binary, metadata or value) of the key given as state"
        )?;
        register_command!($cr,
            async fn recipe(state, index: usize, context) -> result
            namespace: "acc"
            label: "Recipe"
            doc: "The accessor of one recipe of the recipe list the state accessor reads"
        )?;
        register_command!($cr,
            async fn recipe_file(state, filename: String, context) -> result
            namespace: "acc"
            label: "Recipe by filename"
            doc: "The accessor of the recipe with this filename in the recipe list the state accessor reads, wherever it sits in the list"
        )?;
        register_command!($cr,
            async fn query(state, context) -> result
            namespace: "acc"
            label: "Recipe query"
            doc: "The accessor of the query of the recipe the state accessor reads"
        )?;
        register_command!($cr,
            async fn pipeline(state, context) -> result
            namespace: "acc"
            label: "Recipe pipeline"
            doc: "The accessor of the pipeline of the recipe the state accessor reads"
        )?;
        register_command!($cr,
            async fn action(state, index: usize, context) -> result
            namespace: "acc"
            label: "Pipeline element"
            doc: "The accessor of one element of the query or pipeline the state accessor reads (0 is the resource)"
        )?;
        register_command!($cr,
            async fn argument(state, name: String, context) -> result
            namespace: "acc"
            label: "Action argument"
            doc: "The accessor of one argument of the action the state accessor reads"
        )?;
        register_command!($cr,
            async fn field(state, name: String, context) -> result
            namespace: "acc"
            label: "Document field"
            doc: "The accessor of one field of the document the state accessor reads"
        )?;
        register_command!($cr,
            async fn item(state, index: usize, context) -> result
            namespace: "acc"
            label: "List item"
            doc: "The accessor of one item of the list the state accessor reads"
        )?;
        register_command!($cr,
            async fn get(state) -> result
            namespace: "acc"
            label: "Accessor value"
            doc: "The current value of the accessor given as state"
            volatile: true
        )?;
        Ok::<(), liquers_core::error::Error>(())
    }};
}
