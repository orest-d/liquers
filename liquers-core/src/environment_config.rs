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
//! recipes:                 # absent or `default`: the kind's list; `trivial` or `[]`: none
//!   - recipes_yaml
//!   - manifest
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
use crate::recipes::RecipesConfig;
use crate::value::ValueInterface;
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

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

    /// The recipe-provider chain, as a list of entries (`specs/design/recipe-provider-chain-config/`).
    ///
    /// Absent (or `default`) means [`RecipesConfig::KindDefault`], the *document* default: the
    /// manager kind's default list (`[recipes_yaml]` in core, `[recipes_yaml, manifest]` in
    /// `liquers-lib` with `records`). A configuration saying nothing about recipes most plausibly
    /// wants them to work. This is **not** the unconfigured default of a bare core
    /// [`EnvironmentBuilder::new`], which is the empty chain. `trivial` (aliases `none`,
    /// `no_recipes`) is the empty chain `[]`. Written back, `KindDefault` omits the key.
    #[serde(default, skip_serializing_if = "RecipesConfig::is_kind_default")]
    pub recipes: RecipesConfig,

    /// Per-manager settings. The manager *kind* is not here; see the module documentation.
    #[serde(default)]
    pub assets: AssetManagerOptions,

    /// Namespaces a query searches after those of its last `ns`, highest priority first; root is
    /// appended unless listed (`specs/design/plan-default-namespaces/`). Absent: the command
    /// registry's own chain stands. Present: it replaces it. Meant for the main realm; the registry
    /// holds one chain for every realm until `COMMAND-REGISTRY-NOT-ORGANIZED-BY-REALM`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_namespaces: Option<Vec<String>>,
}

impl EnvironmentConfig {
    /// An empty configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses a YAML document.
    pub fn from_yaml(yaml: &str) -> Result<Self, Error> {
        serde_yaml::from_str(yaml).map_err(|e| {
            Error::parse_error(format!("Failed to parse environment configuration YAML: {e}"))
        })
    }

    /// Parses a JSON document.
    pub fn from_json(json: &str) -> Result<Self, Error> {
        serde_json::from_str(json).map_err(|e| {
            Error::parse_error(format!("Failed to parse environment configuration JSON: {e}"))
        })
    }

    /// Parses a TOML document.
    #[cfg(feature = "toml")]
    pub fn from_toml(toml_str: &str) -> Result<Self, Error> {
        toml::from_str(toml_str).map_err(|e| {
            Error::parse_error(format!("Failed to parse environment configuration TOML: {e}"))
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

/// One configuration document for a whole host: the environment's keys, the command metadata
/// registry file, and the host's own sections.
///
/// The document's top level is split three ways. The keys of [`Self::ENVIRONMENT_KEYS`] become the
/// [`EnvironmentConfig`] (`environment`), exactly as [`EnvironmentConfig::from_yaml`] reads them;
/// `command_registry` names a command metadata registry file (resolved against `base`, the
/// document's folder, by [`Self::command_registry_path`]); every other key is a *section* that waits
/// for its host. A host takes its sections with [`Self::take_section`] (a server's `server`, the
/// Python host's `python`) and then calls [`Self::finish`], which refuses any section nobody took,
/// naming it and the keys the host knows. So a typo (`sever:`) fails loudly instead of being
/// ignored.
///
/// Sections are plain data: YAML is converted to JSON values first, so a YAML tag (`!Value`) or a
/// non-string key is a parse error, and an empty document (`null`) is the empty map, the defaults.
///
/// ```
/// use liquers_core::environment_config::ConfigDocument;
/// use std::collections::BTreeMap;
///
/// let mut doc = ConfigDocument::from_yaml("store:\n  stores: []\nserver:\n  port: 9000\n").unwrap();
/// // A host parses its section into its own type (here a plain map).
/// let server: Option<BTreeMap<String, u16>> = doc.take_section("server").unwrap();
/// assert_eq!(server.unwrap()["port"], 9000);
/// let environment = doc.finish().unwrap();     // nothing left: the document is accepted
/// assert!(environment.store.stores.is_empty());
///
/// let doc = ConfigDocument::from_yaml("sever:\n  port: 9000\n").unwrap();
/// assert!(doc.finish().unwrap_err().to_string().contains("sever"));
/// ```
#[derive(Debug, Clone, Default)]
pub struct ConfigDocument {
    /// The environment's keys, parsed as an [`EnvironmentConfig`].
    pub environment: EnvironmentConfig,
    /// The `command_registry` value as written: a command metadata registry file.
    pub command_registry: Option<String>,
    /// The document's folder when it was read from a file ([`Self::load`]); only
    /// `command_registry` resolves against it (store paths stay relative to the working directory).
    pub base: Option<PathBuf>,
    sections: BTreeMap<String, serde_json::Value>,
    taken: Vec<String>,
}

impl ConfigDocument {
    /// The top-level keys that belong to [`EnvironmentConfig`].
    pub const ENVIRONMENT_KEYS: &'static [&'static str] =
        &["store", "recipes", "assets", "default_namespaces"];

    /// The key naming the command metadata registry file.
    pub const COMMAND_REGISTRY_KEY: &'static str = "command_registry";

    /// Parses a YAML document. Tags and non-string keys are refused (`ParseError`).
    pub fn from_yaml(text: &str) -> Result<Self, Error> {
        let value: serde_yaml::Value = serde_yaml::from_str(text).map_err(|e| {
            Error::parse_error(format!("Failed to parse the configuration document YAML: {e}"))
        })?;
        Self::from_value(yaml_to_json(value, "")?)
    }

    /// Parses a JSON document.
    pub fn from_json(text: &str) -> Result<Self, Error> {
        let value: serde_json::Value = serde_json::from_str(text).map_err(|e| {
            Error::parse_error(format!("Failed to parse the configuration document JSON: {e}"))
        })?;
        Self::from_value(value)
    }

    /// Parses a TOML document.
    #[cfg(feature = "toml")]
    pub fn from_toml(text: &str) -> Result<Self, Error> {
        let value: toml::Value = toml::from_str(text).map_err(|e| {
            Error::parse_error(format!("Failed to parse the configuration document TOML: {e}"))
        })?;
        let value = serde_json::to_value(value).map_err(|e| {
            Error::parse_error(format!("Failed to convert the configuration document TOML: {e}"))
        })?;
        Self::from_value(value)
    }

    /// Reads a document from a file, by extension: `.yaml` / `.yml`, `.json`, or `.toml` (which
    /// needs this crate's `toml` feature). Sets [`Self::base`] to the file's folder.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load(path: &Path) -> Result<Self, Error> {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        let parse: fn(&str) -> Result<Self, Error> = match extension.as_str() {
            "yaml" | "yml" => Self::from_yaml,
            "json" => Self::from_json,
            #[cfg(feature = "toml")]
            "toml" => Self::from_toml,
            #[cfg(not(feature = "toml"))]
            "toml" => {
                return Err(Error::configuration_error(format!(
                    "cannot read {}: TOML configuration documents need liquers-core's `toml` feature",
                    path.display()
                )))
            }
            other => {
                return Err(Error::configuration_error(format!(
                    "cannot read {}: unknown configuration document extension '{other}' (expected .yaml, .yml, .json or .toml)",
                    path.display()
                )))
            }
        };
        let text = std::fs::read_to_string(path).map_err(|e| {
            Error::general_error(format!(
                "cannot read the configuration document {}: {e}",
                path.display()
            ))
        })?;
        let mut doc = parse(&text).map_err(|mut e| {
            e.message = format!("{}: {}", path.display(), e.message);
            e
        })?;
        doc.base = Some(match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
            Some(_) | None => PathBuf::from("."),
        });
        Ok(doc)
    }

    fn from_value(value: serde_json::Value) -> Result<Self, Error> {
        let map = match value {
            serde_json::Value::Null => serde_json::Map::new(),
            serde_json::Value::Object(map) => map,
            other => {
                return Err(Error::parse_error(format!(
                    "a configuration document must be a mapping of sections, not {}",
                    json_kind(&other)
                )))
            }
        };
        let mut environment = serde_json::Map::new();
        let mut command_registry = None;
        let mut sections = BTreeMap::new();
        for (key, value) in map {
            if Self::ENVIRONMENT_KEYS.contains(&key.as_str()) {
                environment.insert(key, value);
            } else if key == Self::COMMAND_REGISTRY_KEY {
                match value {
                    serde_json::Value::String(path) => command_registry = Some(path),
                    serde_json::Value::Null => {}
                    other => {
                        return Err(Error::configuration_error(format!(
                            "'command_registry' must name a file, not {}",
                            json_kind(&other)
                        )))
                    }
                }
            } else {
                sections.insert(key, value);
            }
        }
        let environment: EnvironmentConfig =
            serde_json::from_value(serde_json::Value::Object(environment)).map_err(|e| {
                Error::parse_error(format!("Failed to parse environment configuration: {e}"))
            })?;
        Ok(ConfigDocument {
            environment,
            command_registry,
            base: None,
            sections,
            taken: Vec::new(),
        })
    }

    /// Takes the section `name`, if the document has one, parsed as `T`. An empty section
    /// (`server:` with nothing under it) parses as an empty map. A bad value or unknown key (for a
    /// `T` with `deny_unknown_fields`) is a `ConfigurationError` naming the section. The name is
    /// remembered as known, so [`Self::finish`] lists it.
    pub fn take_section<T: DeserializeOwned>(&mut self, name: &str) -> Result<Option<T>, Error> {
        if !self.taken.iter().any(|t| t == name) {
            self.taken.push(name.to_string());
        }
        match self.sections.remove(name) {
            None => Ok(None),
            Some(value) => {
                let value = match value {
                    serde_json::Value::Null => serde_json::Value::Object(serde_json::Map::new()),
                    other => other,
                };
                serde_json::from_value(value).map(Some).map_err(|e| {
                    Error::configuration_error(format!("in section '{name}': {e}"))
                })
            }
        }
    }

    /// The names of the sections not taken yet.
    pub fn remaining_sections(&self) -> Vec<String> {
        self.sections.keys().cloned().collect()
    }

    /// The known top-level keys: the environment's, `command_registry`, and every section a host
    /// asked for with [`Self::take_section`].
    pub fn known_keys(&self) -> Vec<String> {
        Self::ENVIRONMENT_KEYS
            .iter()
            .map(|k| k.to_string())
            .chain(std::iter::once(Self::COMMAND_REGISTRY_KEY.to_string()))
            .chain(self.taken.iter().cloned())
            .collect()
    }

    /// Ends the host's reading: returns the [`EnvironmentConfig`], or a `ConfigurationError`
    /// naming every section nobody took and the keys this host knows.
    pub fn finish(self) -> Result<EnvironmentConfig, Error> {
        if self.sections.is_empty() {
            return Ok(self.environment);
        }
        let unknown = self
            .sections
            .keys()
            .map(|k| format!("'{k}'"))
            .collect::<Vec<_>>()
            .join(", ");
        let plural = if self.sections.len() == 1 { "" } else { "s" };
        Err(Error::configuration_error(format!(
            "unknown section{plural} {unknown} (this host knows: {})",
            self.known_keys().join(", ")
        )))
    }

    /// `command_registry` resolved against [`Self::base`] (as written when there is no base or the
    /// path is absolute).
    pub fn command_registry_path(&self) -> Option<PathBuf> {
        self.command_registry.as_ref().map(|file| {
            let path = PathBuf::from(file);
            match &self.base {
                Some(base) if path.is_relative() => base.join(path),
                Some(_) | None => path,
            }
        })
    }
}

fn json_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "a list",
        serde_json::Value::Object(_) => "a mapping",
    }
}

/// Converts a YAML value to plain JSON data, refusing what JSON cannot say: tags, non-string keys,
/// non-finite numbers. `at` names the position for the error.
fn yaml_to_json(value: serde_yaml::Value, at: &str) -> Result<serde_json::Value, Error> {
    let place = |at: &str| if at.is_empty() { "the top level".to_string() } else { format!("'{at}'") };
    match value {
        serde_yaml::Value::Null => Ok(serde_json::Value::Null),
        serde_yaml::Value::Bool(b) => Ok(serde_json::Value::Bool(b)),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(serde_json::Value::from(i))
            } else if let Some(u) = n.as_u64() {
                Ok(serde_json::Value::from(u))
            } else {
                let f = n.as_f64().unwrap_or(f64::NAN);
                serde_json::Number::from_f64(f).map(serde_json::Value::Number).ok_or_else(|| {
                    Error::parse_error(format!(
                        "configuration document: the number {n} at {} is not finite",
                        place(at)
                    ))
                })
            }
        }
        serde_yaml::Value::String(s) => Ok(serde_json::Value::String(s)),
        serde_yaml::Value::Sequence(items) => items
            .into_iter()
            .enumerate()
            .map(|(i, item)| yaml_to_json(item, &format!("{at}[{i}]")))
            .collect::<Result<Vec<_>, _>>()
            .map(serde_json::Value::Array),
        serde_yaml::Value::Mapping(mapping) => {
            let mut map = serde_json::Map::new();
            for (key, item) in mapping {
                let key = match key {
                    serde_yaml::Value::String(key) => key,
                    other => {
                        return Err(Error::parse_error(format!(
                            "configuration document: a key at {} is not a string ({}); sections are plain data",
                            place(at),
                            serde_yaml::to_string(&other).unwrap_or_default().trim()
                        )))
                    }
                };
                let inner = if at.is_empty() { key.clone() } else { format!("{at}.{key}") };
                let item = yaml_to_json(item, &inner)?;
                map.insert(key, item);
            }
            Ok(serde_json::Value::Object(map))
        }
        serde_yaml::Value::Tagged(tagged) => Err(Error::parse_error(format!(
            "configuration document: the YAML tag {} at {} is not allowed; sections are plain data",
            tagged.tag,
            place(at)
        ))),
    }
}

impl<V: ValueInterface, P: PayloadType, K: AssetManagerKind> EnvironmentBuilder<V, P, K> {
    /// Applies a whole configuration document: store, recipes, manager options and default
    /// namespaces at once.
    ///
    /// Equivalent to the matching setters, and composes with hand-written configuration in
    /// either order — document first then overridden in code, or the reverse. Commands are the
    /// caller's job either way.
    pub fn with_config(self, config: EnvironmentConfig, factory: Box<dyn StoreFactory>) -> Self {
        let builder = self
            .with_store_config(config.store, factory)
            .with_recipes(config.recipes)
            .with_asset_manager_options(config.assets);
        match config.default_namespaces {
            Some(namespaces) => builder.with_default_namespaces(namespaces),
            None => builder,
        }
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

    /// T5: a malformed environment document is a parse error, as a malformed store document is.
    #[test]
    fn malformed_environment_document_is_parse_error() {
        use crate::error::ErrorType;
        let yaml = EnvironmentConfig::from_yaml("store: [unclosed").expect_err("bad yaml");
        assert_eq!(yaml.error_type, ErrorType::ParseError);
        let json = EnvironmentConfig::from_json("{").expect_err("bad json");
        assert_eq!(json.error_type, ErrorType::ParseError);
        #[cfg(feature = "toml")]
        {
            let toml = EnvironmentConfig::from_toml("=").expect_err("bad toml");
            assert_eq!(toml.error_type, ErrorType::ParseError);
        }
    }

    /// T15: a document round-trips through YAML and JSON, and reaches the builder.
    #[test]
    fn config_roundtrips_and_applies() {
        let config = EnvironmentConfig::from_yaml(SAMPLE).expect("parse yaml");
        assert_eq!(config.store.stores.len(), 1);
        assert_eq!(config.store.stores[0].store_type, "memory");
        assert_eq!(config.recipes, RecipesConfig::Chain(Vec::new()));
        assert_eq!(config.assets.job_capacity, Some(8));

        let json = config.to_json().expect("to json");
        let from_json = EnvironmentConfig::from_json(&json).expect("from json");
        assert_eq!(from_json.store.stores[0].prefix, "tmp");
        assert_eq!(from_json.recipes, RecipesConfig::Chain(Vec::new()));

        let yaml = config.to_yaml().expect("to yaml");
        let from_yaml = EnvironmentConfig::from_yaml(&yaml).expect("from yaml");
        assert_eq!(from_yaml.assets.job_capacity, Some(8));
        assert_eq!(from_yaml.recipes, RecipesConfig::Chain(Vec::new()));

        // AC-10: the kind default is written by omitting the key, so it reads back as itself.
        let default_yaml = EnvironmentConfig::default().to_yaml().expect("to yaml");
        assert!(!default_yaml.contains("recipes"), "{default_yaml}");
        let reread = EnvironmentConfig::from_yaml(&default_yaml).expect("from yaml");
        assert_eq!(reread.recipes, RecipesConfig::KindDefault);
    }

    /// T15 (the asymmetry half): an absent `recipes:` key means the kind default, which is
    /// **not** the bare core builder's empty chain.
    ///
    /// Deliberate, and the kind of default that is obvious in a reference and surprising in
    /// practice, so it is pinned here: applying an otherwise-empty configuration changes how
    /// recipes resolve.
    #[test]
    fn an_absent_recipes_key_means_default_not_trivial() {
        let config = EnvironmentConfig::from_yaml("store:\n  stores: []\n").expect("parse");
        assert_eq!(
            config.recipes,
            RecipesConfig::KindDefault,
            "the document default is the kind default — a config saying nothing about recipes wants them"
        );
        assert_eq!(
            EnvironmentConfig::default().recipes,
            RecipesConfig::KindDefault
        );
    }

    /// The manager kind is absent from the document, so a document naming one is rejected rather
    /// than silently ignored. (`deny_unknown_fields` is not set, so this documents current
    /// behaviour: unknown keys are ignored. Recorded so a later tightening is a deliberate choice.)
    #[test]
    fn an_unknown_key_is_currently_ignored() {
        let config = EnvironmentConfig::from_yaml("manager: queued\n").expect("parse");
        assert_eq!(config.recipes, RecipesConfig::KindDefault);
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
    #[test]
    fn config_with_and_without_assets_section_parses() {
        use crate::environment_builder::DependencyAuditPolicy;
        let without = EnvironmentConfig::from_yaml("recipes: trivial\n").expect("parse");
        assert_eq!(without.assets, AssetManagerOptions::default());
        assert_eq!(without.assets.dependency_audit, DependencyAuditPolicy::Explicit);

        let with = EnvironmentConfig::from_yaml("assets:\n  dependency_audit: on_load\n").expect("parse");
        assert_eq!(with.assets.dependency_audit, DependencyAuditPolicy::OnLoad);
        assert_eq!(with.assets.job_capacity, None);

        let yaml = with.to_yaml().expect("to yaml");
        let again = EnvironmentConfig::from_yaml(&yaml).expect("reparse");
        assert_eq!(again.assets.dependency_audit, DependencyAuditPolicy::OnLoad);
    }

    /// plan-policy: the three caching keys under `assets:` parse, reach the manager, are absent
    /// from a serialized document at their defaults, and an empty document means today's
    /// behaviour (`all` / `all` / cutting on).
    #[test]
    fn cache_strategy_keys_round_trip_and_default() {
        use crate::assets::AssetManager;
        use crate::cache_strategy::CacheStrategy;

        let empty = EnvironmentConfig::from_yaml("recipes: trivial\n").expect("parse");
        assert_eq!(empty.assets.recipe_cache_strategy, CacheStrategy::All);
        assert_eq!(empty.assets.query_cache_strategy, CacheStrategy::All);
        assert!(empty.assets.cut_predecessors());
        let yaml = empty.to_yaml().expect("to yaml");
        assert!(!yaml.contains("cache_strategy"), "defaults are not written: {yaml}");
        assert!(!yaml.contains("cut_predecessors"), "defaults are not written: {yaml}");

        let doc = "assets:\n  recipe_cache_strategy: result\n  query_cache_strategy: none\n  cut_predecessors: false\n";
        let config = EnvironmentConfig::from_yaml(doc).expect("parse");
        assert_eq!(config.assets.recipe_cache_strategy, CacheStrategy::Result);
        assert_eq!(config.assets.query_cache_strategy, CacheStrategy::None);
        assert!(!config.assets.cut_predecessors());
        let again = EnvironmentConfig::from_yaml(&config.to_yaml().expect("to yaml")).expect("reparse");
        assert_eq!(again.assets, config.assets);

        let envref = crate::environment_builder::EnvironmentBuilder::<Value, (), Inline>::new()
            .with_config(config, Box::new(default_store_factory()))
            .build()
            .expect("build");
        let manager = envref.get_asset_manager();
        assert_eq!(manager.recipe_cache_strategy(), CacheStrategy::Result);
        assert_eq!(manager.query_cache_strategy(), CacheStrategy::None);
        assert!(!manager.cut_predecessors());

        assert!(EnvironmentConfig::from_yaml("assets:\n  query_cache_strategy: some\n").is_err());
    }

    /// AC-6: `default_namespaces` round-trips in order, and an absent key is not written.
    #[test]
    fn default_namespaces_round_trip() {
        let config = EnvironmentConfig::from_yaml("default_namespaces: [pl, lui]\n").expect("parse");
        assert_eq!(config.default_namespaces, Some(vec!["pl".to_string(), "lui".to_string()]));
        let json = config.to_json().expect("to json");
        let back = EnvironmentConfig::from_json(&json).expect("from json");
        assert_eq!(back.default_namespaces, config.default_namespaces);

        let absent = EnvironmentConfig::from_yaml("store:\n  stores: []\n").expect("parse");
        assert_eq!(absent.default_namespaces, None);
        let yaml = absent.to_yaml().expect("to yaml");
        assert!(!yaml.contains("default_namespaces"), "{yaml}");
    }

    /// AC-6: a present key replaces a chain set in code; an absent key leaves the setter's alone;
    /// `[]` means root only.
    #[test]
    fn absent_default_namespaces_keeps_builder_setting() {
        let build = |yaml: &str| {
            EnvironmentBuilder::<Value, (), Inline>::new()
                .with_default_namespaces(["lui"])
                .with_config(EnvironmentConfig::from_yaml(yaml).expect("parse"), Box::new(default_store_factory()))
                .build()
                .expect("build")
        };
        assert_eq!(build("store:\n  stores: []\n").default_namespaces(), ["lui", "", "root"]);
        assert_eq!(build("default_namespaces: [pl]\n").default_namespaces(), ["pl", "", "root"]);
        assert_eq!(build("default_namespaces: []\n").default_namespaces(), ["", "root"]);
    }

    /// configuration-document AC-3: environment keys reach `EnvironmentConfig`, `command_registry`
    /// resolves against `base`, other sections wait for their host.
    #[test]
    fn document_splits_environment_and_sections() {
        let mut doc = ConfigDocument::from_yaml(
            "store:\n  stores:\n    - {type: memory, prefix: tmp}\nrecipes: trivial\nassets: {job_capacity: 3}\ndefault_namespaces: [pl]\ncommand_registry: commands.yaml\nserver: {port: 9000}\npython: {pickle: true}\n",
        )
        .expect("parse");
        assert_eq!(doc.environment.store.stores[0].prefix, "tmp");
        assert_eq!(doc.environment.recipes, RecipesConfig::Chain(Vec::new()));
        assert_eq!(doc.environment.assets.job_capacity, Some(3));
        assert_eq!(doc.environment.default_namespaces, Some(vec!["pl".to_string()]));
        assert_eq!(doc.command_registry.as_deref(), Some("commands.yaml"));
        assert_eq!(doc.command_registry_path(), Some(PathBuf::from("commands.yaml")));
        doc.base = Some(PathBuf::from("/srv/app"));
        assert_eq!(doc.command_registry_path(), Some(PathBuf::from("/srv/app/commands.yaml")));
        assert_eq!(doc.remaining_sections(), vec!["python".to_string(), "server".to_string()]);

        let server: Option<serde_json::Value> = doc.take_section("server").expect("server");
        assert_eq!(server, Some(serde_json::json!({"port": 9000})));
        let python: Option<serde_json::Value> = doc.take_section("python").expect("python");
        assert!(python.is_some());
        let missing: Option<serde_json::Value> = doc.take_section("ui").expect("absent");
        assert!(missing.is_none());
        let environment = doc.finish().expect("all taken");
        assert_eq!(environment.assets.job_capacity, Some(3));
    }

    /// configuration-document AC-2: an untaken section fails `finish`, naming it and the known keys.
    #[test]
    fn finish_refuses_an_untaken_section() {
        use crate::error::ErrorType;
        let mut doc = ConfigDocument::from_yaml("server: {port: 8080}\nsever: {port: 9000}\n").expect("parse");
        let _: Option<serde_json::Value> = doc.take_section("server").expect("server");
        let _: Option<serde_json::Value> = doc.take_section("python").expect("python");
        let error = doc.finish().expect_err("sever is unknown");
        assert_eq!(error.error_type, ErrorType::ConfigurationError);
        let message = error.to_string();
        assert!(message.contains("unknown section 'sever'"), "{message}");
        assert!(
            message.contains("store, recipes, assets, default_namespaces, command_registry, server, python"),
            "{message}"
        );
    }

    /// A section with a bad value is a configuration error naming the section.
    #[test]
    fn a_bad_section_value_names_the_section() {
        use crate::error::ErrorType;
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        #[allow(dead_code)]
        struct Server {
            port: u16,
        }
        let mut doc = ConfigDocument::from_yaml("server: {prot: 1}\n").expect("parse");
        let error = doc.take_section::<Server>("server").expect_err("unknown key");
        assert_eq!(error.error_type, ErrorType::ConfigurationError);
        assert!(error.to_string().contains("server") && error.to_string().contains("prot"), "{error}");
        let mut empty = ConfigDocument::from_yaml("python:\n").expect("parse");
        let section: Option<BTreeMap<String, String>> = empty.take_section("python").expect("empty");
        assert_eq!(section, Some(BTreeMap::new()));
    }

    /// configuration-document AC-6: `ENVIRONMENT_KEYS` is exactly the struct's serialized keys,
    /// and a fully set configuration round-trips through the document unchanged.
    #[test]
    fn environment_keys_match_the_struct() {
        let full = EnvironmentConfig::from_yaml(
            "store:\n  stores:\n    - {type: memory, prefix: tmp}\nrecipes: [recipes_yaml]\nassets: {job_capacity: 2, query_cache_strategy: none}\ndefault_namespaces: [pl]\n",
        )
        .expect("parse");
        let value = serde_json::to_value(&full).expect("to value");
        let mut keys: Vec<&str> = value.as_object().expect("object").keys().map(|k| k.as_str()).collect();
        keys.sort();
        let mut expected = ConfigDocument::ENVIRONMENT_KEYS.to_vec();
        expected.sort();
        assert_eq!(keys, expected, "a field of EnvironmentConfig is missing from ENVIRONMENT_KEYS");

        let doc = ConfigDocument::from_json(&full.to_json().expect("json")).expect("document");
        let back = doc.finish().expect("no sections");
        assert_eq!(serde_json::to_value(&back).expect("to value"), value);
    }

    /// configuration-document AC-6: an empty document, `{}` or `null`, is the default.
    #[test]
    fn empty_document_is_the_default() {
        let default = serde_json::to_value(EnvironmentConfig::default()).expect("to value");
        for text in ["", "{}", "null", "# only a comment\n"] {
            let doc = ConfigDocument::from_yaml(text).expect("parse");
            assert!(doc.command_registry.is_none());
            assert!(doc.remaining_sections().is_empty());
            let config = doc.finish().expect("finish");
            assert_eq!(serde_json::to_value(&config).expect("to value"), default, "{text:?}");
        }
        assert!(ConfigDocument::from_json("{}").expect("json").finish().is_ok());
        assert!(ConfigDocument::from_yaml("just text").is_err(), "a scalar is not a document");
    }

    #[test]
    fn document_refuses_tags_in_sections() {
        use crate::error::ErrorType;
        let error = ConfigDocument::from_yaml("python:\n  pickle: !Value true\n").expect_err("tag");
        assert_eq!(error.error_type, ErrorType::ParseError);
        assert!(error.to_string().contains("python.pickle"), "{error}");
    }

    #[test]
    fn document_refuses_non_string_keys() {
        use crate::error::ErrorType;
        let error = ConfigDocument::from_yaml("1: x\n").expect_err("non-string key");
        assert_eq!(error.error_type, ErrorType::ParseError);
        let error = ConfigDocument::from_yaml("command_registry: [a]\n").expect_err("not a file");
        assert_eq!(error.error_type, ErrorType::ConfigurationError);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn load_refuses_an_unknown_extension() {
        use crate::error::ErrorType;
        let error = ConfigDocument::load(std::path::Path::new("liquers.conf")).expect_err("extension");
        assert_eq!(error.error_type, ErrorType::ConfigurationError);
        assert!(error.to_string().contains(".yaml"), "{error}");
    }

    /// `load` reads by extension and sets `base` to the file's folder.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn load_reads_a_file_and_sets_base() {
        let dir = std::env::temp_dir().join(format!("liquers-config-document-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("liquers.yaml");
        std::fs::write(&path, "command_registry: commands.yaml\nassets: {job_capacity: 5}\n").expect("write");
        let doc = ConfigDocument::load(&path).expect("load");
        assert_eq!(doc.base.as_deref(), Some(dir.as_path()));
        assert_eq!(doc.command_registry_path(), Some(dir.join("commands.yaml")));
        assert_eq!(doc.environment.assets.job_capacity, Some(5));
        let json = dir.join("liquers.json");
        std::fs::write(&json, "{\"assets\": {\"job_capacity\": 6}}").expect("write");
        assert_eq!(ConfigDocument::load(&json).expect("json").environment.assets.job_capacity, Some(6));
        let missing = ConfigDocument::load(&dir.join("absent.yaml")).expect_err("missing");
        assert!(missing.to_string().contains("absent.yaml"), "{missing}");
        let toml = dir.join("liquers.toml");
        std::fs::write(&toml, "[assets]\njob_capacity = 7\n").expect("write");
        #[cfg(feature = "toml")]
        assert_eq!(ConfigDocument::load(&toml).expect("toml").environment.assets.job_capacity, Some(7));
        #[cfg(not(feature = "toml"))]
        assert!(ConfigDocument::load(&toml).expect_err("feature").to_string().contains("toml"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
