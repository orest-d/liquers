//! One document describing an environment and its store.
//!
//! # What a configuration can and cannot cover
//!
//! Commands are Rust functions registered by a macro, and no document can name one. So a
//! configuration configures *services* and code registers *commands*. [`EnvironmentBuilder`] splits
//! along exactly that line — the `with_*` setters are the config-drivable half, the public
//! `command_registry` field is the code-only half — and this type drives the first half.
//!
//! ```yaml
//! store:
//!   stores:
//!     - type: filesystem
//!       prefix: data
//!       config:
//!         path: ${LIQUERS_DATA}
//!     - type: memory
//!       prefix: tmp
//! recipes: default
//! assets:
//!   job_capacity: 8
//! ```
//!
//! ```ignore
//! let config = EnvironmentConfig::from_yaml(&std::fs::read_to_string("environment.yaml")?)?;
//! let mut builder = EnvironmentBuilder::<Value>::new()
//!     .with_config(config, Box::new(default_store_factory()));
//! register_my_commands(&mut builder.command_registry)?;   // code, not config
//! let envref = builder.build()?;
//! ```
//!
//! # Two things that are deliberately absent
//!
//! **The asset-manager kind.** A YAML string cannot select a type: `"queued"` and `"inline"`
//! produce two different concrete environment types, and [`Environment`](crate::context::Environment)
//! is not object-safe, so they cannot be erased behind a `dyn`. The choice is a *build* fact rather
//! than a deployment one — wasm has no choice at all, and natively the inline manager exists for
//! deterministic testing rather than production tuning. An application that genuinely wants runtime
//! selection monomorphizes its own tail with an explicit two-arm match.
//!
//! **The store factories.** Which backends exist is a build fact for the same reason:
//! `liquers-core` supplies memory and filesystem, `liquers-store` chains OpenDAL onto them, and
//! `liquers-web` chains its own. So the factory reaches the builder as an argument, and the
//! document names store *types* the chain is expected to resolve.

use crate::environment_builder::{
    AssetManagerKind, AssetManagerOptions, EnvironmentBuilder,
};
use crate::error::Error;
use crate::store_config::StoreRouterConfig;
use crate::store_factory::StoreFactory;
use crate::commands::PayloadType;
use crate::recipes::RecipeProviderChoice;
use crate::value::ValueInterface;

/// Everything about an environment that can be written down rather than compiled in.
///
/// Every field has a serde default, so a document may configure one section and leave the rest —
/// and so a field added later does not break an existing document.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    /// The store router, verbatim [`StoreRouterConfig`]. Reused rather than re-specified, so the
    /// store half of this document is exactly the store configuration format.
    #[serde(default)]
    pub store: StoreRouterConfig,

    /// Which built-in recipe provider.
    ///
    /// Absent means [`RecipeProviderChoice::Default`], the *document* default: a configuration
    /// saying nothing about recipes most plausibly wants them to work. Note this is **not** the
    /// unconfigured default of [`EnvironmentBuilder::new`], which is `Trivial`. Applying a
    /// configuration is an explicit act, and it says so — a bare builder resolves recipes
    /// trivially, and the same builder with `EnvironmentConfig::default()` applied resolves them
    /// through the store.
    #[serde(default)]
    pub recipes: RecipeProviderChoice,

    /// Per-manager settings. The manager *kind* is not here; see the module documentation.
    #[serde(default)]
    pub assets: AssetManagerOptions,
}

impl EnvironmentConfig {
    /// An empty configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses a YAML document.
    pub fn from_yaml(yaml: &str) -> Result<Self, Error> {
        serde_yaml::from_str(yaml).map_err(|e| {
            Error::general_error(format!("Failed to parse environment configuration YAML: {e}"))
        })
    }

    /// Parses a JSON document.
    pub fn from_json(json: &str) -> Result<Self, Error> {
        serde_json::from_str(json).map_err(|e| {
            Error::general_error(format!("Failed to parse environment configuration JSON: {e}"))
        })
    }

    /// Parses a TOML document.
    #[cfg(feature = "toml")]
    pub fn from_toml(toml_str: &str) -> Result<Self, Error> {
        toml::from_str(toml_str).map_err(|e| {
            Error::general_error(format!("Failed to parse environment configuration TOML: {e}"))
        })
    }

    /// Serializes to YAML.
    pub fn to_yaml(&self) -> Result<String, Error> {
        serde_yaml::to_string(self).map_err(|e| {
            Error::general_error(format!(
                "Failed to serialize environment configuration to YAML: {e}"
            ))
        })
    }

    /// Serializes to JSON.
    pub fn to_json(&self) -> Result<String, Error> {
        serde_json::to_string_pretty(self).map_err(|e| {
            Error::general_error(format!(
                "Failed to serialize environment configuration to JSON: {e}"
            ))
        })
    }

    /// Expands `${VAR}` references in the store section.
    ///
    /// The expander errors on an unset variable and has no default-value syntax, so a missing
    /// variable fails loudly rather than producing an empty path. [`EnvironmentBuilder::build`]
    /// calls this, so an ordinary caller does not have to remember it.
    pub fn expand_env_vars(&mut self) -> Result<(), Error> {
        self.store.expand_env_vars()
    }
}

impl<V: ValueInterface, P: PayloadType, K: AssetManagerKind> EnvironmentBuilder<V, P, K> {
    /// Applies a whole configuration document: store, recipes and manager options at once.
    ///
    /// Equivalent to the three matching setters, and composes with hand-written configuration in
    /// either order — document first then overridden in code, or the reverse. Commands are the
    /// caller's job either way.
    pub fn with_config(self, config: EnvironmentConfig, factory: Box<dyn StoreFactory>) -> Self {
        self.with_store_config(config.store, factory)
            .with_recipe_provider_choice(config.recipes)
            .with_asset_manager_options(config.assets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::AssetManager;
    use crate::environment_builder::Inline;
    use crate::store_factory::default_store_factory;
    use crate::value::Value;

    const SAMPLE: &str = r#"
store:
  stores:
    - type: memory
      prefix: tmp
recipes: trivial
assets:
  job_capacity: 8
"#;

    /// T15: a document round-trips through YAML and JSON, and reaches the builder.
    #[test]
    fn config_roundtrips_and_applies() {
        let config = EnvironmentConfig::from_yaml(SAMPLE).expect("parse yaml");
        assert_eq!(config.store.stores.len(), 1);
        assert_eq!(config.store.stores[0].store_type, "memory");
        assert_eq!(config.recipes, RecipeProviderChoice::Trivial);
        assert_eq!(config.assets.job_capacity, Some(8));

        let json = config.to_json().expect("to json");
        let from_json = EnvironmentConfig::from_json(&json).expect("from json");
        assert_eq!(from_json.store.stores[0].prefix, "tmp");
        assert_eq!(from_json.recipes, RecipeProviderChoice::Trivial);

        let yaml = config.to_yaml().expect("to yaml");
        let from_yaml = EnvironmentConfig::from_yaml(&yaml).expect("from yaml");
        assert_eq!(from_yaml.assets.job_capacity, Some(8));
    }

    /// T15 (the asymmetry half): an absent `recipes:` key means `Default`, which is **not** the
    /// bare builder's `Trivial`.
    ///
    /// Deliberate, and the kind of default that is obvious in a reference and surprising in
    /// practice, so it is pinned here: applying an otherwise-empty configuration changes how
    /// recipes resolve.
    #[test]
    fn an_absent_recipes_key_means_default_not_trivial() {
        let config = EnvironmentConfig::from_yaml("store:\n  stores: []\n").expect("parse");
        assert_eq!(
            config.recipes,
            RecipeProviderChoice::Default,
            "the document default is Default — a config saying nothing about recipes wants them"
        );
        assert_eq!(
            EnvironmentConfig::default().recipes,
            RecipeProviderChoice::Default
        );
    }

    /// The manager kind is absent from the document, so a document naming one is rejected rather
    /// than silently ignored. (`deny_unknown_fields` is not set, so this documents current
    /// behaviour: unknown keys are ignored. Recorded so a later tightening is a deliberate choice.)
    #[test]
    fn an_unknown_key_is_currently_ignored() {
        let config = EnvironmentConfig::from_yaml("manager: queued\n").expect("parse");
        assert_eq!(config.recipes, RecipeProviderChoice::Default);
        assert!(config.store.stores.is_empty());
    }

    /// T16: a store type no factory in the chain claims fails at `build()`, not at the setter, and
    /// the error names what the chain does support.
    #[test]
    fn config_errors_surface_at_build() {
        let config =
            EnvironmentConfig::from_yaml("store:\n  stores:\n    - type: nonexistent\n")
                .expect("parse");

        let result = EnvironmentBuilder::<Value, (), Inline>::new()
            .with_config(config, Box::new(default_store_factory()))
            .build();

        match result {
            Ok(_) => panic!("an unresolvable store type must fail the build"),
            Err(e) => {
                let message = e.to_string();
                assert!(
                    message.contains("nonexistent"),
                    "the error must name the offending type, got: {message}"
                );
            }
        }
    }

    /// T16 (expansion half): an unset `${VAR}` fails the build rather than producing an empty
    /// value.
    #[test]
    fn an_unset_environment_variable_fails_the_build() {
        let config = EnvironmentConfig::from_yaml(
            "store:\n  stores:\n    - type: memory\n      prefix: tmp\n      config:\n        path: ${LIQUERS_DEFINITELY_UNSET_VARIABLE}\n",
        )
        .expect("parse");

        let result = EnvironmentBuilder::<Value, (), Inline>::new()
            .with_config(config, Box::new(default_store_factory()))
            .build();

        assert!(
            result.is_err(),
            "an unset ${{VAR}} must fail the build; the expander has no default-value syntax"
        );
    }

    /// A configured store actually reaches the environment.
    #[test]
    fn a_configured_store_is_installed() {
        let config = EnvironmentConfig::from_yaml(
            "store:\n  stores:\n    - type: memory\n      prefix: tmp\nrecipes: trivial\n",
        )
        .expect("parse");

        let envref = EnvironmentBuilder::<Value, (), Inline>::new()
            .with_config(config, Box::new(default_store_factory()))
            .build()
            .expect("build");

        // The router is installed rather than the NoAsyncStore default.
        let _store = envref.get_async_store();
        assert!(envref.get_asset_manager().is_started());
    }
}
