//! `liquers-validate` and pipelines (AC-35 of `specs/design/composite-command-definition/`).

use liquers_core::command_metadata::{ArgumentInfo, CommandMetadata, CommandMetadataRegistry};
use liquers_core::error::Error;
use liquers_core::pipeline::Pipeline;
use liquers_core::recipes::RecipeList;
use liquers_core::validate::{
    build_report, pipeline_from_build_args, validate_pipeline, validate_query, validate_recipes,
    with_pipeline_form, ValidationLevel, ValidationRegistryBuilder, ValidationStatus,
    WarningSource,
};
use serde_json::json;

/// A registry with `upper` (no arguments), `suffix-s` and `describe-config: any`.
fn registry() -> Result<CommandMetadataRegistry, Error> {
    let mut cmr = CommandMetadataRegistry::new();
    cmr.add_command(&CommandMetadata::new("upper"));
    let mut suffix = CommandMetadata::new("suffix");
    suffix.arguments = vec![ArgumentInfo::string_argument("s")];
    cmr.add_command(&suffix);
    let mut describe = CommandMetadata::new("describe");
    describe.arguments = vec![ArgumentInfo::any_argument("config")];
    cmr.add_command(&describe);
    let mut builder = ValidationRegistryBuilder::new();
    builder.merge_str("test.yaml", &serde_yaml::to_string(&cmr).map_err(|e| {
        Error::general_error(e.to_string())
    })?)?;
    Ok(builder.build().0)
}

/// `value` without the `pos` a pipeline converted from a parsed query carries.
fn without_positions(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut map: serde_json::Map<String, serde_json::Value> = map
                .into_iter()
                .filter(|(k, _)| k != "pos")
                .map(|(k, v)| (k, without_positions(v)))
                .collect();
            // `{value: x}` without its position is the bare value.
            if map.len() == 1 {
                if let Some(v) = map.remove("value") {
                    return v;
                }
            }
            serde_json::Value::Object(map)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(without_positions).collect())
        }
        other => other,
    }
}

/// A pipeline file, as JSON or YAML, validates and reports its query form.
#[test]
fn validate_pipeline_file() -> Result<(), Error> {
    let cmr = registry()?;
    let yaml = "- resource: data/x.txt\n- action: upper\n- {action: suffix, parameters: [x y]}\n";
    let value: serde_json::Value = serde_yaml::from_str(yaml)
        .map_err(|e| Error::general_error(e.to_string()))?;
    let pipeline = Pipeline::from_json(&value)?;
    let result = validate_pipeline(&pipeline, "p.yaml".into(), 0, ValidationLevel::Plan, false, &cmr);
    assert_eq!(result.status, ValidationStatus::Ok, "{:?}", result.error);
    assert_eq!(result.encoded.as_deref(), Some("-R/data/x.txt/-/upper/suffix-x~.y"));
    assert!(result.plan.is_some());
    assert!(result.pipeline.is_some());
    Ok(())
}

/// The conversions on request: query → pipeline, pipeline → query, recipe → pipeline.
#[test]
fn validate_emit_query_and_pipeline() -> Result<(), Error> {
    let cmr = registry()?;
    let result = with_pipeline_form(validate_query(
        "-R/data/x.txt/-/upper",
        0,
        ValidationLevel::Plan,
        &cmr,
    ));
    assert_eq!(
        result.pipeline.map(without_positions),
        Some(json!({"elements": [{"resource": "data/x.txt"}, {"action": "upper"}]}))
    );
    let recipes: RecipeList = serde_yaml::from_str(
        "recipes:\n  - pipeline: [{value: abc}]\n    query: upper/out.txt\n",
    )
    .map_err(|e| Error::general_error(e.to_string()))?;
    let results = validate_recipes(&recipes, ValidationLevel::Plan, &cmr);
    assert_eq!(results[0].status, ValidationStatus::Ok, "{:?}", results[0].error);
    let pipeline = without_positions(
        results[0].pipeline.clone().ok_or(Error::general_error("no pipeline".into()))?,
    );
    assert_eq!(pipeline["elements"][1], json!({"action": "upper"}));
    assert_eq!(pipeline["filename"], json!("out.txt"));
    Ok(())
}

/// `--build` spells a query as shell arguments; values need no escaping.
#[test]
fn validate_build_from_shell_args() -> Result<(), Error> {
    let cmr = registry()?;
    let args: Vec<String> = ["R:data/x.txt", "A:upper", "A:suffix", "P:-a b/"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let text = pipeline_from_build_args(&args)?.to_query_text()?;
    let result = validate_query(&text, 0, ValidationLevel::Plan, &cmr);
    assert_eq!(result.status, ValidationStatus::Ok, "{:?}", result.error);
    let query = result.query.ok_or(Error::general_error("no query".into()))?;
    let pipeline = Pipeline::try_from(&query)?;
    assert_eq!(
        without_positions(pipeline.to_json())["elements"][2]["parameters"][0],
        json!("-a b/"),
        "{text}"
    );
    assert!(pipeline_from_build_args(&["P:x".to_string()]).is_err());
    assert!(pipeline_from_build_args(&["X:x".to_string()]).is_err());
    Ok(())
}

/// A pipeline without a query form plans with a warning naming the rule; with `--emit query`
/// it is an error, so the tool exits 1.
#[test]
fn validate_not_representable_exit_1() -> Result<(), Error> {
    let cmr = registry()?;
    let pipeline = Pipeline::from_json(&json!([
        {"value": 1},
        {"action": "describe", "parameters": [{"value": {"a": [1, 2]}}]}
    ]))?;
    let lenient = validate_pipeline(&pipeline, "p".into(), 0, ValidationLevel::Plan, false, &cmr);
    assert_eq!(lenient.status, ValidationStatus::Warning, "{:?}", lenient.error);
    assert!(lenient.encoded.is_none());
    assert!(lenient.plan.is_some());
    let warning = &lenient.warnings[0];
    assert_eq!(warning.source, WarningSource::NotRepresentable);
    assert!(warning.message.contains("not representable"), "{}", warning.message);

    let strict = validate_pipeline(&pipeline, "p".into(), 0, ValidationLevel::Plan, true, &cmr);
    assert_eq!(strict.status, ValidationStatus::Error);
    let report = build_report(ValidationLevel::Plan, Default::default(), vec![strict]);
    assert_eq!(report.exit_code(), 1);
    Ok(())
}

/// The binary: `--pipeline -` reads stdin, `--build` takes shell arguments, `--emit query` makes
/// a pipeline without a query form exit 1.
#[cfg(feature = "cli")]
#[test]
fn validate_binary_pipeline_and_build() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let exe = env!("CARGO_BIN_EXE_liquers-validate");
    let run = |args: &[&str], stdin: &str| -> Result<(i32, serde_json::Value), Box<dyn std::error::Error>> {
        let mut child = Command::new(exe)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        child.stdin.take().ok_or("stdin")?.write_all(stdin.as_bytes())?;
        let output = child.wait_with_output()?;
        let code = output.status.code().ok_or("signal")?;
        Ok((code, serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null)))
    };
    let (code, out) = run(
        &["--no-registry", "--pipeline", "-"],
        r#"[{"resource": "data/x.txt"}, {"action": "upper"}]"#,
    )?;
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["results"][0]["encoded"], "-R/data/x.txt/-/upper");
    let (code, out) = run(&["--no-registry", "--build", "R:data/x.txt", "A:suffix", "P:a b"], "")?;
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["results"][0]["encoded"], "-R/data/x.txt/-/suffix-a~.b");
    let (code, _) = run(
        &["--no-registry", "--emit", "query", "--pipeline", "-"],
        r#"[{"value": 1}, {"action": "describe", "parameters": [{"value": {"a": [1, 2]}}]}]"#,
    )?;
    assert_eq!(code, 1);
    Ok(())
}
