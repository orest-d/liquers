//! Building stores from configuration: the construction half.
//!
//! [`crate::store_config`] describes a store; this module turns a description into an
//! [`AsyncStore`]. The seam between them is [`StoreFactory`], which a crate or an integration
//! implements to contribute store types the rest of the system does not know about.
//!
//! # The model
//!
//! - A factory **declares** the store types it can build, as [`StoreTypeInfo`] values carrying
//!   documentation, configuration arguments and availability.
//! - A factory **resolves** a configuration entry to one of those types. The default is an exact
//!   match on `type`, but a factory may infer the type from the entry instead — see
//!   [`StoreFactory::resolve`].
//! - Factories **chain**, and the *first* one to resolve an entry builds it
//!   ([`ChainedStoreFactory`]). Order is the whole contract: a chain is assembled bottom-up, so a
//!   core store type means the same thing everywhere by default. A caller who needs to override one
//!   composes a chain with their factory first.
//!
//! # There is no built-in fallback
//!
//! [`StoreRouterBuilder`] has no store types of its own: everything it builds comes from the
//! factory it was given. Each crate offers a `default_store_factory()` as the convenience — this
//! crate's is [`default_store_factory`], and `liquers-store` chains its OpenDAL types after it.
//!
//! See `specs/design/store-factories-in-core/` and `specs/reference/STORE_CONFIG_FSD.md`.

use std::collections::BTreeMap;

use crate::error::Error;
use crate::store::{AsyncStore, AsyncStoreRouter};
use crate::store_config::{StoreConfig, StoreRouterConfig};

#[cfg(not(target_arch = "wasm32"))]
use crate::store::AsyncFileStore;
use crate::store::AsyncMemoryStore;

/// The JSON type a store configuration argument accepts.
///
/// Store configuration is a JSON/YAML document, so the vocabulary is JSON's rather than the
/// command-parameter vocabulary of [`crate::command_metadata::ArgumentType`]: that one splits
/// number into integer and float variants, carries enum cases needing a command registry, and has
/// no container variant at all.
///
/// Scalars are strongly preferred. [`StoreArgumentType::Array`] and [`StoreArgumentType::Object`]
/// exist for the arguments that genuinely need them, not as an invitation — a `config:` block
/// stays easiest to read, and easiest to pass to a backend, when its values are scalars.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StoreArgumentType {
    String,
    /// JSON has a single numeric type. An argument that must be a whole number says so in its
    /// documentation.
    Number,
    Boolean,
    Array,
    Object,
    /// Unconstrained: any JSON value.
    #[default]
    Any,
}

impl StoreArgumentType {
    /// The type implied by a value, used when an argument is described by its default.
    ///
    /// A `null` default carries no type information — it is what an absent optional argument
    /// serializes to — so it yields [`StoreArgumentType::Any`] rather than a guess.
    pub fn of_value(value: &serde_json::Value) -> Self {
        match value {
            serde_json::Value::String(_) => StoreArgumentType::String,
            serde_json::Value::Number(_) => StoreArgumentType::Number,
            serde_json::Value::Bool(_) => StoreArgumentType::Boolean,
            serde_json::Value::Array(_) => StoreArgumentType::Array,
            serde_json::Value::Object(_) => StoreArgumentType::Object,
            serde_json::Value::Null => StoreArgumentType::Any,
        }
    }
}

/// One configuration key of one store type.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct StoreArgumentInfo {
    /// The key as it appears under `config:`, e.g. `root`, `bucket`.
    pub name: String,
    /// Human-readable label, for a form or a generated table.
    pub label: String,
    /// What the argument means and what a valid value looks like.
    pub doc: String,
    #[serde(default)]
    pub argument_type: StoreArgumentType,
    /// The store cannot be built without it.
    #[serde(default)]
    pub required: bool,
    /// Value used when the key is absent. `None` for a required argument.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
}

impl StoreArgumentInfo {
    pub fn new(name: &str, argument_type: StoreArgumentType) -> Self {
        Self {
            name: name.to_string(),
            label: name.to_string(),
            doc: String::new(),
            argument_type,
            required: false,
            default: None,
        }
    }

    /// An argument described by a backend's own default value rather than by hand.
    ///
    /// The type is inferred from the default; see [`StoreArgumentType::of_value`] for why a `null`
    /// default yields [`StoreArgumentType::Any`].
    pub fn derived(name: &str, default: serde_json::Value) -> Self {
        let argument_type = StoreArgumentType::of_value(&default);
        Self {
            name: name.to_string(),
            label: name.to_string(),
            doc: String::new(),
            argument_type,
            required: false,
            default: (!default.is_null()).then_some(default),
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.label = label.to_string();
        self
    }

    pub fn with_doc(mut self, doc: &str) -> Self {
        self.doc = doc.to_string();
        self
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn with_default(mut self, default: serde_json::Value) -> Self {
        self.default = Some(default);
        self
    }
}

/// Whether a store type can be built in *this* build.
///
/// A type that is real and documented but compiled out — because a Cargo feature is off, or
/// because the target does not support it — must say so. Reporting it as an unknown type instead
/// sends the reader hunting for a typo in something that exists.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub enum StoreTypeAvailability {
    /// Constructible here. Creation may still fail on a bad configuration.
    #[default]
    Available,
    /// Known but not constructible here. The string names the feature or target responsible.
    Unavailable(String),
}

/// Whether a store type's argument list is exhaustive.
///
/// A store type Liquers owns can be described completely, and its argument list *is* the
/// specification. A store type defined by another project cannot: its arguments change on that
/// project's release schedule, so a hand-written copy becomes silently wrong rather than merely
/// incomplete.
///
/// [`ArgumentCoverage::Partial`] makes incompleteness a stated fact instead of an omission, which
/// is the only property that survives contact with a dependency's release cadence.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub enum ArgumentCoverage {
    /// The argument list is the specification; an unlisted key may be refused.
    #[default]
    Complete,
    /// The argument list is guidance. Unlisted keys are passed to the backend, and the string
    /// says where the authoritative documentation lives.
    Partial { authority: String },
}

/// One store type a factory declares.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct StoreTypeInfo {
    /// The `type:` value in a store configuration entry, e.g. `memory`, `s3`, `localstorage`.
    pub store_type: String,
    pub label: String,
    pub doc: String,
    /// Configuration keys this type accepts, in a stable order.
    #[serde(default)]
    pub arguments: Vec<StoreArgumentInfo>,
    #[serde(default)]
    pub availability: StoreTypeAvailability,
    #[serde(default)]
    pub coverage: ArgumentCoverage,
}

impl StoreTypeInfo {
    pub fn new(store_type: &str) -> Self {
        Self {
            store_type: store_type.to_string(),
            label: store_type.to_string(),
            doc: String::new(),
            arguments: Vec::new(),
            availability: StoreTypeAvailability::Available,
            coverage: ArgumentCoverage::Complete,
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.label = label.to_string();
        self
    }

    pub fn with_doc(mut self, doc: &str) -> Self {
        self.doc = doc.to_string();
        self
    }

    pub fn with_argument(mut self, argument: StoreArgumentInfo) -> Self {
        self.arguments.push(argument);
        self
    }

    pub fn with_arguments(mut self, arguments: Vec<StoreArgumentInfo>) -> Self {
        self.arguments = arguments;
        self
    }

    /// Mark the type known but not constructible here, naming the feature or target responsible.
    pub fn unavailable(mut self, reason: &str) -> Self {
        self.availability = StoreTypeAvailability::Unavailable(reason.to_string());
        self
    }

    /// Mark the argument list as guidance about a surface Liquers does not own.
    pub fn partial(mut self, authority: &str) -> Self {
        self.coverage = ArgumentCoverage::Partial {
            authority: authority.to_string(),
        };
        self
    }

    /// The error this type's [`StoreTypeAvailability`] implies, if it cannot be built here.
    pub fn unavailability_error(&self) -> Option<Error> {
        match &self.availability {
            StoreTypeAvailability::Available => None,
            StoreTypeAvailability::Unavailable(reason) => Some(Error::not_supported(format!(
                "Store type '{}' is not available in this build: {}",
                self.store_type, reason
            ))),
        }
    }

    fn is_available(&self) -> bool {
        match self.availability {
            StoreTypeAvailability::Available => true,
            StoreTypeAvailability::Unavailable(_) => false,
        }
    }
}

/// Creates stores of types the rest of the system does not know about.
///
/// Deliberately carries no `Send`/`Sync` bound. A factory is transient — it is consumed while the
/// router is built — and only the [`AsyncStore`] it produces has thread requirements, which
/// `AsyncStore` already states. A bound no call site needs would exclude a browser factory, which
/// holds JavaScript handles and is `!Send`.
pub trait StoreFactory {
    /// Store types this factory declares, with their arguments and availability.
    fn store_types(&self) -> Vec<StoreTypeInfo>;

    /// Which of this factory's store types, if any, the entry describes.
    ///
    /// The default is an exact match on `config.store_type`. A factory **may override this to
    /// infer** the type from the entry — from a URI, or anything else it recognises. A store type
    /// is the *resolved identity* of an entry; what identifies it is input.
    ///
    /// Two rules keep inference from becoming magic in a routing decision:
    ///
    /// - resolve only to a store type this factory declares in [`Self::store_types`]; and
    /// - key on something whose purpose is identification, not on the incidental presence of an
    ///   argument — otherwise adding that argument elsewhere silently reroutes a document.
    fn resolve(&self, config: &StoreConfig) -> Option<String> {
        let requested = config.store_type.as_str();
        (!requested.is_empty()
            && self
                .store_types()
                .iter()
                .any(|t| t.store_type == requested))
        .then(|| requested.to_string())
    }

    /// Build a store from an entry this factory resolved.
    ///
    /// `config.store_type` is always the name [`Self::resolve`] returned: [`ChainedStoreFactory`]
    /// fills it in before calling, so an implementation never has to re-derive it.
    fn create(&self, config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error>;
}

/// Builds one store from its configuration entry.
///
/// No `Send`/`Sync` bound, for the reason given on [`StoreFactory`].
pub type StoreConstructor = Box<dyn Fn(&StoreConfig) -> Result<Box<dyn AsyncStore>, Error>>;

/// A [`StoreFactory`] assembled from named creation functions rather than by implementing the
/// trait.
///
/// Ordered by store type name: [`StoreFactory::store_types`] feeds error messages, and a hash
/// order that varies between runs would make those messages — and any test asserting on them —
/// non-deterministic.
#[derive(Default)]
pub struct StoreTypeMap {
    entries: BTreeMap<String, (StoreTypeInfo, StoreConstructor)>,
}

impl StoreTypeMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_store_type(mut self, info: StoreTypeInfo, create: StoreConstructor) -> Self {
        self.entries.insert(info.store_type.clone(), (info, create));
        self
    }
}

impl StoreFactory for StoreTypeMap {
    fn store_types(&self) -> Vec<StoreTypeInfo> {
        self.entries.values().map(|(info, _)| info.clone()).collect()
    }

    /// Overridden as a map lookup so chain dispatch does not rebuild every description per entry.
    fn resolve(&self, config: &StoreConfig) -> Option<String> {
        let requested = config.store_type.as_str();
        (!requested.is_empty() && self.entries.contains_key(requested))
            .then(|| requested.to_string())
    }

    fn create(&self, config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error> {
        match self.entries.get(config.store_type.as_str()) {
            Some((info, create)) => match info.unavailability_error() {
                Some(error) => Err(error),
                None => create(config),
            },
            None => Err(unknown_store_type_error(
                &config.store_type,
                &self.store_types(),
            )),
        }
    }
}

/// Several factories consulted in order; the **first** to resolve an entry builds it.
///
/// A chain is assembled bottom-up — `liquers-core`, then `liquers-store`, then a library, then the
/// integration — so a core store type means the same thing everywhere by default. Overriding is
/// available to anyone who needs it: compose a chain with your factory first.
#[derive(Default)]
pub struct ChainedStoreFactory {
    factories: Vec<Box<dyn StoreFactory>>,
}

impl ChainedStoreFactory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Consult `factory` after everything already in the chain.
    pub fn chain(mut self, factory: Box<dyn StoreFactory>) -> Self {
        self.factories.push(factory);
        self
    }

    /// The factory that resolves this entry, with the name it resolved to.
    fn resolve_with(&self, config: &StoreConfig) -> Option<(&dyn StoreFactory, String)> {
        self.factories
            .iter()
            .find_map(|f| f.resolve(config).map(|name| (f.as_ref(), name)))
    }
}

impl StoreFactory for ChainedStoreFactory {
    /// The union, with earlier members winning, so the list a caller sees matches the dispatch
    /// they will get.
    fn store_types(&self) -> Vec<StoreTypeInfo> {
        let mut seen: BTreeMap<String, StoreTypeInfo> = BTreeMap::new();
        for factory in &self.factories {
            for info in factory.store_types() {
                seen.entry(info.store_type.clone()).or_insert(info);
            }
        }
        seen.into_values().collect()
    }

    fn resolve(&self, config: &StoreConfig) -> Option<String> {
        self.resolve_with(config).map(|(_, name)| name)
    }

    fn create(&self, config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error> {
        match self.resolve_with(config) {
            Some((factory, resolved)) => {
                if config.store_type == resolved {
                    factory.create(config)
                } else {
                    // Hand the member a fully resolved entry, so `create` can always trust
                    // `store_type`.
                    let mut resolved_config = config.clone();
                    resolved_config.store_type = resolved;
                    factory.create(&resolved_config)
                }
            }
            None => Err(unknown_store_type_error(
                &config.store_type,
                &self.store_types(),
            )),
        }
    }
}

/// The store types `liquers-core` implements: `memory`, and `filesystem` off wasm32.
pub fn core_store_factory() -> StoreTypeMap {
    let memory = StoreTypeInfo::new("memory")
        .with_label("In-memory store")
        .with_doc("Volatile store held in process memory. Contents are lost when the process ends.");

    #[cfg(not(target_arch = "wasm32"))]
    let filesystem = StoreTypeInfo::new("filesystem")
        .with_label("Local filesystem")
        .with_doc("Stores data and metadata as files under a local directory.")
        .with_argument(
            StoreArgumentInfo::new("path", StoreArgumentType::String)
                .required()
                .with_doc("Directory treated as the store root. Created if it does not exist."),
        );

    // `AsyncFileStore` uses `tokio::fs`, which wasm32-unknown-unknown does not provide. The type is
    // still declared, so a document naming it is told *why* rather than that it is unknown.
    #[cfg(target_arch = "wasm32")]
    let filesystem = StoreTypeInfo::new("filesystem")
        .with_label("Local filesystem")
        .with_doc("Stores data and metadata as files under a local directory.")
        .with_argument(
            StoreArgumentInfo::new("path", StoreArgumentType::String)
                .required()
                .with_doc("Directory treated as the store root."),
        )
        .unavailable("not available on wasm32: it needs tokio::fs");

    StoreTypeMap::new()
        .with_store_type(
            memory,
            Box::new(|config: &StoreConfig| {
                let prefix = config.key_prefix()?;
                Ok(Box::new(AsyncMemoryStore::new(&prefix)) as Box<dyn AsyncStore>)
            }),
        )
        .with_store_type(
            filesystem,
            Box::new(|config: &StoreConfig| {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let prefix = config.key_prefix()?;
                    let path = config.require_config_string_expanded("path")?;
                    Ok(Box::new(AsyncFileStore::new(&path, &prefix)) as Box<dyn AsyncStore>)
                }
                #[cfg(target_arch = "wasm32")]
                {
                    let _ = config;
                    Err(Error::not_supported(
                        "Store type 'filesystem' is not available on wasm32: it needs tokio::fs"
                            .to_string(),
                    ))
                }
            }),
        )
}

/// This crate's convenience chain. Nothing sits below `liquers-core`, so it is
/// [`core_store_factory`] in a chain — named for the convention, so a consumer writes the same call
/// whichever crate they take it from.
pub fn default_store_factory() -> ChainedStoreFactory {
    ChainedStoreFactory::new().chain(Box::new(core_store_factory()))
}

/// The error for an entry no factory resolved.
///
/// Lists the store types the chain actually supports, so the message is accurate for the build in
/// hand rather than describing a type set that may not be compiled in, and reports a known but
/// unavailable type separately from an unknown one.
pub fn unknown_store_type_error(store_type: &str, known: &[StoreTypeInfo]) -> Error {
    let available: Vec<&str> = known
        .iter()
        .filter(|t| t.is_available())
        .map(|t| t.store_type.as_str())
        .collect();
    let unavailable: Vec<String> = known
        .iter()
        .filter(|t| !t.is_available())
        .map(|t| match &t.availability {
            StoreTypeAvailability::Available => t.store_type.clone(),
            StoreTypeAvailability::Unavailable(reason) => {
                format!("{} ({})", t.store_type, reason)
            }
        })
        .collect();

    let described = if store_type.is_empty() {
        "A store entry has no type".to_string()
    } else {
        format!("Unknown store type '{}'", store_type)
    };
    let mut message = format!(
        "{}. Supported store types: {}.",
        described,
        if available.is_empty() {
            "none".to_string()
        } else {
            available.join(", ")
        }
    );
    if !unavailable.is_empty() {
        message.push_str(&format!(
            " Known but unavailable in this build: {}.",
            unavailable.join(", ")
        ));
    }
    Error::not_supported(message)
}

/// Builds an [`AsyncStoreRouter`] from a configuration document and a factory.
///
/// The factory is required rather than optional: this builder has no store types of its own, so
/// "which stores do I get" is answerable at the call site.
pub struct StoreRouterBuilder {
    config: StoreRouterConfig,
    factory: Box<dyn StoreFactory>,
}

impl StoreRouterBuilder {
    pub fn new(config: StoreRouterConfig, factory: Box<dyn StoreFactory>) -> Self {
        Self { config, factory }
    }

    pub fn from_yaml(yaml: &str, factory: Box<dyn StoreFactory>) -> Result<Self, Error> {
        Ok(Self::new(StoreRouterConfig::from_yaml(yaml)?, factory))
    }

    pub fn from_json(json: &str, factory: Box<dyn StoreFactory>) -> Result<Self, Error> {
        Ok(Self::new(StoreRouterConfig::from_json(json)?, factory))
    }

    /// Replace the builder's factory outright.
    pub fn with_factory(mut self, factory: Box<dyn StoreFactory>) -> Self {
        self.factory = factory;
        self
    }

    /// Consult `factory` after the current one.
    pub fn chain_factory(self, factory: Box<dyn StoreFactory>) -> Self {
        let chained = ChainedStoreFactory::new()
            .chain(self.factory)
            .chain(factory);
        Self {
            config: self.config,
            factory: Box::new(chained),
        }
    }

    /// Expand `${VAR}` references, then build.
    pub fn build(mut self) -> Result<AsyncStoreRouter, Error> {
        self.config.expand_env_vars()?;
        self.build_without_env_expansion()
    }

    /// Build without expanding `${VAR}` references.
    ///
    /// For an environment that has none — a browser page — or where expansion was already done.
    pub fn build_without_env_expansion(self) -> Result<AsyncStoreRouter, Error> {
        let mut router = AsyncStoreRouter::new();
        for store_config in &self.config.stores {
            router.add_store(self.factory.create(store_config)?);
        }
        Ok(router)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_key;

    /// A factory over a fixed type list whose `create` records nothing but succeeds, so tests can
    /// assert on resolution rather than on store behaviour.
    struct NamedFactory {
        types: Vec<StoreTypeInfo>,
    }

    impl NamedFactory {
        fn new(names: &[&str]) -> Self {
            Self {
                types: names.iter().map(|n| StoreTypeInfo::new(n)).collect(),
            }
        }

        fn with(types: Vec<StoreTypeInfo>) -> Self {
            Self { types }
        }
    }

    impl StoreFactory for NamedFactory {
        fn store_types(&self) -> Vec<StoreTypeInfo> {
            self.types.clone()
        }

        fn create(&self, config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error> {
            let prefix = config.key_prefix()?;
            Ok(Box::new(AsyncMemoryStore::new(&prefix)))
        }
    }

    /// Resolves any entry carrying a `marker` key to the single type it declares, whatever the
    /// entry's `type` says. Stands in for a factory that infers a type — from a URI, say.
    struct InferringFactory;

    impl StoreFactory for InferringFactory {
        fn store_types(&self) -> Vec<StoreTypeInfo> {
            vec![StoreTypeInfo::new("inferred")]
        }

        fn resolve(&self, config: &StoreConfig) -> Option<String> {
            config.config.contains_key("marker").then(|| "inferred".to_string())
        }

        fn create(&self, config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error> {
            let prefix = config.key_prefix()?;
            Ok(Box::new(AsyncMemoryStore::new(&prefix)))
        }
    }

    /// Records the `store_type` it was handed, so a test can assert the chain resolved it first.
    /// The log is shared with the test, the way `CountingFactory` shared its counter.
    struct RecordingFactory {
        seen: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    }

    impl StoreFactory for RecordingFactory {
        fn store_types(&self) -> Vec<StoreTypeInfo> {
            vec![StoreTypeInfo::new("recorded")]
        }

        fn resolve(&self, config: &StoreConfig) -> Option<String> {
            config
                .config
                .contains_key("marker")
                .then(|| "recorded".to_string())
        }

        fn create(&self, config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error> {
            self.seen.borrow_mut().push(config.store_type.clone());
            let prefix = config.key_prefix()?;
            Ok(Box::new(AsyncMemoryStore::new(&prefix)))
        }
    }

    fn entry(store_type: &str, prefix: &str) -> StoreConfig {
        StoreConfig::new(store_type).with_prefix(prefix)
    }

    // --- StoreTypeMap ---

    #[test]
    fn map01_resolves_only_registered_types() {
        let map = core_store_factory();
        assert_eq!(
            map.resolve(&entry("memory", "a")),
            Some("memory".to_string())
        );
        assert_eq!(map.resolve(&entry("nonesuch", "a")), None);
    }

    #[test]
    fn map02_create_dispatches_to_the_registered_constructor(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let store = core_store_factory().create(&entry("memory", "cache"))?;
        assert_eq!(store.key_prefix(), parse_key("cache")?);
        Ok(())
    }

    /// `store_types()` feeds error text, so its order must not vary between runs.
    #[test]
    fn map03_store_types_is_sorted() {
        let map = StoreTypeMap::new()
            .with_store_type(
                StoreTypeInfo::new("zebra"),
                Box::new(|_| Err(Error::general_error("unused".to_string()))),
            )
            .with_store_type(
                StoreTypeInfo::new("alpha"),
                Box::new(|_| Err(Error::general_error("unused".to_string()))),
            );
        let names: Vec<String> = map.store_types().into_iter().map(|t| t.store_type).collect();
        assert_eq!(names, vec!["alpha".to_string(), "zebra".to_string()]);
    }

    #[test]
    fn map04_unregistered_type_errors() {
        match core_store_factory().create(&entry("nonesuch", "a")) {
            Ok(_) => panic!("an unregistered type must not build"),
            Err(e) => assert_eq!(e.error_type, crate::error::ErrorType::NotSupported),
        }
    }

    // --- ChainedStoreFactory ---

    #[test]
    fn chain01_empty_chain_resolves_nothing() {
        let chain = ChainedStoreFactory::new();
        assert_eq!(chain.resolve(&entry("memory", "a")), None);
        assert!(chain.store_types().is_empty());
    }

    #[test]
    fn chain02_single_factory_behaves_as_itself() -> Result<(), Box<dyn std::error::Error>> {
        let chain = ChainedStoreFactory::new().chain(Box::new(core_store_factory()));
        assert_eq!(
            chain.resolve(&entry("memory", "a")),
            Some("memory".to_string())
        );
        let store = chain.create(&entry("memory", "cache"))?;
        assert_eq!(store.key_prefix(), parse_key("cache")?);
        Ok(())
    }

    /// Replaces `factory02_factory_precedes_builtin`.
    ///
    /// That test asserted a factory beat the built-in types because factories were consulted
    /// first. There are no built-ins now: order in the chain is the whole rule, and a factory
    /// chained *earlier* wins. This is what lets a caller override a store type someone else
    /// defines — by composing a chain with their own factory first.
    #[test]
    fn chain03_earlier_factory_wins() -> Result<(), Box<dyn std::error::Error>> {
        let first = NamedFactory::with(vec![StoreTypeInfo::new("memory").with_doc("first")]);
        let second = NamedFactory::with(vec![StoreTypeInfo::new("memory").with_doc("second")]);
        let chain = ChainedStoreFactory::new()
            .chain(Box::new(first))
            .chain(Box::new(second));

        let types = chain.store_types();
        assert_eq!(types.len(), 1, "the union must not duplicate a type");
        assert_eq!(
            types[0].doc, "first",
            "the surviving description must belong to the factory that will actually run"
        );
        Ok(())
    }

    /// The union must be first-wins too, not merely deduplicated.
    ///
    /// Otherwise `store_types()` can advertise a description belonging to a factory that will
    /// never be called — and since that list is what the unclaimed-type error prints, the message
    /// would lie.
    #[test]
    fn chain04_store_types_is_the_union_first_wins() {
        let chain = ChainedStoreFactory::new()
            .chain(Box::new(NamedFactory::with(vec![
                StoreTypeInfo::new("shared").with_doc("earlier"),
                StoreTypeInfo::new("only_first"),
            ])))
            .chain(Box::new(NamedFactory::with(vec![
                StoreTypeInfo::new("shared").with_doc("later"),
                StoreTypeInfo::new("only_second"),
            ])));

        let types = chain.store_types();
        let names: Vec<&str> = types.iter().map(|t| t.store_type.as_str()).collect();
        assert_eq!(names, vec!["only_first", "only_second", "shared"]);
        let shared = types.iter().find(|t| t.store_type == "shared").unwrap();
        assert_eq!(shared.doc, "earlier");
    }

    /// Asserts on message *content*, not just `is_err()`.
    ///
    /// The test this replaces checked only `is_err()`, which would pass against an empty message.
    /// The whole point of the new error is that it tells the reader what this build does support.
    #[test]
    fn chain05_unclaimed_type_lists_supported_types() {
        let chain = ChainedStoreFactory::new().chain(Box::new(core_store_factory()));
        let error = match chain.create(&entry("postgress", "a")) {
            Ok(_) => panic!("an unclaimed type must not build"),
            Err(e) => e,
        };

        assert_eq!(error.error_type, crate::error::ErrorType::NotSupported);
        assert!(
            error.message.contains("postgress"),
            "the message must name the unclaimed type, got: {}",
            error.message
        );
        assert!(
            error.message.contains("memory"),
            "the message must list what is supported, got: {}",
            error.message
        );
    }

    /// A type that is real but not constructible here is refused with the reason, never as
    /// "unknown". This is conformance item STORE13.
    #[test]
    fn chain06_unavailable_type_reports_its_reason() {
        let factory = StoreTypeMap::new().with_store_type(
            StoreTypeInfo::new("gated").unavailable("requires the 'example' feature"),
            Box::new(|_| Err(Error::general_error("must not be called".to_string()))),
        );
        let chain = ChainedStoreFactory::new().chain(Box::new(factory));

        let error = match chain.create(&entry("gated", "a")) {
            Ok(_) => panic!("an unavailable type must not build"),
            Err(e) => e,
        };
        assert!(
            error.message.contains("example"),
            "the message must name the feature responsible, got: {}",
            error.message
        );
        assert!(
            !error.message.contains("Unknown store type"),
            "a gated-off type is not an unknown type, got: {}",
            error.message
        );
    }

    // --- core factory ---

    #[test]
    fn core01_resolves_memory_and_filesystem() {
        let names: Vec<String> = core_store_factory()
            .store_types()
            .into_iter()
            .map(|t| t.store_type)
            .collect();
        assert_eq!(names, vec!["filesystem".to_string(), "memory".to_string()]);
    }

    #[test]
    fn core02_memory_store_is_constructed() -> Result<(), Box<dyn std::error::Error>> {
        let store = default_store_factory().create(&entry("memory", "cache"))?;
        assert_eq!(store.key_prefix(), parse_key("cache")?);
        Ok(())
    }

    /// On wasm32 `filesystem` is listed and explained rather than absent, so a document naming it
    /// is told why rather than that the type is unknown.
    #[cfg(target_arch = "wasm32")]
    #[test]
    fn core03_filesystem_is_listed_but_unavailable_on_wasm() {
        let types = core_store_factory().store_types();
        let fs = types
            .iter()
            .find(|t| t.store_type == "filesystem")
            .expect("filesystem must still be listed");
        match &fs.availability {
            StoreTypeAvailability::Unavailable(reason) => assert!(reason.contains("wasm32")),
            StoreTypeAvailability::Available => panic!("filesystem cannot be available on wasm32"),
        }
    }

    /// A missing required argument fails at construction, which is where configuration is
    /// validated: there is no separate validation pass.
    #[test]
    fn core04_missing_required_argument_fails_at_construction() {
        let error = match core_store_factory().create(&entry("filesystem", "local")) {
            Ok(_) => panic!("filesystem must not build without a path"),
            Err(e) => e,
        };
        assert!(
            error.message.contains("path"),
            "the message must name the missing argument, got: {}",
            error.message
        );
    }

    // --- resolution ---

    #[test]
    fn resolve01_default_is_an_exact_type_match() {
        let factory = NamedFactory::new(&["alpha"]);
        assert_eq!(
            factory.resolve(&entry("alpha", "p")),
            Some("alpha".to_string())
        );
        assert_eq!(factory.resolve(&entry("beta", "p")), None);
    }

    /// An entry with no type resolves nowhere under the default, so making `store_type`
    /// defaultable changes no behaviour until something actually infers.
    #[test]
    fn resolve02_empty_store_type_resolves_nowhere() {
        let factory = NamedFactory::new(&["alpha"]);
        assert_eq!(factory.resolve(&entry("", "p")), None);
        assert_eq!(core_store_factory().resolve(&entry("", "p")), None);
    }

    /// The behavioural half of "the store type may be inferred": a factory that resolves an entry
    /// the default would reject wins where it resolves, and declines otherwise.
    #[test]
    fn resolve03_an_inferring_factory_wins_where_it_resolves() {
        let chain = ChainedStoreFactory::new()
            .chain(Box::new(InferringFactory))
            .chain(Box::new(core_store_factory()));

        let inferred = entry("", "p").with_config("marker", true);
        assert_eq!(chain.resolve(&inferred), Some("inferred".to_string()));
        // Without the marker the inferring factory declines and the chain falls through.
        assert_eq!(
            chain.resolve(&entry("memory", "p")),
            Some("memory".to_string())
        );
    }

    /// The invariant the trait promises: `create` receives the name `resolve` returned.
    ///
    /// Without it an inferring factory would have to re-derive its own answer inside `create`, and
    /// a factory written against the default would silently receive an empty type.
    #[test]
    fn resolve04_create_receives_the_resolved_store_type(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let chain = ChainedStoreFactory::new().chain(Box::new(RecordingFactory {
            seen: std::rc::Rc::clone(&seen),
        }));

        // The entry carries no type at all; only the factory's `resolve` knows what it is.
        chain.create(&entry("", "p").with_config("marker", true))?;

        assert_eq!(
            seen.borrow().as_slice(),
            &["recorded".to_string()],
            "the chain must fill in the resolved name before calling create"
        );
        Ok(())
    }
}
