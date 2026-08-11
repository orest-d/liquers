//! Report types for query validation.
//!
//! The central decision these types encode: **a validation failure is data, not an `Err`.**
//! A malformed query is the expected output of a validator, so it lands in
//! [`ValidationResult::error`]. `Err` is reserved for the tool itself failing — an unreadable
//! file, an unparseable registry, bad arguments.

use crate::error::{Error, ErrorType};
use crate::plan::{Plan, Step};
use crate::query::{Key, Query};

/// How far validation goes.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidationLevel {
    /// Parse only.
    #[default]
    Parse,
    /// Parse, then build the plan against a command registry.
    Plan,
}

impl std::str::FromStr for ValidationLevel {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        match s {
            "parse" => Ok(ValidationLevel::Parse),
            "plan" => Ok(ValidationLevel::Plan),
            other => Err(Error::general_error(format!(
                "Unknown validation level '{other}'; expected 'parse' or 'plan'"
            ))),
        }
    }
}

impl std::fmt::Display for ValidationLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationLevel::Parse => write!(f, "parse"),
            ValidationLevel::Plan => write!(f, "plan"),
        }
    }
}

/// Outcome of one validation. Ordered so the worst status of a batch is `max()`.
#[derive(
    Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash,
)]
pub enum ValidationStatus {
    #[default]
    Ok,
    /// Validation succeeded; the plan carries an error or warning step.
    Warning,
    /// The query did not parse, or the plan could not be constructed.
    Error,
}

impl std::fmt::Display for ValidationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationStatus::Ok => write!(f, "OK"),
            ValidationStatus::Warning => write!(f, "WARNING"),
            ValidationStatus::Error => write!(f, "ERROR"),
        }
    }
}

/// Where a warning came from.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningSource {
    /// [`Plan::error`].
    PlanError,
    /// A [`Step::Error`] in `steps` or `init_steps`.
    StepError,
    /// A [`Step::Warning`] in `steps` or `init_steps`.
    StepWarning,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ValidationWarning {
    pub source: WarningSource,
    pub message: String,
}

impl ValidationWarning {
    pub fn new(source: WarningSource, message: String) -> Self {
        ValidationWarning { source, message }
    }
}

/// Which recipe check ran.
///
/// Both are correct outcomes for their input; `AdHoc` is not a degraded `Stored`. A recipe need
/// not be stored under a key at all, and the payload boundary is a property of *being* keyed.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipeCheck {
    /// The recipe resolves to a storage key; the payload boundary was enforced.
    Stored,
    /// Ad-hoc recipe with no storage key; the payload boundary does not apply.
    AdHoc,
}

/// Result for a single query or recipe.
///
/// Not `PartialEq`: it embeds [`Plan`], which derives only
/// `Serialize, Deserialize, Debug, Clone`. Compare fields in tests.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ValidationResult {
    /// Position in the input; 0 for a single query.
    pub index: usize,
    /// The query text exactly as supplied.
    pub source: String,
    /// The query re-encoded from the parsed [`Query`].
    ///
    /// Diffing this against `source` reveals mis-parses at [`ValidationLevel::Parse`], with no
    /// registry at all — notably a resource query that swallowed an intended action into its key.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub encoded: Option<String>,
    /// 1-based line in the source file, when the input came from a query file.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub line: Option<usize>,
    /// Recipe title, when the input was a recipe list.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub title: Option<String>,
    /// Which recipe check ran. `None` for query inputs, and when the storage key could not be
    /// computed because the recipe's own query does not parse.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub recipe_check: Option<RecipeCheck>,
    pub status: ValidationStatus,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub query: Option<Query>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub plan: Option<Plan>,
    /// Storage key the recipe result would land under (`Recipe::store_to_key`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub key: Option<Key>,
    /// Set when `status == Error`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<Error>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<ValidationWarning>,
}

impl ValidationResult {
    /// A result that has not failed yet; callers fill in `query`, `plan` and friends.
    pub fn new(index: usize, source: String) -> Self {
        ValidationResult {
            index,
            source,
            encoded: None,
            line: None,
            title: None,
            recipe_check: None,
            status: ValidationStatus::Ok,
            query: None,
            plan: None,
            key: None,
            error: None,
            warnings: Vec::new(),
        }
    }

    /// A failed result. This is the *expected* shape for a bad query — not an `Err`.
    pub fn failed(index: usize, source: String, error: Error) -> Self {
        ValidationResult {
            status: ValidationStatus::Error,
            error: Some(error),
            ..ValidationResult::new(index, source)
        }
    }

    /// Attach a plan and derive the status from the diagnostics it carries.
    pub fn with_plan(mut self, plan: Plan) -> Self {
        self.warnings = collect_warnings(&plan);
        if !self.warnings.is_empty() {
            self.status = ValidationStatus::Warning;
        }
        self.plan = Some(plan);
        self
    }

    pub fn with_line(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }
}

/// Where the registry came from, so an "unknown command" caused by a wrong name can be told
/// apart from one caused by validating against the wrong registry.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct RegistryProvenance {
    /// Files merged in, in order.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub merged_files: Vec<String>,
    /// Permissive commands declared on the command line.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub cli_commands: Vec<crate::command_metadata::CommandKey>,
    /// Total commands in the assembled registry.
    pub command_count: usize,
    /// Namespaces searched by default.
    pub default_namespaces: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationCounts {
    pub total: usize,
    pub ok: usize,
    pub warning: usize,
    pub error: usize,
}

/// The whole envelope: one of these is serialized to stdout per run.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ValidationReport {
    /// Worst status across `results`.
    pub status: ValidationStatus,
    pub level: ValidationLevel,
    pub registry: RegistryProvenance,
    pub results: Vec<ValidationResult>,
    pub counts: ValidationCounts,
}

impl ValidationReport {
    /// 0 when nothing failed (warnings included), 1 when any result errored.
    ///
    /// Exit 2 is reserved for the *tool* failing and is produced by the binary, not here.
    pub fn exit_code(&self) -> i32 {
        match self.status {
            ValidationStatus::Ok | ValidationStatus::Warning => 0,
            ValidationStatus::Error => 1,
        }
    }

    /// Human-readable lines for stderr. Stdout carries only the serialized envelope.
    pub fn diagnostic_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for result in &self.results {
            match result.status {
                ValidationStatus::Ok => {}
                ValidationStatus::Warning => {
                    for warning in &result.warnings {
                        lines.push(format!(
                            "WARNING  [{}] {} {}",
                            result.index,
                            match warning.source {
                                WarningSource::PlanError => "Plan contains error:",
                                WarningSource::StepError => "Plan step error:",
                                WarningSource::StepWarning => "Plan step warning:",
                            },
                            warning.message
                        ));
                    }
                }
                ValidationStatus::Error => {
                    let message = result
                        .error
                        .as_ref()
                        .map(|e| e.message.clone())
                        .unwrap_or_else(|| "validation failed".to_string());
                    lines.push(format!("ERROR    [{}] {}", result.index, message));
                }
            }
        }
        lines
    }

    pub fn to_json(&self) -> Result<String, Error> {
        serde_json::to_string_pretty(self).map_err(|e| Error::from_error(ErrorType::General, e))
    }

    pub fn to_yaml(&self) -> Result<String, Error> {
        serde_yaml::to_string(self).map_err(|e| Error::from_error(ErrorType::General, e))
    }
}

/// Collect diagnostics from a successfully built plan.
///
/// Two subtleties, both load-bearing:
///
/// * [`Plan::set_error`] sets `error` *and* pushes a matching [`Step::Error`] into `init_steps`,
///   so the same problem would otherwise be reported twice. The `init_steps` entry whose message
///   equals the plan error's rendering is skipped.
/// * [`Step::Plan`] nests a whole plan. Without recursing, an error inside a nested plan is
///   invisible and the result wrongly reports `Ok`.
pub fn collect_warnings(plan: &Plan) -> Vec<ValidationWarning> {
    let mut warnings = Vec::new();
    let plan_error_message = plan.error.as_ref().map(|e| e.to_string());

    if let Some(error) = &plan.error {
        warnings.push(ValidationWarning::new(
            WarningSource::PlanError,
            error.message.clone(),
        ));
    }

    for step in plan.init_steps.iter().chain(plan.steps.iter()) {
        collect_step_warnings(step, plan_error_message.as_deref(), &mut warnings);
    }

    warnings
}

/// Walk one step. The match is exhaustive with no `_ =>` arm, so a new [`Step`] variant is a
/// compile error rather than a silently ignored diagnostic.
fn collect_step_warnings(
    step: &Step,
    plan_error_message: Option<&str>,
    warnings: &mut Vec<ValidationWarning>,
) {
    match step {
        Step::Error(message) => {
            // Skip the echo of `Plan::error` that `set_error` pushes into `init_steps`.
            if Some(message.as_str()) != plan_error_message {
                warnings.push(ValidationWarning::new(
                    WarningSource::StepError,
                    message.clone(),
                ));
            }
        }
        Step::Warning(message) => {
            warnings.push(ValidationWarning::new(
                WarningSource::StepWarning,
                message.clone(),
            ));
        }
        Step::Plan(nested) => {
            for warning in collect_warnings(nested) {
                warnings.push(ValidationWarning::new(
                    warning.source,
                    format!("nested plan: {}", warning.message),
                ));
            }
        }
        Step::GetAsset(_)
        | Step::GetAssetBinary(_)
        | Step::GetAssetMetadata(_)
        | Step::GetAssetRecipe(_)
        | Step::GetAssetDirectory(_)
        | Step::GetResource(_)
        | Step::GetResourceMetadata(_)
        | Step::GetResourceDirectory(_)
        | Step::Evaluate(_)
        | Step::UseQueryValue(_)
        | Step::Action { .. }
        | Step::Filename(_)
        | Step::Info(_)
        | Step::SetCwd(_)
        | Step::UseKeyValue(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::Position;

    fn plan_with_steps(steps: Vec<Step>) -> Plan {
        Plan {
            steps,
            ..Plan::new()
        }
    }

    fn ok_result(index: usize) -> ValidationResult {
        ValidationResult::new(index, "q".to_string())
    }

    /// U16 — report status is the worst of its results.
    #[test]
    fn status_ordering_is_ok_lt_warning_lt_error() {
        assert!(ValidationStatus::Ok < ValidationStatus::Warning);
        assert!(ValidationStatus::Warning < ValidationStatus::Error);
        let worst = [
            ValidationStatus::Ok,
            ValidationStatus::Error,
            ValidationStatus::Warning,
        ]
        .into_iter()
        .max();
        assert_eq!(worst, Some(ValidationStatus::Error));
    }

    /// U18 — a warning is not a failure.
    #[test]
    fn exit_code_zero_for_warning_one_for_error() {
        let report = |status| ValidationReport {
            status,
            level: ValidationLevel::Plan,
            registry: RegistryProvenance::default(),
            results: Vec::new(),
            counts: ValidationCounts::default(),
        };
        assert_eq!(report(ValidationStatus::Ok).exit_code(), 0);
        assert_eq!(report(ValidationStatus::Warning).exit_code(), 0);
        assert_eq!(report(ValidationStatus::Error).exit_code(), 1);
    }

    /// U19 — `set_error` also pushes a Step::Error into init_steps; report it once.
    #[test]
    fn warnings_deduplicate_plan_error() {
        let mut plan = Plan::new();
        plan.set_error(Error::general_error("boom".to_string()));

        let warnings = collect_warnings(&plan);
        let plan_errors = warnings
            .iter()
            .filter(|w| w.source == WarningSource::PlanError)
            .count();
        let step_errors = warnings
            .iter()
            .filter(|w| w.source == WarningSource::StepError)
            .count();

        assert_eq!(plan_errors, 1, "plan error reported once");
        assert_eq!(step_errors, 0, "the init_steps echo is suppressed");
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
    }

    #[test]
    fn warnings_collect_step_error_and_warning() {
        let plan = plan_with_steps(vec![
            Step::Info("ignored".to_string()),
            Step::Warning("careful".to_string()),
            Step::Error("bad".to_string()),
            Step::Filename(crate::query::ResourceName::new("x.txt".to_string())),
        ]);
        let warnings = collect_warnings(&plan);

        assert_eq!(warnings.len(), 2, "Info and Filename are not diagnostics");
        assert!(warnings
            .iter()
            .any(|w| w.source == WarningSource::StepWarning && w.message == "careful"));
        assert!(warnings
            .iter()
            .any(|w| w.source == WarningSource::StepError && w.message == "bad"));
    }

    /// U30 — an error inside a nested plan must not be invisible.
    #[test]
    fn warnings_recurse_into_nested_plan() {
        let nested = plan_with_steps(vec![Step::Error("inner problem".to_string())]);
        let outer = plan_with_steps(vec![Step::Plan(nested)]);

        let warnings = collect_warnings(&outer);
        assert_eq!(warnings.len(), 1, "nested diagnostics are collected");
        assert_eq!(warnings[0].source, WarningSource::StepError);
        assert!(
            warnings[0].message.starts_with("nested plan: "),
            "depth is readable: {}",
            warnings[0].message
        );
        assert!(warnings[0].message.contains("inner problem"));
    }

    #[test]
    fn with_plan_sets_warning_status() {
        let clean = ok_result(0).with_plan(Plan::new());
        assert_eq!(clean.status, ValidationStatus::Ok);

        let noisy = ok_result(0).with_plan(plan_with_steps(vec![Step::Warning("w".to_string())]));
        assert_eq!(noisy.status, ValidationStatus::Warning);
        assert_eq!(noisy.warnings.len(), 1);
    }

    /// U25
    #[test]
    fn diagnostic_lines_format() {
        let warned = ok_result(3).with_plan(plan_with_steps(vec![Step::Error("oops".to_string())]));
        let failed = ValidationResult::failed(
            7,
            "bad".to_string(),
            Error::general_error("nope".to_string()),
        );
        let report = ValidationReport {
            status: ValidationStatus::Error,
            level: ValidationLevel::Plan,
            registry: RegistryProvenance::default(),
            results: vec![ok_result(0), warned, failed],
            counts: ValidationCounts::default(),
        };

        let lines = report.diagnostic_lines();
        assert_eq!(lines.len(), 2, "an Ok result emits nothing: {lines:?}");
        assert!(lines[0].starts_with("WARNING"), "{}", lines[0]);
        assert!(
            lines[0].contains("[3]") && lines[0].contains("oops"),
            "{}",
            lines[0]
        );
        assert!(lines[1].starts_with("ERROR"), "{}", lines[1]);
        assert!(
            lines[1].contains("[7]") && lines[1].contains("nope"),
            "{}",
            lines[1]
        );
    }

    /// U20 — both formats round-trip.
    #[test]
    fn report_json_yaml_roundtrip() -> Result<(), Error> {
        let mut result = ok_result(0);
        result.encoded = Some("to_text".to_string());
        result.line = Some(4);
        result.recipe_check = Some(RecipeCheck::AdHoc);
        let report = ValidationReport {
            status: ValidationStatus::Ok,
            level: ValidationLevel::Plan,
            registry: RegistryProvenance {
                command_count: 2,
                default_namespaces: vec!["".to_string(), "root".to_string()],
                ..RegistryProvenance::default()
            },
            results: vec![result],
            counts: ValidationCounts {
                total: 1,
                ok: 1,
                warning: 0,
                error: 0,
            },
        };

        let json = report.to_json()?;
        let from_json: ValidationReport = serde_json::from_str(&json).expect("json roundtrip");
        assert_eq!(from_json.status, report.status);
        assert_eq!(from_json.counts, report.counts);
        assert_eq!(from_json.registry, report.registry);
        assert_eq!(from_json.results[0].encoded, Some("to_text".to_string()));
        assert_eq!(from_json.results[0].line, Some(4));
        assert_eq!(from_json.results[0].recipe_check, Some(RecipeCheck::AdHoc));

        let yaml = report.to_yaml()?;
        let from_yaml: ValidationReport = serde_yaml::from_str(&yaml).expect("yaml roundtrip");
        assert_eq!(from_yaml.counts, report.counts);
        Ok(())
    }

    /// U21 — a clean result carries no empty keys.
    ///
    /// Serializes the *result* rather than the whole report: `ValidationCounts` has a legitimate
    /// `error` field, so scanning the envelope for `"error"` would always match.
    #[test]
    fn optional_fields_omitted() -> Result<(), Error> {
        let json = serde_json::to_string_pretty(&ok_result(0))
            .map_err(|e| Error::from_error(ErrorType::General, e))?;

        for absent in [
            "\"error\"",
            "\"warnings\"",
            "\"key\"",
            "\"title\"",
            "\"line\"",
            "\"encoded\"",
            "\"recipe_check\"",
            "\"query\"",
            "\"plan\"",
        ] {
            assert!(
                !json.contains(absent),
                "{absent} should be omitted:\n{json}"
            );
        }
        assert!(json.contains("\"status\""));
        assert!(json.contains("\"source\""));
        Ok(())
    }

    #[test]
    fn level_from_str_and_display() -> Result<(), Error> {
        use std::str::FromStr;
        assert_eq!(ValidationLevel::from_str("parse")?, ValidationLevel::Parse);
        assert_eq!(ValidationLevel::from_str("plan")?, ValidationLevel::Plan);
        assert!(ValidationLevel::from_str("Plan").is_err(), "case sensitive");
        let err = ValidationLevel::from_str("nonsense").expect_err("must reject");
        assert!(err.message.contains("nonsense"), "{err}");
        assert_eq!(ValidationLevel::Plan.to_string(), "plan");
        Ok(())
    }

    /// U3 — position fidelity is what makes the diagnostic useful.
    #[test]
    fn failed_result_preserves_error_position() {
        let error = Error::general_error("x".to_string()).with_position(&Position::new(5, 1, 6));
        let result = ValidationResult::failed(0, "q".to_string(), error);

        let error = result.error.expect("error present");
        assert_eq!(error.position.line, 1);
        assert_eq!(error.position.column, 6);
        assert_eq!(error.position.offset, 5);
    }
}
