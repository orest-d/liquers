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
    commands::{CommandRegistry, PayloadType},
    context::{Environment, GenericEnvironment},
    environment_builder::{DefaultKind, EnvironmentBuilder},
    error::Error,
    recipes::RecipeProviderChoice,
    value::ValueInterface,
};

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
pub type DefaultEnvironment<V, P = ()> = GenericEnvironment<V, P, DefaultKind>;

/// A builder carrying `liquers-lib`'s defaults.
///
/// The one difference from `EnvironmentBuilder::new()` is the recipe provider:
/// [`RecipeProviderChoice::Default`] reads recipes through the environment's store, which is what
/// every `liquers-lib` consumer has always got and what `-R/` queries need. `liquers-core`'s
/// builder defaults to `Trivial` and is right to: it has no opinion about recipes.
pub fn default_environment_builder<V: ValueInterface, P: PayloadType>(
) -> EnvironmentBuilder<V, P, DefaultKind> {
    EnvironmentBuilder::new().with_recipe_provider_choice(RecipeProviderChoice::Default)
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
impl PolarsCommandRegistration
    for EnvironmentBuilder<crate::value::Value, (), DefaultKind>
{
    fn register_polars_commands(&mut self) -> Result<(), Error> {
        crate::polars::register_commands(&mut self.command_registry)
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
}
