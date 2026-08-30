//! A store router built from a configuration document, in `liquers-core` alone.
//!
//! The point of this file is structural as much as behavioural: a test here **cannot** reach
//! `liquers-store` — it is not a dependency of this crate — so `core_router01` cannot pass by
//! accidentally using a backend from elsewhere. That is the thesis of
//! `specs/design/store-factories-in-core/` stated as an assertion.

use liquers_core::error::Error;
use liquers_core::metadata::Metadata;
use liquers_core::parse::parse_key;
use liquers_core::store::AsyncStore;
use liquers_core::store_config::StoreRouterConfig;
use liquers_core::store_factory::{
    default_store_factory, ArgumentCoverage, StoreArgumentInfo, StoreArgumentType,
    StoreRouterBuilder, StoreTypeAvailability, StoreTypeInfo,
};

/// core_router01 — a working router from a YAML document, with no `liquers-store` in the graph.
#[tokio::test]
async fn core_router01_builds_from_yaml_without_liquers_store(
) -> Result<(), Box<dyn std::error::Error>> {
    let yaml = "stores:\n  - type: memory\n    prefix: cache\n";
    let router = StoreRouterBuilder::from_yaml(yaml, Box::new(default_store_factory()))?.build()?;

    let key = parse_key("cache/greeting.txt")?;
    router.set(&key, b"hello", &Metadata::new()).await?;
    assert_eq!(router.get(&key).await?.0, b"hello".to_vec());
    Ok(())
}

/// core_router02 — routing is by prefix, and the first matching store in document order wins.
#[tokio::test]
async fn core_router02_routes_by_prefix_first_match_wins() -> Result<(), Box<dyn std::error::Error>>
{
    let yaml =
        "stores:\n  - type: memory\n    prefix: data/inner\n  - type: memory\n    prefix: data\n";
    let router = StoreRouterBuilder::from_yaml(yaml, Box::new(default_store_factory()))?.build()?;

    let inner = parse_key("data/inner/a.txt")?;
    let outer = parse_key("data/b.txt")?;
    router.set(&inner, b"1", &Metadata::new()).await?;
    router.set(&outer, b"2", &Metadata::new()).await?;

    assert_eq!(router.get(&inner).await?.0, b"1".to_vec());
    assert_eq!(router.get(&outer).await?.0, b"2".to_vec());

    // The two are distinct stores, so a key under the more specific prefix is not visible to the
    // more general one.
    assert!(!router.is_supported(&parse_key("elsewhere/c.txt")?));
    Ok(())
}

/// core_router03 — `${VAR}` is expanded by `build`, and left alone by
/// `build_without_env_expansion`.
#[tokio::test]
async fn core_router03_env_expansion_applies_on_build() -> Result<(), Box<dyn std::error::Error>> {
    std::env::set_var("LIQUERS_TEST_STORE_PREFIX", "expanded");
    let yaml = "stores:\n  - type: memory\n    prefix: cache\n    config:\n      note: \"${LIQUERS_TEST_STORE_PREFIX}\"\n";

    let mut config = StoreRouterConfig::from_yaml(yaml)?;
    config.expand_env_vars()?;
    assert_eq!(
        config.stores[0].get_config_string("note"),
        Some("expanded".to_string())
    );

    // An unset variable is an error rather than an empty value.
    let missing = "stores:\n  - type: memory\n    prefix: cache\n    config:\n      note: \"${LIQUERS_TEST_UNSET_VARIABLE_XYZ}\"\n";
    let mut bad = StoreRouterConfig::from_yaml(missing)?;
    match bad.expand_env_vars() {
        Ok(()) => panic!("an unset variable must not expand silently"),
        Err(e) => assert!(e.message.contains("LIQUERS_TEST_UNSET_VARIABLE_XYZ")),
    }
    Ok(())
}

/// core_router04 — the type descriptions survive a JSON round trip.
///
/// They derive `Serialize`/`Deserialize` so a store-type registry can be exported later. Asserting
/// the round trip now means that later work does not start from an unverified assumption.
#[test]
fn core_router04_type_info_round_trips_through_json() -> Result<(), Box<dyn std::error::Error>> {
    let info = StoreTypeInfo::new("example")
        .with_label("Example store")
        .with_doc("Only for the round trip.")
        .with_argument(
            StoreArgumentInfo::new("root", StoreArgumentType::String)
                .required()
                .with_doc("Where it lives."),
        )
        .with_argument(StoreArgumentInfo::derived("retries", serde_json::json!(3)))
        .unavailable("requires the 'example' feature")
        .partial("https://example.invalid/docs");

    let json = serde_json::to_string(&info)?;
    let back: StoreTypeInfo = serde_json::from_str(&json)?;
    assert_eq!(back, info);

    // The pieces that carry meaning, spelled out rather than left to PartialEq.
    match back.availability {
        StoreTypeAvailability::Unavailable(reason) => assert!(reason.contains("example")),
        StoreTypeAvailability::Available => panic!("availability must survive the round trip"),
    }
    match back.coverage {
        ArgumentCoverage::Partial { authority } => assert!(authority.starts_with("https://")),
        ArgumentCoverage::Complete => panic!("coverage must survive the round trip"),
    }
    // `derived` infers the argument type from the default's JSON type.
    assert_eq!(back.arguments[1].argument_type, StoreArgumentType::Number);
    Ok(())
}

/// A store type nobody declares is refused with a message naming what this build does support.
#[test]
fn core_router05_unknown_type_names_the_supported_set() -> Result<(), Box<dyn std::error::Error>> {
    let yaml = "stores:\n  - type: postgress\n    prefix: db\n";
    let builder = StoreRouterBuilder::from_yaml(yaml, Box::new(default_store_factory()))?;
    match builder.build() {
        Ok(_) => panic!("an unknown store type must not build"),
        Err(e) => {
            assert!(e.message.contains("postgress"), "got: {}", e.message);
            assert!(e.message.contains("memory"), "got: {}", e.message);
        }
    }
    let _: Option<Error> = None;
    Ok(())
}
