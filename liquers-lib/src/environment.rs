//! The library environment.
//!
//! Since the environment-builder work this is a thin layer over `liquers-core`'s
//! [`GenericEnvironment`]: the struct this module used to define was, field for field, that type
//! with the asset-manager kind chosen by a `cfg` import pair. `DefaultKind` in `liquers-core` makes
//! that selection now, so [`DefaultEnvironment`] is an alias and the `cfg` pair is gone.
//!
//! What stays here is what is genuinely `liquers-lib`'s: a different default recipe provider from
//! the core one, and the polars registration entry point.

use std::sync::Arc;

use liquers_core::{
    assets::AssetManager,
    commands::{CommandRegistry, PayloadType},
    context::{EnvRef, Environment, GenericEnvironment},
    environment_builder::{AssetManagerKind, AssetManagerOptions, DefaultKind, EnvironmentBuilder},
    error::Error,
    recipes::{
        resolve_recipes, AsyncRecipeProvider, ChainedRecipeProviderFactory, RecipeProviderEntry,
        RecipeProviderTypeInfo, RecipeProviderTypeMap, RecipesConfig, UnresolvedRecipeProvider,
    },
    value::ValueInterface,
};

/// The library environment's kind: `DefaultKind`'s asset manager, with `liquers-lib`'s recipe
/// default.
///
/// It exists because a *type alias* cannot change what `new()` does. `liquers-lib` and
/// `liquers-core` have always disagreed about the unconfigured recipe provider — the library reads
/// recipes through the store, the core resolves none — and once both are `GenericEnvironment` the
/// only thing that can still carry that difference is the kind parameter. Without this,
/// `DefaultEnvironment<V, ()>` and `SimpleEnvironment<V>` would be the *same type* natively and
/// `DefaultEnvironment::new()` would silently stop resolving `-R/` queries.
///
/// It selects the same asset manager as [`DefaultKind`] — queued natively, inline on wasm — so the
/// execution model is unchanged.
pub struct LibKind;

impl AssetManagerKind for LibKind {
    type Manager<E: Environment> = <DefaultKind as AssetManagerKind>::Manager<E>;

    fn build<E: Environment>(
        envref: EnvRef<E>,
        options: &AssetManagerOptions,
    ) -> Result<Arc<Self::Manager<E>>, Error> {
        DefaultKind::build(envref, options)
    }

    /// The kind's default list, resolved: reads recipes through the environment's store — the
    /// library default, and the reason this kind exists.
    ///
    /// **With `records` on**, the list is `[recipes_yaml, manifest]`: a manifest's explicit and
    /// template-generated chunk keys become addressable from anywhere `-R/…` is, exactly as a
    /// `recipes.yaml` entry is, not only through the source that names them. A chunk name the
    /// folder's `recipes.yaml` also defines is refused by the chain, naming both providers.
    /// Without `records` it is `[recipes_yaml]`.
    ///
    /// `ManifestRecipeProvider` has no bound on `E::Value`, so this compiles for every
    /// [`DefaultEnvironment`], whether or not its value type implements
    /// `liquers_records::RecordValue` — the provider only ever parses a manifest's own bytes,
    /// never the environment's value type.
    ///
    /// A configuration (a document's `recipes:`, `with_recipes`, `with_recipe_provider_choice`)
    /// resolves against the same list, so `default` keeps manifests; an explicit list keeps them
    /// only if it names `manifest` (`specs/design/recipe-provider-chain-config/`).
    fn default_recipe_provider<E: Environment>() -> Arc<dyn AsyncRecipeProvider<E>> {
        match resolve_recipes(
            &RecipesConfig::KindDefault,
            &Self::default_recipes(),
            &Self::recipe_provider_factory::<E>(),
        ) {
            Ok(chain) => Arc::new(chain),
            Err(error) => Arc::new(UnresolvedRecipeProvider::new(error)),
        }
    }

    /// An unconfigured builder resolves the kind's default list, as `new()` does.
    fn unconfigured_recipes() -> RecipesConfig {
        RecipesConfig::KindDefault
    }

    fn default_recipes() -> Vec<RecipeProviderEntry> {
        let mut entries = vec![RecipeProviderEntry::named("recipes_yaml")];
        if cfg!(feature = "records") {
            entries.push(RecipeProviderEntry::named("manifest"));
        }
        entries
    }

    /// `liquers-core`'s entry types plus `manifest`, which is declared unavailable without the
    /// `records` feature so a list naming it says why it cannot be built.
    fn recipe_provider_factory<E: Environment>() -> ChainedRecipeProviderFactory<E> {
        ChainedRecipeProviderFactory::new()
            .chain(Box::new(
                liquers_core::recipes::core_recipe_provider_factory(),
            ))
            .chain(Box::new(lib_recipe_provider_factory::<E>()))
    }
}

/// The recipe provider entry types `liquers-lib` adds: `manifest`.
pub fn lib_recipe_provider_factory<E: Environment>() -> RecipeProviderTypeMap<E> {
    let info = RecipeProviderTypeInfo::new("manifest")
        .with_label("Record-stream manifests")
        .with_doc(
            "Chunk recipes synthesized from each folder's *.manifest.yaml files (liquers-records).",
        );
    #[cfg(feature = "records")]
    {
        RecipeProviderTypeMap::new().with_provider_type(
            info,
            Box::new(|_entry| {
                Ok(Arc::new(liquers_records::ManifestRecipeProvider::new())
                    as Arc<dyn AsyncRecipeProvider<E>>)
            }),
        )
    }
    #[cfg(not(feature = "records"))]
    {
        RecipeProviderTypeMap::new().with_provider_type(
            info.unavailable("enable the records feature of liquers-lib"),
            Box::new(|_entry| {
                Err(Error::configuration_error(
                    "the manifest recipe provider needs the records feature of liquers-lib"
                        .to_string(),
                ))
            }),
        )
    }
}

pub trait CommandRegistryAccess: Environment {
    fn get_mut_command_registry(&mut self) -> &mut CommandRegistry<Self>;
}

/// The library environment: [`GenericEnvironment`] with the asset-manager kind selected by target.
///
/// An alias, not a newtype — `DefaultKind` is [`Queued`](liquers_core::environment_builder::Queued)
/// natively and [`Inline`](liquers_core::environment_builder::Inline) on wasm, which is exactly
/// what the `cfg` import pair this replaced was emulating.
///
/// **Its default recipe provider differs from the core one.** [`LibKind`] resolves its default
/// list (`[recipes_yaml, manifest]` with `records`) when nothing is configured, where a core kind
/// resolves the empty chain and would silently stop resolving `-R/` queries for an application
/// that relied on the library default.
pub type DefaultEnvironment<V, P = ()> = GenericEnvironment<V, P, LibKind>;

/// A builder carrying `liquers-lib`'s defaults.
///
/// The one difference from a core kind's `EnvironmentBuilder::new()` is the recipe provider:
/// [`LibKind`]'s default list reads recipes through the environment's store (and manifests, with
/// `records`), which is what every `liquers-lib` consumer has always got and what `-R/` queries
/// need. A core builder defaults to the empty chain and is right to: it has no opinion about
/// recipes.
pub fn default_environment_builder<V: ValueInterface, P: PayloadType>(
) -> EnvironmentBuilder<V, P, LibKind> {
    // No explicit provider: `LibKind::default_recipe_provider` supplies it, so the builder and the
    // `DefaultEnvironment::new()` constructor cannot drift apart.
    EnvironmentBuilder::new()
}

impl<V: ValueInterface, P: PayloadType> CommandRegistryAccess for DefaultEnvironment<V, P> {
    fn get_mut_command_registry(&mut self) -> &mut CommandRegistry<Self> {
        &mut self.command_registry
    }
}

/// Registers the polars command namespace on an environment or a builder.
///
/// An extension trait rather than an inherent method: [`DefaultEnvironment`] is now an alias of a
/// type defined in `liquers-core`, and Rust permits an inherent `impl` only in the crate that
/// defines the type. Bring the trait into scope and the call site is unchanged.
#[cfg(feature = "polars")]
pub trait PolarsCommandRegistration {
    /// Registers the `pl` namespace.
    fn register_polars_commands(&mut self) -> Result<(), Error>;
}

#[cfg(feature = "polars")]
impl PolarsCommandRegistration for DefaultEnvironment<crate::value::Value> {
    fn register_polars_commands(&mut self) -> Result<(), Error> {
        crate::polars::register_commands(&mut self.command_registry)
    }
}

#[cfg(feature = "polars")]
impl PolarsCommandRegistration for EnvironmentBuilder<crate::value::Value, (), LibKind> {
    fn register_polars_commands(&mut self) -> Result<(), Error> {
        crate::polars::register_commands(&mut self.command_registry)
    }
}

/// Adds `liquers-records`' manifest-serving recipe provider to a builder for the library kind.
///
/// An extension trait for the same reason as [`PolarsCommandRegistration`]: [`EnvironmentBuilder`]
/// is defined in `liquers-core`, so Rust does not permit an inherent `impl` for it here even with
/// `LibKind` fixed.
///
/// [`LibKind`]'s default list already holds `manifest`, and so does any configuration naming
/// `default`; this method matters for an explicit list or a base provider that leaves it out. It
/// appends the `manifest` entry, which is skipped when the chain already has one, so it never adds
/// a second manifest provider.
#[cfg(feature = "records")]
pub trait RecordsRecipeProvider {
    /// Appends the `manifest` entry after this builder's recipe providers unless it is already
    /// among them, so manifest chunk keys resolve regardless of the configured base.
    fn with_records_recipe_provider(self) -> Self;
}

#[cfg(feature = "records")]
impl<V: ValueInterface, P: PayloadType> RecordsRecipeProvider
    for EnvironmentBuilder<V, P, LibKind>
{
    fn with_records_recipe_provider(self) -> Self {
        self.with_appended_recipe_entry(RecipeProviderEntry::named("manifest"))
    }
}

#[cfg(test)]
mod tests {
    use super::DefaultEnvironment;
    use crate::value::Value;
    use liquers_core::context::Environment;

    #[tokio::test]
    async fn default_environment_has_a_recipe_provider() {
        let environment = DefaultEnvironment::<Value>::new();

        let _provider = environment.get_recipe_provider();
    }

    /// Phase 4 decision 1: a build that replaces the base recipe provider
    /// (`with_recipe_provider_choice`, standing in for a configuration document's own choice)
    /// still serves a manifest's keyed chunk once `with_records_recipe_provider` is called —
    /// the base alone (`Trivial`, which resolves nothing) could not.
    #[cfg(feature = "records")]
    #[tokio::test]
    async fn with_records_recipe_provider_serves_a_manifest_chunk_over_a_trivial_base(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use super::{default_environment_builder, RecordsRecipeProvider};
        use liquers_core::context::Context;
        use liquers_core::error::Error;
        use liquers_core::interpreter::evaluate;
        use liquers_core::parse::parse_key;
        use liquers_core::query::Key;
        use liquers_core::recipes::RecipeProviderChoice;
        use liquers_core::store::{AsyncMemoryStore, AsyncStore};
        use liquers_core::value::ValueInterface;
        use liquers_macro::register_command;
        use std::sync::Arc;

        type CommandEnvironment = DefaultEnvironment<Value>;

        fn greet() -> Result<Value, Error> {
            Ok(Value::from("hello"))
        }

        let mut builder = default_environment_builder::<Value, ()>()
            .with_recipe_provider_choice(RecipeProviderChoice::Trivial)
            .with_records_recipe_provider();
        {
            let cr = &mut builder.command_registry;
            register_command!(cr, fn greet() -> result)?;
        }

        let store = AsyncMemoryStore::new(&Key::new());
        let mut metadata = liquers_core::metadata::Metadata::new();
        metadata.set_filename("chunks.manifest.yaml")?;
        store
            .set(
                &parse_key("data/chunks.manifest.yaml")?,
                b"manifest: record-stream\nchunks:\n  - query: greet/greet.csv\n",
                &metadata,
            )
            .await?;
        let envref = builder.with_async_store(Arc::new(store)).build()?;

        // A `Trivial` base resolves no recipe at all, so this can only succeed through the
        // appended `ManifestRecipeProvider`.
        let state = evaluate(envref, "-R/data/greet.csv", None).await?;
        assert_eq!(state.try_into_string()?, "hello");
        Ok(())
    }
}
