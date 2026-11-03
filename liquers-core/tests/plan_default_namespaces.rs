//! `plan-default-namespaces`: an environment's default namespaces, from its configuration.
//!
//! `specs/design/plan-default-namespaces/` Phase 3, test 13 (AC-1, AC-2, AC-6, AC-7).
use liquers_core::{
    context::{Context, GenericEnvironment},
    environment_builder::{EnvironmentBuilder, Inline},
    environment_config::EnvironmentConfig,
    error::{Error, ErrorType},
    state::State,
    store_factory::default_store_factory,
    value::Value,
};
use liquers_macro::register_command;

type CommandEnvironment = GenericEnvironment<Value, (), Inline>;
type Builder = EnvironmentBuilder<Value, (), Inline>;

fn shout(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("LOUD"))
}

fn builder_from(yaml: &str) -> Result<Builder, Box<dyn std::error::Error>> {
    let config = EnvironmentConfig::from_yaml(yaml)?;
    let mut builder = Builder::new().with_config(config, Box::new(default_store_factory()));
    let cr = &mut builder.command_registry;
    register_command!(cr, fn shout(state) -> result namespace: "x")?;
    Ok(builder)
}

#[tokio::test]
async fn configured_environment_evaluates_unqualified_query(
) -> Result<(), Box<dyn std::error::Error>> {
    let envref = builder_from(
        "store:\n  stores:\n    - type: memory\n      prefix: tmp\ndefault_namespaces: [x]\n",
    )?
    .build()?;
    assert_eq!(envref.default_namespaces(), ["x", "", "root"]);

    let asset = envref.evaluate_immediately("shout", ()).await?;
    let state = asset.get().await?;
    assert_eq!(state.try_into_string()?, "LOUD");
    Ok(())
}

#[tokio::test]
async fn unconfigured_environment_searches_root_only() -> Result<(), Box<dyn std::error::Error>> {
    let envref = builder_from("store:\n  stores: []\n")?.build()?;
    assert_eq!(envref.default_namespaces(), ["", "root"]);

    let error = match envref.evaluate_immediately("shout", ()).await {
        Ok(asset) => asset.get().await.expect_err("shout is only in namespace x"),
        Err(error) => error,
    };
    assert_eq!(error.error_type, ErrorType::ActionNotRegistered, "{error}");
    Ok(())
}

/// AC-6: the configuration takes precedence over the registry; an absent key leaves it alone.
#[test]
fn configured_namespaces_take_precedence_over_registry() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = builder_from("store:\n  stores: []\ndefault_namespaces: [x]\n")?;
    builder
        .command_registry
        .command_metadata_registry
        .default_namespaces = vec!["y".to_string(), "root".to_string()];
    let envref = builder.build()?;
    assert_eq!(envref.default_namespaces(), ["x", "", "root"]);

    let mut builder = builder_from("store:\n  stores: []\n")?;
    builder
        .command_registry
        .command_metadata_registry
        .default_namespaces = vec!["x".to_string(), "root".to_string()];
    let envref = builder.build()?;
    assert_eq!(envref.default_namespaces(), ["x", "root"]);
    Ok(())
}

/// AC-7: an empty entry is a configuration error at build.
#[test]
fn empty_default_namespace_is_a_configuration_error() -> Result<(), Box<dyn std::error::Error>> {
    let builder = builder_from("store:\n  stores: []\ndefault_namespaces: [x, '']\n")?;
    match builder.build() {
        Ok(_) => panic!("an empty default namespace must fail the build"),
        Err(error) => assert_eq!(error.error_type, ErrorType::ConfigurationError, "{error}"),
    }
    Ok(())
}

/// AC-7: `validate()` sees the configured chain before `build()`: an empty entry is an error and
/// an unknown namespace a warning, and validating does not consume the setting.
#[test]
fn validate_checks_the_configured_chain() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = builder_from("store:\n  stores: []\ndefault_namespaces: [x, '']\n")?;
    assert!(builder.validate().has_errors());

    let mut builder = builder_from("store:\n  stores: []\ndefault_namespaces: [x, typo]\n")?;
    let report = builder.validate().clone();
    assert!(!report.has_errors(), "{report}");
    assert_eq!(report.warning_count(), 1, "{report}");
    assert!(report.to_string().contains("typo"), "{report}");
    let envref = builder.build()?;
    assert_eq!(envref.default_namespaces(), ["x", "typo", "", "root"]);
    Ok(())
}
