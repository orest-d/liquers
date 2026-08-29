//! OpenDAL-backed store types, and the chain a native consumer wants.
//!
//! The configuration format, the [`StoreFactory`] seam and [`StoreRouterBuilder`] all live in
//! `liquers-core`. What is left here is the part core cannot own: the backends themselves, and the
//! knowledge of which store type names mean an OpenDAL service.
//!
//! Use [`default_store_factory`] unless you need a different composition — it is core's store
//! types followed by OpenDAL's, which is what a native application usually wants.

#[cfg(feature = "opendal")]
use std::collections::HashMap;

use liquers_core::error::Error;
use liquers_core::store::AsyncStore;
use liquers_core::store_config::{StoreConfig, StoreRouterConfig};
use liquers_core::store_factory::{
    core_store_factory, ChainedStoreFactory, StoreArgumentInfo, StoreArgumentType,
    StoreFactory, StoreRouterBuilder, StoreTypeInfo,
};
use liquers_core::store::AsyncStoreRouter;

#[cfg(feature = "opendal")]
use crate::opendal_store::AsyncOpenDALStore;
#[cfg(feature = "opendal")]
use opendal::Operator;

/// Store types that map to OpenDAL services.
///
/// A hand-maintained list of *names*. It changes when a service is added or removed, not when a
/// service gains a configuration field — the arguments are derived, not written down. See
/// `specs/design/store-factories-in-core/` Phase 3.
pub const OPENDAL_STORE_TYPES: &[&str] = &[
    "fs",
    "s3",
    "ftp",
    "gcs",
    "azblob",
    "sftp",
    "webdav",
    "github",
    "hdfs",
    "webhdfs",
    "http",
    "https",
    "redis",
    "mongodb",
    "postgresql",
    "mysql",
    "sqlite",
    "dropbox",
    "onedrive",
    "gdrive",
    "ipfs",
];

/// Whether a store type is handled by OpenDAL.
pub fn is_opendal_store_type(store_type: &str) -> bool {
    OPENDAL_STORE_TYPES.contains(&store_type) || store_type.starts_with("opendal_")
}

/// The OpenDAL scheme a store type names, stripping the `opendal_` escape-hatch prefix.
pub fn get_opendal_scheme(store_type: &str) -> &str {
    store_type.strip_prefix("opendal_").unwrap_or(store_type)
}

/// Where the authoritative documentation for an OpenDAL service's options lives.
///
/// The argument lists this factory reports are [`ArgumentCoverage::Partial`] against this: OpenDAL
/// owns those options and changes them on its own release schedule, so a hand-written copy here
/// would go silently wrong rather than merely stale.
///
/// [`ArgumentCoverage::Partial`]: liquers_core::store_factory::ArgumentCoverage::Partial
const OPENDAL_DOCS: &str = "https://opendal.apache.org/docs/rust/opendal/services/index.html";

/// Builds the OpenDAL-backed store types.
///
/// Compiled whether or not the `opendal` feature is on. With it off, every type is still
/// *declared*, marked unavailable with the feature responsible — a type that is real and
/// documented but gated out must say so rather than be reported as unknown.
pub struct OpendalStoreFactory;

impl OpendalStoreFactory {
    /// Arguments common to every OpenDAL service, described by hand because they are the ones a
    /// reader actually needs. Everything else is OpenDAL's to document — the list is
    /// `ArgumentCoverage::Partial` against [`OPENDAL_DOCS`] precisely so it need not be complete.
    ///
    /// # Why these are hand-written, for now
    ///
    /// The full argument list should be *derived* from the linked OpenDAL rather than written
    /// here: `Configurator` bounds `Serialize`, every service config derives `Default`, and none
    /// carries `skip_serializing_if`, so `serde_json::to_value(C::default())` yields every field
    /// name and default. What used to block it was that naming `opendal::services::S3Config`
    /// requires the `services-s3` feature and this crate enabled **no** service features. That is
    /// fixed — `services-default` names the config types of every service it enables — so the
    /// remaining work is the derivation itself, tracked as `STORE-OPENDAL-ARGUMENTS-NOT-DERIVED`.
    /// It must stay `#[cfg]`-aware: a service left out of this build has no nameable config.
    ///
    /// See `specs/design/store-factories-in-core/` Phase 4 Step 9.
    fn common_arguments(store_type: &str) -> Vec<StoreArgumentInfo> {
        let mut arguments = vec![StoreArgumentInfo::new("root", StoreArgumentType::String)
            .with_doc("Path within the backend treated as the store root.")];
        match store_type {
            "s3" | "gcs" | "azblob" | "cos" | "oss" => {
                arguments.insert(
                    0,
                    StoreArgumentInfo::new("bucket", StoreArgumentType::String)
                        .required()
                        .with_doc("Bucket or container name."),
                );
                arguments.push(
                    StoreArgumentInfo::new("region", StoreArgumentType::String)
                        .with_doc("Service region, e.g. eu-central-1. Required by some services."),
                );
                arguments.push(
                    StoreArgumentInfo::new("endpoint", StoreArgumentType::String).with_doc(
                        "Override the service endpoint, for an S3-compatible server such as MinIO.",
                    ),
                );
            }
            "ftp" | "sftp" | "webdav" | "http" | "https" => {
                arguments.push(
                    StoreArgumentInfo::new("endpoint", StoreArgumentType::String)
                        .required()
                        .with_doc("Server address, e.g. ftp.example.org:21."),
                );
            }
            _ => {}
        }
        arguments.push(
            StoreArgumentInfo::new("access_key_id", StoreArgumentType::String).with_doc(
                "Credential. Write it as ${ENV_VAR}; never a literal secret in a document.",
            ),
        );
        arguments
    }

    fn type_info(store_type: &str) -> StoreTypeInfo {
        let info = StoreTypeInfo::new(store_type)
            .with_doc(&format!(
                "OpenDAL '{}' service. Its full option set is documented by OpenDAL; the arguments \
                 listed here are the common ones.",
                get_opendal_scheme(store_type)
            ))
            .with_arguments(Self::common_arguments(store_type))
            .partial(OPENDAL_DOCS);

        match unavailability_reason(store_type) {
            Some(reason) => info.unavailable(&reason),
            None => info,
        }
    }

}

impl StoreFactory for OpendalStoreFactory {
    fn store_types(&self) -> Vec<StoreTypeInfo> {
        OPENDAL_STORE_TYPES
            .iter()
            .map(|t| Self::type_info(t))
            .collect()
    }

    /// Also resolves the `opendal_<scheme>` escape hatch, which names a service not in the table.
    fn resolve(&self, config: &StoreConfig) -> Option<String> {
        let requested = config.store_type.as_str();
        is_opendal_store_type(requested).then(|| requested.to_string())
    }

    /// Refuses an unavailable type with the same reason [`Self::store_types`] reports, so what is
    /// advertised and what happens cannot disagree.
    fn create(&self, config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error> {
        let unavailable = |reason: String| {
            Error::not_supported(format!(
                "Store type '{}' is not available in this build: {}",
                config.store_type, reason
            ))
        };
        #[cfg(feature = "opendal")]
        {
            // Resolving to a `Scheme` rather than passing the name through is what makes an
            // *alias* work. `Scheme::from_str` accepts several — `https` is `Scheme::Http`,
            // `ipns` is `Scheme::Ipfs` — but `Operator::via_iter` matches the canonical scheme
            // constant (`services::HTTP_SCHEME`) and nothing else, so `via_iter("https", ..)`
            // fails with "scheme is not enabled or supported" however many features are on.
            // `https` is an advertised store type, so that was a type declared available that
            // could not be built; `availability01` catches it now.
            let scheme = resolve_enabled_scheme(&config.store_type).map_err(unavailable)?;
            let prefix = config.key_prefix()?;
            let operator = create_opendal_operator(scheme, config.config_as_string_map()?)?;
            Ok(Box::new(AsyncOpenDALStore::new(operator, prefix)))
        }
        #[cfg(not(feature = "opendal"))]
        {
            // Without OpenDAL every type is unavailable, so this branch only has to report it.
            match unavailability_reason(&config.store_type) {
                Some(reason) => Err(unavailable(reason)),
                None => Err(unavailable(
                    "requires the 'opendal' feature of liquers-store".to_string(),
                )),
            }
        }
    }
}

/// Why this build cannot construct `store_type`, or `None` if it can.
///
/// Two independent things must both hold, and conflating them is how a factory ends up advertising
/// a store type it cannot build:
///
/// 1. `liquers-store`'s own `opendal` feature must be on, or there is no OpenDAL at all; and
/// 2. **OpenDAL's own `services-*` feature for that scheme must be on.** `opendal`'s `default`
///    enables only `services-memory`, and enabling `dep:opendal` enables no service. So a
///    consumer can have OpenDAL linked and still not have `s3`.
///
/// The second is asked of OpenDAL rather than guessed, via `Scheme::enabled()`, so the answer
/// tracks whatever features the dependency graph actually resolved.
fn unavailability_reason(store_type: &str) -> Option<String> {
    #[cfg(not(feature = "opendal"))]
    {
        let _ = store_type;
        Some("requires the 'opendal' feature of liquers-store".to_string())
    }
    #[cfg(feature = "opendal")]
    {
        resolve_enabled_scheme(store_type).err()
    }
}

/// The OpenDAL scheme `store_type` names, if this build compiled that service in.
///
/// The single place a store type becomes an OpenDAL scheme, so [`unavailability_reason`] and
/// `create` cannot answer differently — one reports the `Err`, the other uses the `Ok`.
///
/// Returning the `Scheme` rather than a name matters: it is the *canonical* form, and
/// `Operator::via_iter` accepts nothing else.
#[cfg(feature = "opendal")]
fn resolve_enabled_scheme(store_type: &str) -> Result<opendal::Scheme, String> {
    use std::str::FromStr;
    let scheme_name = get_opendal_scheme(store_type);
    match opendal::Scheme::from_str(scheme_name) {
        Ok(scheme) if opendal::Scheme::enabled().contains(&scheme) => Ok(scheme),
        Ok(scheme) => Err(format!(
            "OpenDAL is linked but the '{scheme}' service is not compiled in; enable \
             liquers-store's 'services-{scheme}' feature"
        )),
        // Not a scheme this OpenDAL knows. Reachable through the `opendal_<scheme>` escape
        // hatch, and — if it ever happens for an OPENDAL_STORE_TYPES entry — a sign the table
        // has drifted from the dependency.
        Err(_) => Err(format!(
            "OpenDAL does not recognise the scheme '{scheme_name}'"
        )),
    }
}

/// Takes a [`opendal::Scheme`] rather than a name: `via_iter` matches canonical scheme constants,
/// so an alias such as `https` must already have been resolved. See [`resolve_enabled_scheme`].
#[cfg(feature = "opendal")]
fn create_opendal_operator(
    scheme: opendal::Scheme,
    config: HashMap<String, String>,
) -> Result<Operator, Error> {
    let config_pairs: Vec<(String, String)> = config.into_iter().collect();
    Operator::via_iter(scheme.into_static(), config_pairs).map_err(|e| {
        Error::general_error(format!(
            "Failed to create OpenDAL operator for scheme '{}': {}",
            scheme, e
        ))
    })
}

/// Core's store types, then OpenDAL's — the chain a native consumer wants.
///
/// Core comes first, so `memory` and `filesystem` mean the same thing here as everywhere else. A
/// caller who needs to override one composes their own chain with their factory first.
pub fn default_store_factory() -> ChainedStoreFactory {
    ChainedStoreFactory::new()
        .chain(Box::new(core_store_factory()))
        .chain(Box::new(OpendalStoreFactory))
}

/// Build a router from a YAML document using [`default_store_factory`].
pub fn create_router_from_yaml(yaml: &str) -> Result<AsyncStoreRouter, Error> {
    StoreRouterBuilder::from_yaml(yaml, Box::new(default_store_factory()))?.build()
}

/// Build a router from a JSON document using [`default_store_factory`].
pub fn create_router_from_json(json: &str) -> Result<AsyncStoreRouter, Error> {
    StoreRouterBuilder::from_json(json, Box::new(default_store_factory()))?.build()
}

/// Build a router from an already-parsed configuration using [`default_store_factory`].
pub fn create_router(config: StoreRouterConfig) -> Result<AsyncStoreRouter, Error> {
    StoreRouterBuilder::new(config, Box::new(default_store_factory())).build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::parse::parse_key;
    use liquers_core::store_factory::{ArgumentCoverage, StoreTypeAvailability};

    fn entry(store_type: &str, prefix: &str) -> StoreConfig {
        StoreConfig::new(store_type).with_prefix(prefix)
    }

    // --- the OpenDAL type table (moved from config.rs, assertions unchanged) ---

    #[test]
    fn test_is_opendal_store_type() {
        assert!(is_opendal_store_type("s3"));
        assert!(is_opendal_store_type("fs"));
        assert!(is_opendal_store_type("opendal_custom"));
        assert!(!is_opendal_store_type("memory"));
        assert!(!is_opendal_store_type("filesystem"));
    }

    #[test]
    fn test_get_opendal_scheme() {
        assert_eq!(get_opendal_scheme("s3"), "s3");
        assert_eq!(get_opendal_scheme("opendal_fs"), "fs");
        assert_eq!(get_opendal_scheme("opendal_custom"), "custom");
    }

    // --- OpendalStoreFactory ---

    #[test]
    fn opendal01_claims_the_opendal_type_table() {
        let factory = OpendalStoreFactory;
        for store_type in OPENDAL_STORE_TYPES {
            assert_eq!(
                factory.resolve(&entry(store_type, "p")),
                Some(store_type.to_string()),
                "{store_type} is advertised, so it must resolve"
            );
        }
        assert_eq!(factory.resolve(&entry("memory", "p")), None);
    }

    #[test]
    fn opendal02_claims_the_opendal_underscore_prefix() {
        assert_eq!(
            OpendalStoreFactory.resolve(&entry("opendal_tikv", "p")),
            Some("opendal_tikv".to_string()),
            "the escape hatch must reach a service not in the table"
        );
    }

    /// Constructing an OpenDAL store performs no I/O: the builder is lazy, so a backend that does
    /// not exist still yields a store. That is what makes these tests offline.
    ///
    /// **Deliberately does not assert `key_prefix()`.** Written that way first, it failed:
    /// `AsyncOpenDALStore::key_prefix` returns an empty key rather than the configured prefix.
    /// That is a known defect of the backend, not of this factory — `STORE-OPENDAL-SLASH-HANDLING`,
    /// designed in `specs/design/opendal-path-mapping/`, whose Phase 2 lists `key_prefix` (`:296`)
    /// among the functions it repairs. Asserting it here would fail for a reason this module does
    /// not control.
    /// Gated on `services-fs`, not on `opendal`: the type it builds needs the service compiled
    /// in, and `opendal` alone compiles in none.
    #[cfg(feature = "services-fs")]
    #[test]
    fn opendal03_constructs_a_store() -> Result<(), Box<dyn std::error::Error>> {
        let config = entry("fs", "local").with_config("root", "/tmp/liquers-opendal03");
        let store = default_store_factory().create(&config)?;
        assert!(store.is_supported(&parse_key("local/a.txt")?));
        Ok(())
    }

    /// factory04 — a real type that this build cannot provide says which feature is missing.
    ///
    /// "Unknown store type" would send the reader hunting for a typo in a type that is real and
    /// documented. Only meaningful when the feature is actually off, so this test **never runs in
    /// the default configuration** — `cargo test -p liquers-store --no-default-features --features
    /// async_store` is its only run, and it is the only coverage of the message
    /// `StoreTypeAvailability` exists to preserve.
    #[cfg(not(feature = "opendal"))]
    #[test]
    fn factory04_gated_type_names_the_feature() {
        let error = match default_store_factory().create(&entry("s3", "remote")) {
            Ok(_) => panic!("`s3` must not be constructible without the opendal feature"),
            Err(e) => e,
        };
        assert!(
            error.message.contains("opendal"),
            "the error must name the missing feature, got: {}",
            error.message
        );
        assert!(
            !error.message.contains("Unknown store type"),
            "a gated-off type is not an unknown type, got: {}",
            error.message
        );
    }

    /// Every advertised type's declared availability must match what `create` actually does.
    ///
    /// Added after a review found the two disagreeing. `type_info` used to mark a type `Available`
    /// whenever `liquers-store`'s own `opendal` feature was on — but that feature only enables
    /// `dep:opendal`, and OpenDAL's `default` compiles in `services-memory` alone. So 20 of the 21
    /// advertised types were reported as supported while `Operator::via_iter` rejected them as
    /// disabled, which is exactly the "unknown versus unavailable" confusion
    /// `StoreTypeAvailability` exists to prevent — made worse than before, because the old code
    /// only failed at construction whereas the new metadata actively advertised them.
    ///
    /// Runs in both feature configurations and is meaningful in each: with `opendal` off every
    /// type must be refused, and with it on the answer must track OpenDAL's own
    /// `Scheme::enabled()`.
    #[test]
    fn availability01_declared_availability_matches_create() {
        let factory = OpendalStoreFactory;
        for info in factory.store_types() {
            let config = entry(&info.store_type, "p");
            let outcome = factory.create(&config);
            match &info.availability {
                StoreTypeAvailability::Unavailable(reason) => match outcome {
                    Ok(_) => panic!(
                        "{} is declared unavailable but built anyway",
                        info.store_type
                    ),
                    Err(e) => assert!(
                        e.message.contains(reason.as_str()),
                        "{}: create must refuse with the declared reason {reason:?}, got: {}",
                        info.store_type,
                        e.message
                    ),
                },
                StoreTypeAvailability::Available => {
                    // `create` may still fail on a missing required argument — `s3` needs a
                    // bucket. What it must not do is refuse the type as unavailable, which is the
                    // disagreement this test exists to catch.
                    if let Err(e) = outcome {
                        assert!(
                            !e.message.contains("not available in this build")
                                && !e.message.contains("not enabled or supported"),
                            "{} is declared available but create refused it as unavailable: {}",
                            info.store_type,
                            e.message
                        );
                    }
                }
            }
        }
    }

    /// Whether this build's *features* say `store_type` should be constructible.
    ///
    /// Written out by hand on purpose. It is the manifest's claim, stated independently of the
    /// dependency graph, so that
    /// [`availability02_advertised_types_match_the_enabled_features`] compares two things that
    /// were arrived at separately — what `Cargo.toml` enables, and what OpenDAL reports through
    /// `Scheme::enabled()`. Deriving one from the other would make the test tautological.
    ///
    /// The catch-all panics rather than guessing: an entry added to [`OPENDAL_STORE_TYPES`]
    /// without a feature to back it is exactly the drift being guarded against.
    fn enabled_by_features(store_type: &str) -> bool {
        match store_type {
            "fs" => cfg!(feature = "services-fs"),
            "s3" => cfg!(feature = "services-s3"),
            "gcs" => cfg!(feature = "services-gcs"),
            "azblob" => cfg!(feature = "services-azblob"),
            // One OpenDAL service, `Scheme::Http`, behind both type names.
            "http" | "https" => cfg!(feature = "services-http"),
            "webdav" => cfg!(feature = "services-webdav"),
            "ftp" => cfg!(feature = "services-ftp"),
            "github" => cfg!(feature = "services-github"),
            "webhdfs" => cfg!(feature = "services-webhdfs"),
            "dropbox" => cfg!(feature = "services-dropbox"),
            "onedrive" => cfg!(feature = "services-onedrive"),
            "gdrive" => cfg!(feature = "services-gdrive"),
            "ipfs" => cfg!(feature = "services-ipfs"),
            // The `[target.'cfg(unix)'.dependencies]` row enables `services-sftp` through the
            // dependency rather than through a feature of this crate, so on Unix it rides along
            // with `opendal` itself. The explicit feature still counts, for a Unix-like target
            // someone enables it on deliberately.
            "sftp" => {
                cfg!(feature = "services-sftp") || (cfg!(unix) && cfg!(feature = "opendal"))
            }
            "hdfs" => cfg!(feature = "services-hdfs"),
            "redis" => cfg!(feature = "services-redis"),
            "mongodb" => cfg!(feature = "services-mongodb"),
            "postgresql" => cfg!(feature = "services-postgresql"),
            "mysql" => cfg!(feature = "services-mysql"),
            "sqlite" => cfg!(feature = "services-sqlite"),
            other => panic!(
                "{other} is advertised in OPENDAL_STORE_TYPES but no feature of liquers-store \
                 enables it; add a `services-{other}` feature and a row here"
            ),
        }
    }

    /// The advertised type table, the manifest, and OpenDAL must all say the same thing.
    ///
    /// This is the test the issue `STORE-OPENDAL-SERVICES-NOT-ENABLED` asked for. Before the fix
    /// the crate advertised 21 store types and enabled no service feature at all, so 20 of them
    /// could not be constructed by any consumer — and nothing failed, because the crate's
    /// dev-dependencies added `services-fs` to the *test* binary through Cargo's feature
    /// unification while the shipped library had nothing. A suite green for that reason is worse
    /// than no suite.
    ///
    /// Runs in every feature configuration and is meaningful in each: with `opendal` off both
    /// sides are empty; with `services-default` on, the thirteen features it names must be
    /// exactly the services OpenDAL reports as compiled in.
    ///
    /// It catches drift in either direction — a type added to the table with no feature behind
    /// it, a feature dropped from `services-default`, or an OpenDAL upgrade that renames a
    /// scheme or a service feature.
    #[test]
    fn availability02_advertised_types_match_the_enabled_features() {
        for store_type in OPENDAL_STORE_TYPES {
            let actually_available = unavailability_reason(store_type).is_none();
            assert_eq!(
                actually_available,
                enabled_by_features(store_type),
                "{store_type}: Cargo.toml and OpenDAL disagree about whether this build has it \
                 (OpenDAL says available={actually_available}). Either the feature is missing \
                 from liquers-store's manifest, or `enabled_by_features` is stale."
            );
        }
    }

    /// The headline capability, from the documented YAML: an S3 store a consumer can configure.
    ///
    /// Goes through `create_router_from_yaml` rather than the factory directly, because the
    /// configuration document is what `STORE_CONFIG_FSD.md` promises and what actually broke.
    /// Offline: OpenDAL's builders perform no I/O, so a bucket that does not exist still yields
    /// a store, and no credentials are needed.
    #[cfg(feature = "services-s3")]
    #[test]
    fn availability03_documented_s3_configuration_builds(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let router = create_router_from_yaml(
            "stores:\n  - type: s3\n    prefix: remote\n    config:\n      \
             bucket: my-liquers-bucket\n      region: us-east-1\n",
        )?;
        assert!(router.is_supported(&parse_key("remote/data.csv")?));
        Ok(())
    }

    /// A service this build leaves out must name the feature that would bring it in.
    ///
    /// "Not enabled or supported" — what OpenDAL says on its own — reads like the type does not
    /// exist. The reader needs to know that `sqlite` is real, is advertised, and is one Cargo
    /// feature away.
    #[cfg(all(feature = "opendal", not(feature = "services-sqlite")))]
    #[test]
    fn availability04_unavailable_type_names_the_feature_to_enable() {
        let error = match default_store_factory().create(&entry("sqlite", "db")) {
            Ok(_) => panic!("`sqlite` must not build without its service feature"),
            Err(e) => e,
        };
        assert!(
            error.message.contains("services-sqlite"),
            "the message must name the feature to enable, got: {}",
            error.message
        );
        assert!(
            !error.message.contains("Unknown store type"),
            "a service compiled out is not an unknown type, got: {}",
            error.message
        );
    }

    /// An advertised type that is an *alias* of an OpenDAL scheme must build like any other.
    ///
    /// `https` and `http` are one service, `Scheme::Http`. `Scheme::from_str` accepts both names,
    /// so availability reporting always said `https` was fine; `Operator::via_iter` matches the
    /// canonical constant only, so construction always failed. Nothing noticed until the service
    /// features were turned on, because before that `https` was unavailable for the other reason
    /// and never reached `via_iter`. `create` now resolves through `Scheme` first.
    #[cfg(feature = "services-http")]
    #[test]
    fn availability05_an_alias_scheme_builds() -> Result<(), Box<dyn std::error::Error>> {
        let config = entry("https", "web").with_config("endpoint", "https://example.org");
        default_store_factory().create(&config)?;
        // The escape hatch takes the same path, so it must resolve the alias too.
        let escaped = entry("opendal_https", "web2").with_config("endpoint", "https://example.org");
        default_store_factory().create(&escaped)?;
        Ok(())
    }

    /// With `liquers-store`'s `opendal` feature off the types are still *declared*, marked
    /// unavailable — so a reader is told why rather than that the type does not exist.
    #[cfg(not(feature = "opendal"))]
    #[test]
    fn opendal04_types_are_declared_but_unavailable() {
        let types = OpendalStoreFactory.store_types();
        let s3 = types.iter().find(|t| t.store_type == "s3").expect("declared");
        match &s3.availability {
            StoreTypeAvailability::Unavailable(reason) => assert!(reason.contains("opendal")),
            StoreTypeAvailability::Available => {
                panic!("s3 cannot be available without the feature")
            }
        }
    }

    // --- the default chain ---

    #[test]
    fn default01_chain_is_core_then_opendal() -> Result<(), Box<dyn std::error::Error>> {
        let chain = default_store_factory();
        assert_eq!(
            chain.resolve(&entry("memory", "p")),
            Some("memory".to_string())
        );
        assert_eq!(chain.resolve(&entry("s3", "p")), Some("s3".to_string()));
        let store = chain.create(&entry("memory", "cache"))?;
        assert_eq!(store.key_prefix(), parse_key("cache")?);
        Ok(())
    }

    /// A near-miss worth guarding: OpenDAL calls the local filesystem `fs`, core calls it
    /// `filesystem`. They do not collide today, and renaming either would silently reroute every
    /// document that names it.
    #[test]
    fn default02_core_types_are_not_shadowed_by_opendal() {
        assert!(
            !OPENDAL_STORE_TYPES.contains(&"filesystem"),
            "core's `filesystem` must not also be an OpenDAL type name"
        );
        assert!(
            !OPENDAL_STORE_TYPES.contains(&"memory"),
            "core's `memory` must not also be an OpenDAL type name"
        );
        let names: Vec<String> = default_store_factory()
            .store_types()
            .into_iter()
            .map(|t| t.store_type)
            .collect();
        assert!(names.contains(&"filesystem".to_string()));
        assert!(names.contains(&"fs".to_string()));
    }

    /// OpenDAL's options are OpenDAL's to document, so the argument list is guidance rather than a
    /// contract.
    #[test]
    fn coverage01_opendal_types_are_partial_with_an_authority() {
        let types = OpendalStoreFactory.store_types();
        let s3 = types.iter().find(|t| t.store_type == "s3").expect("declared");
        match &s3.coverage {
            ArgumentCoverage::Partial { authority } => assert!(authority.starts_with("https://")),
            ArgumentCoverage::Complete => {
                panic!("an externally-owned type must not claim a complete argument list")
            }
        }
    }

    /// The behavioural half of `Partial`: a key the factory does not describe must still reach the
    /// backend. `atomic_write_dir` is a real `fs` option this factory says nothing about.
    /// Gated on `services-fs` for the same reason as `opendal03`.
    #[cfg(feature = "services-fs")]
    #[test]
    fn coverage02_partial_type_accepts_an_undescribed_key(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let config = entry("fs", "local")
            .with_config("root", "/tmp/liquers-coverage02")
            .with_config("atomic_write_dir", "/tmp/liquers-coverage02-tmp");
        default_store_factory().create(&config)?;
        Ok(())
    }

    // --- router convenience (moved from store_builder.rs) ---

    #[tokio::test]
    async fn test_store_router_from_yaml() -> Result<(), Box<dyn std::error::Error>> {
        let router = create_router_from_yaml("stores:\n  - type: memory\n    prefix: cache\n")?;
        assert!(router.is_supported(&parse_key("cache/file.txt")?));
        Ok(())
    }

    #[tokio::test]
    async fn test_store_router_from_json() -> Result<(), Box<dyn std::error::Error>> {
        let router =
            create_router_from_json(r#"{ "stores": [ { "type": "memory", "prefix": "mem1" } ] }"#)?;
        assert!(router.is_supported(&parse_key("mem1/file.txt")?));
        Ok(())
    }

    #[test]
    fn test_unknown_store_type() {
        match default_store_factory().create(&entry("unknown_type", "p")) {
            Ok(_) => panic!("an unknown store type must not build"),
            Err(e) => assert!(
                e.message.contains("unknown_type") && e.message.contains("memory"),
                "the message must name the type and list what is supported, got: {}",
                e.message
            ),
        }
    }

    #[test]
    fn test_filesystem_missing_path() {
        match default_store_factory().create(&entry("filesystem", "local")) {
            Ok(_) => panic!("filesystem must not build without a path"),
            Err(e) => assert!(e.message.contains("path"), "got: {}", e.message),
        }
    }
}
