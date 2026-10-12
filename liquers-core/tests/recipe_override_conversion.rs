//! `Recipe::to_plan` with argument overrides: type conversion with planning warnings, sorted
//! order, typed errors and aliases. Design: `specs/design/axum-query-named-arguments/`
//! (Phase 3, ROC01–ROC06).

use std::collections::HashMap;

use liquers_core::{
    command_metadata::{
        ArgumentInfo, ArgumentType, CommandDefinition, CommandKey, CommandMetadata,
        CommandMetadataRegistry, CommandParameterValue, EnumArgument,
    },
    error::ErrorType,
    parse::parse_query,
    plan::{ParameterValue, Plan, Step},
    recipes::Recipe,
};
use serde_json::{json, Value};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// `echo_args(query: string = "", rank: bool = true, limit: int = 50, ratio: float_opt,
/// mode: enum, fields: multiple string, ctx: injected)` and an alias `echo_alias` of it with the
/// head `query = "fixed"` and its own argument `flag` (filling `rank`).
fn registry() -> CommandMetadataRegistry {
    let mut cmr = CommandMetadataRegistry::new();
    let mode = EnumArgument::new("mode")
        .with_alternative("fast")
        .with_string_value("s", "slow")
        .with_link("generated", parse_query("make_text").expect("query"));
    let mut echo = CommandMetadata::new("echo_args");
    echo.with_argument(ArgumentInfo::string_argument("query").with_default(""))
        .with_argument(ArgumentInfo::boolean_argument("rank").with_default(true))
        .with_argument(ArgumentInfo::integer_argument("limit", false).with_default(50))
        .with_argument(ArgumentInfo::float_argument("ratio", true))
        .with_argument(
            ArgumentInfo::argument("mode")
                .with_type(ArgumentType::Enum(mode))
                .with_default("fast"),
        )
        .with_argument(ArgumentInfo::string_argument("fields").set_multiple())
        .with_argument(ArgumentInfo::argument("ctx").set_injected());
    cmr.add_command(&echo);
    cmr.add_command(&CommandMetadata::new("make_text"));
    let mut alias = CommandMetadata::new("echo_alias");
    alias.with_argument(ArgumentInfo::boolean_argument("flag"));
    alias.definition = CommandDefinition::Alias {
        command: CommandKey::new_name("echo_args"),
        head_parameters: vec![CommandParameterValue::Value(json!("fixed"))],
    };
    cmr.add_command(&alias);
    cmr
}

fn recipe(query: &str, arguments: &[(&str, Value)]) -> Result<Recipe, Box<dyn std::error::Error>> {
    let mut recipe = Recipe::new(query.to_string(), String::new(), String::new())?;
    recipe.arguments = arguments
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect::<HashMap<_, _>>();
    Ok(recipe)
}

/// The resolved parameter `name` of the last action.
fn last_action_parameter(plan: &Plan, name: &str) -> Option<ParameterValue> {
    plan.steps.iter().rev().find_map(|step| {
        if let Step::Action { parameters, .. } = step {
            parameters
                .iter()
                .find(|p| p.name().as_deref() == Some(name))
                .cloned()
        } else {
            None
        }
    })
}

fn warnings(plan: &Plan) -> Vec<String> {
    plan.init_steps
        .iter()
        .filter_map(|step| {
            if let Step::Warning(message) = step {
                Some(message.clone())
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn roc01_mismatched_json_converts_with_a_warning() -> TestResult {
    let cmr = registry();
    let cases: Vec<(&str, Value, Value)> = vec![
        ("limit", json!("20"), json!(20)),
        ("limit", json!(3.0), json!(3)),
        ("query", json!(20), json!("20")),
        ("query", json!(true), json!("true")),
        ("rank", json!("yes"), json!(true)),
        ("ratio", json!("0.5"), json!(0.5)),
    ];
    for (name, given, expected) in cases {
        let plan = recipe("echo_args", &[(name, given.clone())])?.to_plan(&cmr)?;
        let parameter = last_action_parameter(&plan, name).ok_or("parameter")?;
        assert_eq!(parameter.value(), Some(expected.clone()), "{name} = {given}");
        let warnings = warnings(&plan);
        assert_eq!(warnings.len(), 1, "{name} = {given}: {warnings:?}");
        assert!(warnings[0].contains(&format!("'{name}'")), "{}", warnings[0]);
    }
    // Matching types and a null for an optional argument add no warning.
    for (name, value) in [("limit", json!(7)), ("query", json!("x")), ("ratio", Value::Null)] {
        let plan = recipe("echo_args", &[(name, value)])?.to_plan(&cmr)?;
        assert!(warnings(&plan).is_empty(), "{name}");
    }
    // A variadic converts element-wise.
    let plan = recipe("echo_args", &[("fields", json!(["a", 2]))])?.to_plan(&cmr)?;
    match last_action_parameter(&plan, "fields").ok_or("fields")? {
        ParameterValue::MultipleParameters(_, elements) => {
            let values: Vec<_> = elements.iter().filter_map(|e| e.value()).collect();
            assert_eq!(values, vec![json!("a"), json!("2")]);
        }
        other => return Err(format!("expected a list, got {other}").into()),
    }
    assert_eq!(warnings(&plan).len(), 1);
    Ok(())
}

#[test]
fn roc02_unknown_injected_or_actionless_name_is_parameter_error() -> TestResult {
    let cmr = registry();
    for (query, name) in [("echo_args", "nope"), ("echo_args", "ctx"), ("-R/notes/todo.txt", "x")] {
        let error = recipe(query, &[(name, json!("v"))])?
            .to_plan(&cmr)
            .err()
            .ok_or("expected an error")?;
        assert_eq!(error.error_type, ErrorType::ParameterError, "{query} {name}: {error}");
    }
    Ok(())
}

#[test]
fn roc03_unconvertible_value_is_conversion_error() -> TestResult {
    let cmr = registry();
    for value in [json!("abc"), Value::Null, json!(2.5), json!(true)] {
        let error = recipe("echo_args", &[("limit", value.clone())])?
            .to_plan(&cmr)
            .err()
            .ok_or("expected an error")?;
        assert_eq!(error.error_type, ErrorType::ConversionError, "{value}: {error}");
    }
    Ok(())
}

#[test]
fn roc04_overrides_apply_in_sorted_order() -> TestResult {
    let cmr = registry();
    for _ in 0..5 {
        let plan = recipe("echo_args", &[("query", json!(1)), ("limit", json!("2"))])?
            .to_plan(&cmr)?;
        let warnings = warnings(&plan);
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].contains("'limit'") && warnings[1].contains("'query'"), "{warnings:?}");
    }
    Ok(())
}

#[test]
fn roc05_enum_alias_is_expanded() -> TestResult {
    let cmr = registry();
    // A value of the enum passes unchanged.
    let plan = recipe("echo_args", &[("mode", json!("fast"))])?.to_plan(&cmr)?;
    assert_eq!(last_action_parameter(&plan, "mode").and_then(|p| p.value()), Some(json!("fast")));
    assert!(warnings(&plan).is_empty());
    // An alias expands to its value.
    let plan = recipe("echo_args", &[("mode", json!("s"))])?.to_plan(&cmr)?;
    assert_eq!(last_action_parameter(&plan, "mode").and_then(|p| p.value()), Some(json!("slow")));
    assert_eq!(warnings(&plan).len(), 1);
    // A query alias becomes a link override.
    let plan = recipe("echo_args", &[("mode", json!("generated"))])?.to_plan(&cmr)?;
    match last_action_parameter(&plan, "mode").ok_or("mode")? {
        ParameterValue::OverrideLink(_, query) => assert_eq!(query.encode(), "make_text"),
        other => return Err(format!("expected a link, got {other}").into()),
    }
    Ok(())
}

#[test]
fn roc06_alias_overrides_use_alias_names_heads_refused() -> TestResult {
    let cmr = registry();
    let plan = recipe("echo_alias", &[("flag", json!(false))])?.to_plan(&cmr)?;
    assert_eq!(last_action_parameter(&plan, "flag").and_then(|p| p.value()), Some(json!(false)));
    let error = recipe("echo_alias", &[("query", json!("other"))])?
        .to_plan(&cmr)
        .err()
        .ok_or("a head slot must not be overridable")?;
    assert_eq!(error.error_type, ErrorType::ParameterError);
    Ok(())
}
