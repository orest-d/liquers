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
    recipes::{AsyncRecipeProvider, RecipeProviderChoice},
    value::ValueInterface,
};
#[cfg(feature = "records")]
use liquers_core::recipes::RecipeProviderChain;

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

    /// Reads recipes through the environment's store — the library default, and the reason this
    /// kind exists.
    ///
    /// **With `records` on**, the chain also carries [`liquers_records::ManifestRecipeProvider`],
    /// consulted after [`RecipeProviderChoice::Default`]: a plain `recipes.yaml` entry always wins
    /// (`ManifestRecipeProvider` itself refuses a chunk name a sibling `recipes.yaml` also
    /// defines), and a manifest's explicit and template-generated chunk keys become addressable
    /// from anywhere `-R/…` is, exactly as a `recipes.yaml` entry is — not only through the source
    /// that names them. Without `records`, this is unchanged: `RecipeProviderChoice::Default`
    /// alone, as before the record-streams work.
    ///
    /// `ManifestRecipeProvider` has no bound on `E::Value`, so this compiles for every
    /// [`DefaultEnvironment`], whether or not its value type implements
    /// [`liquers_records::RecordValue`] — the provider only ever parses a manifest's own bytes
    /// into a [`liquers_records::ManifestSpec`], never the environment's value type.
    ///
    /// A build that replaces the *base* provider (`with_recipe_provider` /
    /// `with_recipe_provider_choice`, e.g. from a configuration document) bypasses this default
    /// entirely and loses manifest support unless it also calls
    /// [`RecordsRecipeProvider::with_records_recipe_provider`] (Phase 4 decision 1,
    /// `specs/design/record-streams/phase4-implementation.md`).
    fn default_recipe_provider<E: Environment>() -> Arc<dyn AsyncRecipeProvider<E>> {
        #[cfg(feature = "records")]
        {
            Arc::new(RecipeProviderChain::new(vec![
                RecipeProviderChoice::Default.provider::<E>(),
                Arc::new(liquers_records::ManifestRecipeProvider::new()),
            ]))
        }
        #[cfg(not(feature = "records"))]
        {
            RecipeProviderChoice::Default.provider()
        }
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
/// **Its default recipe provider differs from the core builder's.** Construct it with
/// [`default_environment_builder`], which configures [`RecipeProviderChoice::Default`]; a bare
/// `EnvironmentBuilder::new()` configures `Trivial` and would silently stop resolving `-R/`
/// queries for an application that relied on the library default.
pub type DefaultEnvironment<V, P = ()> = GenericEnvironment<V, P, LibKind>;

/// A builder carrying `liquers-lib`'s defaults.
///
/// The one difference from `EnvironmentBuilder::new()` is the recipe provider:
/// [`RecipeProviderChoice::Default`] reads recipes through the environment's store, which is what
/// every `liquers-lib` consumer has always got and what `-R/` queries need. `liquers-core`'s
/// builder defaults to `Trivial` and is right to: it has no opinion about recipes.
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
/// [`LibKind::default_recipe_provider`] already carries `ManifestRecipeProvider` after
/// [`RecipeProviderChoice::Default`], so an unconfigured [`default_environment_builder`] needs
/// nothing further. This method matters for a build that replaces the *base* provider —
/// [`EnvironmentBuilder::with_recipe_provider`] or
/// [`EnvironmentBuilder::with_recipe_provider_choice`], which a configuration document's own
/// choice would go through — and would otherwise lose keyed-chunk support with nothing to say so
/// (Phase 4 decision 1, `specs/design/record-streams/phase4-implementation.md`: "`liquers-lib`
/// adds `with_records_recipe_provider()` to its builder and calls it on its own configured
/// construction paths, so a server built from a configuration document serves keyed chunks too").
/// `liquers-lib` has no such configured-construction path yet — nothing here calls it — so this is
/// the surface a future one (an `EnvironmentConfig`-driven builder, e.g.) is expected to call.
#[cfg(feature = "records")]
pub trait RecordsRecipeProvider {
    /// Appends [`liquers_records::ManifestRecipeProvider`] after whatever base recipe provider
    /// this builder already has, so manifest chunk keys resolve regardless of the configured base.
    fn with_records_recipe_provider(self) -> Self;
}

#[cfg(feature = "records")]
impl<V: ValueInterface, P: PayloadType> RecordsRecipeProvider
    for EnvironmentBuilder<V, P, LibKind>
{
    fn with_records_recipe_provider(self) -> Self {
        self.with_appended_recipe_provider(Arc::new(liquers_records::ManifestRecipeProvider::new()))
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
