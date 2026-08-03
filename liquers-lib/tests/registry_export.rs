//! Tests for the exported command registry and its committed copy.
//!
//! These need `#[tokio::test]`: `DefaultEnvironment::new()` builds a `DefaultAssetManager`,
//! which calls `tokio::spawn` and panics without a runtime.

use std::path::PathBuf;

use liquers_core::command_metadata::{CommandKey, CommandMetadataRegistry};
use liquers_core::context::Environment;
use liquers_core::error::Error;
use liquers_core::validate::{
    from_json_or_yaml, validate_query, ValidationLevel, ValidationRegistryBuilder,
    ValidationStatus,
};
use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::value::Value;

// The `register_*_commands!` macros expand to code naming these unqualified.
#[allow(unused_imports)]
use liquers_core::{
    command_metadata::{ArgumentType, CommandDefinition, CommandParameterValue},
    context::{Context, Environment as _},
    state::State,
    value::ValueInterface,
};
#[allow(unused_imports)]
use liquers_lib::value::simple::SimpleValue;

type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

/// Build the registry the exporter would produce with every compiled-in group.
///
/// Mirrors the binary rather than calling `register_all_commands!`, which does not compile when
/// a feature is off because the macros it expands to do not exist then.
fn full_registry() -> Result<CommandMetadataRegistry, Error> {
    let mut env = CommandEnvironment::new();
    {
        let cr = env.get_mut_command_registry();
        liquers_lib::register_core_commands!(cr)?;
        liquers_lib::register_lui_commands!(cr)?;
        #[cfg(feature = "egui")]
        liquers_lib::register_egui_commands!(cr)?;
        #[cfg(feature = "image-support")]
        liquers_lib::register_image_commands!(cr)?;
        #[cfg(feature = "polars")]
        liquers_lib::register_polars_commands!(cr)?;
    }
    Ok(env.get_command_metadata_registry().clone())
}

fn committed_registry_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("specs/command_registry.yaml")
}

/// I6 — guards against a silently empty export.
#[tokio::test]
async fn exported_registry_is_nonempty() -> Result<(), Error> {
    let registry = full_registry()?;
    assert!(
        registry.commands.len() >= 20,
        "suspiciously small export: {} commands",
        registry.commands.len()
    );
    assert!(registry.find_command("", "root", "to_text").is_some());
    Ok(())
}

/// I4 — the contract between the two binaries: what the exporter writes, the validator reads.
#[tokio::test]
async fn exported_registry_roundtrips_into_validator() -> Result<(), Error> {
    let registry = full_registry()?;
    let yaml = serde_yaml::to_string(&registry)
        .map_err(|e| Error::from_error(liquers_core::error::ErrorType::General, e))?;

    let mut builder = ValidationRegistryBuilder::new();
    builder.merge_str("exported.yaml", &yaml)?;
    let (validation_registry, provenance) = builder.build();

    assert_eq!(provenance.command_count, registry.commands.len());

    let result = validate_query(
        "-R/data/report.txt/-/to_text",
        0,
        ValidationLevel::Plan,
        &validation_registry,
    );
    assert_eq!(result.status, ValidationStatus::Ok, "{:?}", result.error);
    Ok(())
}

/// I5 — namespace filtering, as `--namespaces` performs it.
#[tokio::test]
async fn namespace_filtering_selects_expected_commands() -> Result<(), Error> {
    let mut registry = full_registry()?;
    let before = registry.commands.len();

    registry.commands.retain(|c| c.namespace == "pl");
    assert!(
        registry.commands.len() < before,
        "filtering must remove something"
    );

    #[cfg(feature = "polars")]
    {
        assert!(!registry.commands.is_empty(), "polars commands are in 'pl'");
        assert!(registry.find_command("", "pl", "head").is_some());
    }
    Ok(())
}

/// Compare two commands by signature, ignoring implementation identity.
///
/// `metadata_version` cannot be used for this: it is `#[serde(skip)]`, so it never reaches the
/// file and always reads back as zero. Instead this reproduces what
/// `CommandMetadataRegistry::calculate_metadata_version` hashes — the command with
/// `impl_version` zeroed — so an implementation edit does not fire the freshness test while any
/// signature, doc, label or argument change does.
///
/// Comparing structures rather than file bytes also keeps the test from failing spuriously on a
/// serde_yaml formatting change.
fn signature_of(command: &liquers_core::command_metadata::CommandMetadata) -> String {
    let mut command = command.clone();
    command.impl_version = liquers_core::metadata::Version::new(0);
    serde_json::to_string(&command).unwrap_or_else(|e| format!("unserializable: {e}"))
}

/// The committed registry must match the registered commands.
#[tokio::test]
async fn committed_registry_is_fresh() -> Result<(), Error> {
    let path = committed_registry_path();
    let text = std::fs::read_to_string(&path).map_err(|e| {
        Error::general_error(format!("Cannot read '{}': {}", path.display(), e))
    })?;
    let committed: CommandMetadataRegistry = from_json_or_yaml(&path.display().to_string(), &text)?;
    let current = full_registry()?;

    let regenerate = "Regenerate with:\n  cargo run -p liquers-lib --features cli \
                      --bin export-command-registry -- --format yaml -o specs/command_registry.yaml";

    // CommandKey is Eq + Hash but not Ord, so a hash set rather than a btree set.
    let committed_keys: std::collections::HashSet<CommandKey> =
        committed.commands.iter().map(|c| c.key()).collect();
    let current_keys: std::collections::HashSet<CommandKey> =
        current.commands.iter().map(|c| c.key()).collect();

    let mut missing: Vec<String> = current_keys
        .difference(&committed_keys)
        .map(|k| format!("{}/{}/{}", k.realm, k.namespace, k.name))
        .collect();
    let mut extra: Vec<String> = committed_keys
        .difference(&current_keys)
        .map(|k| format!("{}/{}/{}", k.realm, k.namespace, k.name))
        .collect();
    missing.sort();
    extra.sort();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "specs/command_registry.yaml is stale.\n  missing from the file: {missing:?}\n  \
         no longer registered: {extra:?}\n{regenerate}"
    );

    for command in &current.commands {
        let key = command.key();
        let committed_command = committed
            .get(key.clone())
            .expect("key sets already compared");
        assert_eq!(
            signature_of(committed_command),
            signature_of(command),
            "signature of '{}/{}/{}' changed since the last export.\n{regenerate}",
            key.realm,
            key.namespace,
            key.name
        );
    }
    Ok(())
}
