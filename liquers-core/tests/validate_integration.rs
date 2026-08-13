//! End-to-end tests for the query validation module, exercised through its public API.
//!
//! See `specs/design/query-validation/phase3-examples.md` for the scenarios these cover.

use liquers_core::command_metadata::{ArgumentInfo, CommandMetadata, CommandMetadataRegistry};
use liquers_core::error::Error;
use liquers_core::plan::Step;
use liquers_core::validate::{
    build_report, validate_query, ValidationLevel, ValidationRegistryBuilder, ValidationStatus,
};

fn registry_text(names: &[(&str, &str)]) -> String {
    let mut cmr = CommandMetadataRegistry::new();
    for (name, namespace) in names {
        let mut cm = CommandMetadata::new(name);
        cm.namespace = namespace.to_string();
        cmr.add_command(&cm);
    }
    serde_yaml::to_string(&cmr).expect("serialize registry")
}

/// Registry text for one command that declares a single integer argument.
///
/// `registry_text` declares commands with no arguments at all, which is fine for a query that
/// supplies none. A query such as `head-10` needs the argument to exist: an action may not
/// supply more parameters than its command declares.
fn registry_text_with_int_arg(name: &str, namespace: &str, argument: &str) -> String {
    let mut cmr = CommandMetadataRegistry::new();
    let mut cm = CommandMetadata::new(name);
    cm.namespace = namespace.to_string();
    cm.arguments = vec![ArgumentInfo::integer_argument(argument, false)];
    cmr.add_command(&cm);
    serde_yaml::to_string(&cmr).expect("serialize registry")
}

/// I1 — assemble an empty registry, validate at level Parse, serialize the envelope.
#[test]
fn end_to_end_parse_only() -> Result<(), Error> {
    let (registry, provenance) = ValidationRegistryBuilder::new().build();
    assert_eq!(provenance.command_count, 0);

    let results = vec![validate_query(
        "-R/data/report.txt/-/to_text",
        0,
        ValidationLevel::Parse,
        &registry,
    )];
    let report = build_report(ValidationLevel::Parse, provenance, results);

    assert_eq!(report.status, ValidationStatus::Ok);
    assert_eq!(report.exit_code(), 0);

    let json = report.to_json()?;
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(parsed["level"], "Parse");
    assert_eq!(
        parsed["results"][0]["encoded"],
        "-R/data/report.txt/-/to_text"
    );
    assert!(
        parsed["results"][0]["plan"].is_null(),
        "no plan at level Parse"
    );
    Ok(())
}

/// I2 — merge a serialized registry from a string, then plan against it.
#[test]
fn end_to_end_plan_with_merged_registry() -> Result<(), Error> {
    let mut builder = ValidationRegistryBuilder::new();
    builder.merge_str("test.yaml", &registry_text_with_int_arg("head", "pl", "n"))?;
    let (registry, provenance) = builder.build();

    assert_eq!(provenance.command_count, 1);
    assert_eq!(provenance.merged_files, vec!["test.yaml".to_string()]);

    let result = validate_query(
        "-R/data/sales.csv/-/ns-pl/head-10",
        0,
        ValidationLevel::Plan,
        &registry,
    );
    assert_eq!(result.status, ValidationStatus::Ok, "{:?}", result.error);

    let plan = result.plan.expect("plan built");
    assert_eq!(plan.steps.len(), 2);
    match &plan.steps[1] {
        Step::Action {
            ns, action_name, ..
        } => {
            assert_eq!(ns, "pl", "the resolved namespace rides along in the step");
            assert_eq!(action_name, "head");
        }
        other => panic!("expected an Action step, got {other:?}"),
    }
    Ok(())
}

/// I3 — the `-R/` swallowing trap.
///
/// This is the behaviour the whole design is built around: both queries are valid, and only the
/// plan (or `encoded`) distinguishes them. Pinning it here means a future parser change cannot
/// silently invalidate the tool's central example.
#[test]
fn swallowing_trap_plans_differ() -> Result<(), Error> {
    let mut builder = ValidationRegistryBuilder::new();
    builder.merge_str("r.yaml", &registry_text(&[("to_text", "root")]))?;
    let (registry, _) = builder.build();

    let intended = validate_query(
        "-R/data/report.txt/-/to_text",
        0,
        ValidationLevel::Plan,
        &registry,
    );
    let typo = validate_query(
        "-R/data/report.txt/to_text",
        1,
        ValidationLevel::Plan,
        &registry,
    );

    // Neither is an error. That is the point.
    assert_eq!(
        intended.status,
        ValidationStatus::Ok,
        "{:?}",
        intended.error
    );
    assert_eq!(typo.status, ValidationStatus::Ok, "{:?}", typo.error);

    let intended_plan = intended.plan.expect("plan");
    let typo_plan = typo.plan.expect("plan");
    assert_eq!(intended_plan.steps.len(), 2, "GetAsset + Action");
    assert_eq!(typo_plan.steps.len(), 1, "one GetAsset, action swallowed");

    match (&intended_plan.steps[0], &typo_plan.steps[0]) {
        (Step::GetAsset(intended_key), Step::GetAsset(typo_key)) => {
            assert_eq!(intended_key.len(), 2, "data/report.txt");
            assert_eq!(typo_key.len(), 3, "data/report.txt/to_text");
        }
        other => panic!("expected two GetAsset steps, got {other:?}"),
    }
    Ok(())
}

/// The design-overlay workflow: a base registry plus a proposal that changes a signature.
#[test]
fn design_overlay_requires_allow_overwrite() -> Result<(), Error> {
    let base = registry_text(&[("head", "pl")]);
    let overlay = registry_text(&[("head", "pl")]);

    let mut strict = ValidationRegistryBuilder::new();
    strict.merge_str("base.yaml", &base)?;
    let err = strict
        .merge_str("proposed.yaml", &overlay)
        .expect_err("redefining a command must be reported by default");
    assert!(err.message.contains("--allow-overwrite"), "{err}");

    let mut permissive = ValidationRegistryBuilder::new().with_overwrite_allowed(true);
    permissive.merge_str("base.yaml", &base)?;
    permissive.merge_str("proposed.yaml", &overlay)?;
    let (_, provenance) = permissive.build();
    assert_eq!(provenance.command_count, 1, "the overlay replaced the base");
    assert_eq!(provenance.merged_files.len(), 2);
    Ok(())
}

/// Permissive CLI commands stand in for commands that do not exist yet.
#[test]
fn permissive_commands_validate_undesigned_queries() -> Result<(), Error> {
    let mut builder = ValidationRegistryBuilder::new();
    builder.add_permissive_command("greet")?;
    builder.add_permissive_command("custom/transform")?;
    let (registry, provenance) = builder.build();

    assert_eq!(provenance.cli_commands.len(), 2);
    // CommandKey::new normalizes the default namespace `root` to the empty string.
    assert_eq!(provenance.cli_commands[0].namespace, "");

    for query in [
        "greet",
        "greet-world",
        "greet-a-b-c-d-e",
        "ns-custom/greet-world",
    ] {
        let result = validate_query(query, 0, ValidationLevel::Plan, &registry);
        assert_eq!(
            result.status,
            ValidationStatus::Ok,
            "{query} should plan: {:?}",
            result.error
        );
    }
    Ok(())
}

/// A registry file is accepted in either format, with no flag telling the tool which it is.
///
/// Each format must be produced by its own serializer. `ArgumentGUIInfo` is an externally-tagged
/// enum, which serde_yaml writes as a YAML tag (`!TextField`) and serde_json as a map — so
/// converting one text into the other through a generic `Value` yields a hybrid that neither
/// parser accepts. That is a property of the formats, not a limitation of `from_json_or_yaml`.
#[test]
fn registry_accepts_json_and_yaml() -> Result<(), Error> {
    let mut cmr = CommandMetadataRegistry::new();
    cmr.add_command(&CommandMetadata::new("to_text"));

    let yaml = serde_yaml::to_string(&cmr).expect("to yaml");
    let json = serde_json::to_string_pretty(&cmr).expect("to json");

    for (name, text) in [("r.yaml", &yaml), ("r.json", &json)] {
        let mut builder = ValidationRegistryBuilder::new();
        builder.merge_str(name, text)?;
        let (registry, provenance) = builder.build();
        assert_eq!(provenance.command_count, 1, "{name} should load");
        assert!(registry.find_command("", "root", "to_text").is_some());
    }
    Ok(())
}
