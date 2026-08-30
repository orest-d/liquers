//! Static validation of Liquers queries and recipes.
//!
//! Two levels: [`ValidationLevel::Parse`] checks that a query parses, and
//! [`ValidationLevel::Plan`] additionally builds the execution plan against a command registry,
//! which catches unknown commands and argument-type errors that parsing alone cannot see.
//!
//! Nothing here touches a store, evaluates an asset, or runs a command — validation is entirely
//! static, and therefore entirely synchronous.
//!
//! **Validation failures are data.** These functions do not return `Err` for a bad query; they
//! return a [`ValidationResult`] with `status == Error`. `Err` is reserved for the caller getting
//! something wrong (an unparseable `cwd`, a malformed registry). Conflating the two would force
//! callers to tell "your query is wrong" from "the validator is broken" by reading message text.
//!
//! ```no_run
//! use liquers_core::validate::{validate_query, ValidationLevel, ValidationRegistryBuilder};
//!
//! let (registry, provenance) = ValidationRegistryBuilder::new().build();
//! let result = validate_query("-R/data/report.txt/-/to_text", 0, ValidationLevel::Parse, &registry);
//! assert!(result.error.is_none());
//! let _ = provenance;
//! ```

pub mod registry;
pub mod report;

pub use registry::{from_json_or_yaml, ValidationRegistryBuilder};
pub use report::{
    collect_warnings, RecipeCheck, RegistryProvenance, ValidationCounts, ValidationLevel,
    ValidationReport, ValidationResult, ValidationStatus, ValidationWarning, WarningSource,
};

use crate::command_metadata::CommandMetadataRegistry;
use crate::error::Error;
use crate::parse::{parse_key, parse_query};
use crate::plan::PlanBuilder;
use crate::query::Query;
use crate::recipes::{Recipe, RecipeList};

/// Validate a single query string.
///
/// Never fails: a bad query is a [`ValidationResult`] with `status == Error` and `error` set.
pub fn validate_query(
    source: &str,
    index: usize,
    level: ValidationLevel,
    cmr: &CommandMetadataRegistry,
) -> ValidationResult {
    let query = match parse_query(source) {
        Ok(query) => query,
        Err(e) => return ValidationResult::failed(index, source.to_string(), e),
    };

    let mut result = ValidationResult::new(index, source.to_string());
    result.encoded = Some(query.encode());
    result.query = Some(query.clone());

    match level {
        ValidationLevel::Parse => result,
        ValidationLevel::Plan => match build_plan(&query, cmr) {
            Ok(plan) => result.with_plan(plan),
            Err(e) => ValidationResult {
                status: ValidationStatus::Error,
                error: Some(e),
                ..result
            },
        },
    }
}

fn build_plan(query: &Query, cmr: &CommandMetadataRegistry) -> Result<crate::plan::Plan, Error> {
    PlanBuilder::new(query.clone(), cmr).build()
}

/// Validate one recipe.
///
/// Uses [`Recipe::to_plan`], which checks more than a bare query: it also verifies that every
/// `arguments` and `links` override names something present in the plan's last action. When the
/// recipe resolves to a storage key, [`Recipe::to_plan_for_key`] is used instead, which
/// additionally enforces the payload boundary — a keyed asset is shared, so it can never receive
/// a per-evaluation payload.
pub fn validate_recipe(
    recipe: &Recipe,
    index: usize,
    level: ValidationLevel,
    cmr: &CommandMetadataRegistry,
) -> ValidationResult {
    let source = recipe.query.clone();

    let query = match recipe.get_query() {
        Ok(query) => query,
        Err(e) => return ValidationResult::failed(index, source, e),
    };

    let mut result = ValidationResult::new(index, source.clone());
    result.encoded = Some(query.encode());
    result.query = Some(query);
    if !recipe.title.is_empty() {
        result.title = Some(recipe.title.clone());
    }

    if level == ValidationLevel::Parse {
        return result;
    }

    // `store_to_key` is fallible for *query* reasons, not just cwd ones: it goes through
    // `filename()` -> `get_query()` -> `parse_query`. `?` is unavailable here — this function
    // returns a ValidationResult, not a Result — and using it would invert the API's contract.
    let store_key = match recipe.store_to_key() {
        Ok(key) => key,
        Err(e) => {
            return ValidationResult {
                status: ValidationStatus::Error,
                error: Some(e),
                ..result
            }
        }
    };

    let plan = match &store_key {
        // Stored recipe: the payload boundary applies.
        Some(key) => {
            result.recipe_check = Some(RecipeCheck::Stored);
            result.key = Some(key.clone());
            recipe.to_plan_for_key(cmr, key)
        }
        // Ad-hoc recipe: no storage key, so the payload boundary does not apply. This is a
        // correct outcome in its own right, not a weaker fallback.
        None => {
            result.recipe_check = Some(RecipeCheck::AdHoc);
            recipe.to_plan(cmr)
        }
    };

    match plan {
        Ok(plan) => result.with_plan(plan),
        Err(e) => ValidationResult {
            status: ValidationStatus::Error,
            error: Some(e),
            ..result
        },
    }
}

/// Validate every recipe in a list, in order.
pub fn validate_recipes(
    recipes: &RecipeList,
    level: ValidationLevel,
    cmr: &CommandMetadataRegistry,
) -> Vec<ValidationResult> {
    recipes
        .recipes
        .iter()
        .enumerate()
        .map(|(index, recipe)| validate_recipe(recipe, index, level, cmr))
        .collect()
}

/// Apply a CLI-supplied working directory to a recipe list.
///
/// Returns the indices of recipes that already declare their own `cwd`; the caller reports those
/// as per-recipe findings. `Err` is reserved for an unparseable `cwd`, which is a mistake in the
/// *invocation* rather than in the input.
///
/// This deliberately does not delegate to [`RecipeList::set_cwd`], which aborts on the first
/// offender — reducing a batch to a single finding — and partially mutates the list before
/// returning. It does parse the key first and pass the re-encoded form on, matching what
/// `DefaultRecipeProvider` does with a folder key.
pub fn apply_cwd(recipes: &mut RecipeList, cwd: &str) -> Result<Vec<usize>, Error> {
    let cwd = parse_key(cwd)?.encode();

    let mut conflicts = Vec::new();
    for (index, recipe) in recipes.recipes.iter_mut().enumerate() {
        match &recipe.cwd {
            None => recipe.cwd = Some(cwd.clone()),
            Some(_) => conflicts.push(index),
        }
    }
    Ok(conflicts)
}

/// The error reported for a recipe that declares its own `cwd` while one was supplied.
///
/// Mirrors `RecipeList::set_cwd`: such a `recipes.yaml` fails in `DefaultRecipeProvider` too, so
/// this is a genuine finding rather than validator strictness.
pub fn cwd_conflict_error() -> Error {
    Error::not_supported("CWD can't be explicitly specified in a recipe".to_owned())
}

/// Assemble results into the report, computing the overall status and the counts.
pub fn build_report(
    level: ValidationLevel,
    registry: RegistryProvenance,
    results: Vec<ValidationResult>,
) -> ValidationReport {
    let mut counts = ValidationCounts {
        total: results.len(),
        ..ValidationCounts::default()
    };
    for result in &results {
        match result.status {
            ValidationStatus::Ok => counts.ok += 1,
            ValidationStatus::Warning => counts.warning += 1,
            ValidationStatus::Error => counts.error += 1,
        }
    }

    // An empty batch is `Ok`, not `Error`: `max()` over nothing must not aggregate to a failure.
    let status = results
        .iter()
        .map(|r| r.status)
        .max()
        .unwrap_or(ValidationStatus::Ok);

    ValidationReport {
        status,
        level,
        registry,
        results,
        counts,
    }
}

/// Split query-file or stdin text into queries, one per line.
///
/// Blank lines and `#` comments are skipped so an agent can annotate a generated list. Both are
/// unambiguous: the query grammar admits neither a newline nor `#`. Each entry carries its true
/// 1-based source line, which skipping would otherwise lose.
pub fn split_query_lines(text: &str) -> Vec<(usize, String)> {
    text.lines()
        .enumerate()
        .filter_map(|(offset, line)| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                None
            } else {
                Some((offset + 1, trimmed.to_string()))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_metadata::{ArgumentInfo, CommandMetadata};
    use crate::error::ErrorType;

    /// A registry holding one state-only command in the default namespace.
    fn registry_with(name: &str, namespace: &str) -> CommandMetadataRegistry {
        let mut cmr = CommandMetadataRegistry::new();
        let mut cm = CommandMetadata::new(name);
        cm.namespace = namespace.to_string();
        cmr.add_command(&cm);
        cmr
    }

    fn permissive_registry(name: &str) -> CommandMetadataRegistry {
        let mut cmr = CommandMetadataRegistry::new();
        let mut cm = CommandMetadata::new(name);
        cm.arguments = vec![ArgumentInfo::any_argument("arguments").set_multiple()];
        cmr.add_command(&cm);
        cmr
    }

    fn recipe(query: &str) -> Recipe {
        Recipe {
            query: query.to_string(),
            ..Recipe::default()
        }
    }

    /// U1
    #[test]
    fn parse_level_ok() {
        let cmr = CommandMetadataRegistry::new();
        let result = validate_query(
            "-R/data/report.txt/-/to_text",
            0,
            ValidationLevel::Parse,
            &cmr,
        );

        assert_eq!(result.status, ValidationStatus::Ok);
        assert!(result.query.is_some(), "Query is reported");
        assert!(result.plan.is_none(), "no plan at level Parse");
        assert!(result.error.is_none());
        assert_eq!(
            result.encoded.as_deref(),
            Some("-R/data/report.txt/-/to_text")
        );
    }

    /// U2 + U3 — a parse failure is data, and it carries a usable position.
    #[test]
    fn parse_level_rejects_spaces_with_position() {
        let cmr = CommandMetadataRegistry::new();
        let result = validate_query("bad query with spaces", 0, ValidationLevel::Parse, &cmr);

        assert_eq!(result.status, ValidationStatus::Error);
        assert!(result.query.is_none());
        assert!(result.encoded.is_none());
        let error = result.error.expect("error is reported as data, not Err");
        assert_eq!(error.error_type, ErrorType::ParseError);
        assert_eq!(error.position.line, 1, "position is usable: {error:?}");
    }

    /// U4
    #[test]
    fn plan_level_ok_with_registry() {
        let cmr = registry_with("to_text", "root");
        let result = validate_query(
            "-R/data/report.txt/-/to_text",
            0,
            ValidationLevel::Plan,
            &cmr,
        );

        assert_eq!(result.status, ValidationStatus::Ok, "{:?}", result.error);
        let plan = result.plan.expect("plan built");
        assert_eq!(plan.steps.len(), 2, "GetAsset + Action");
    }

    /// U5 + C1 — every action fails against an empty registry.
    #[test]
    fn plan_level_unknown_command() {
        let cmr = CommandMetadataRegistry::new();
        let result = validate_query(
            "-R/data/report.txt/-/to_text",
            0,
            ValidationLevel::Plan,
            &cmr,
        );

        assert_eq!(result.status, ValidationStatus::Error);
        let error = result.error.expect("error");
        assert_eq!(error.error_type, ErrorType::ActionNotRegistered);
        assert!(error.message.contains("to_text"), "{error}");
    }

    /// U6
    #[test]
    fn index_is_preserved() {
        let cmr = CommandMetadataRegistry::new();
        assert_eq!(
            validate_query("to_text", 7, ValidationLevel::Parse, &cmr).index,
            7
        );
    }

    /// U15 — a permissive command accepts zero, one or many arguments of any type.
    #[test]
    fn permissive_command_accepts_any_argument_list() {
        let cmr = permissive_registry("greet");
        for query in ["greet", "greet-world", "greet-a-b-c-d-e"] {
            let result = validate_query(query, 0, ValidationLevel::Plan, &cmr);
            assert_eq!(
                result.status,
                ValidationStatus::Ok,
                "{query} should plan: {:?}",
                result.error
            );
        }
    }

    /// I3 as a unit test: the `-R/` swallowing trap. Both queries are valid; they mean
    /// different things, and only the plan (or `encoded`) shows it.
    #[test]
    fn resource_query_swallows_action_without_separator() {
        let cmr = registry_with("to_text", "root");

        let intended = validate_query(
            "-R/data/report.txt/-/to_text",
            0,
            ValidationLevel::Plan,
            &cmr,
        );
        let typo = validate_query("-R/data/report.txt/to_text", 1, ValidationLevel::Plan, &cmr);

        assert_eq!(intended.status, ValidationStatus::Ok);
        assert_eq!(
            typo.status,
            ValidationStatus::Ok,
            "the typo is NOT an error"
        );

        let intended_plan = intended.plan.expect("plan");
        let typo_plan = typo.plan.expect("plan");
        assert_eq!(intended_plan.steps.len(), 2, "GetAsset + Action");
        assert_eq!(typo_plan.steps.len(), 1, "everything became one key");

        match &typo_plan.steps[0] {
            crate::plan::Step::GetAsset(key) => {
                assert_eq!(key.len(), 3, "to_text was swallowed into the key: {key:?}")
            }
            other => panic!("expected GetAsset, got {other:?}"),
        }
    }

    /// U7 — an ad-hoc recipe has no storage key, and that is a correct outcome.
    #[test]
    fn recipe_adhoc_uses_to_plan() {
        let cmr = registry_with("to_text", "root");
        let result = validate_recipe(
            &recipe("-R/data/report.txt/-/to_text"),
            0,
            ValidationLevel::Plan,
            &cmr,
        );

        assert_eq!(result.status, ValidationStatus::Ok, "{:?}", result.error);
        assert_eq!(result.recipe_check, Some(RecipeCheck::AdHoc));
        assert!(result.key.is_none());
    }

    /// U8 — a recipe with a filename and a cwd is stored, and reports its key.
    #[test]
    fn recipe_stored_reports_key() -> Result<(), Error> {
        let cmr = registry_with("to_text", "root");
        let mut list = RecipeList::new();
        list.add_recipe(recipe("-R/data/report.txt/-/to_text/out.txt"));
        let conflicts = apply_cwd(&mut list, "reports")?;
        assert!(conflicts.is_empty());

        let results = validate_recipes(&list, ValidationLevel::Plan, &cmr);
        assert_eq!(results[0].recipe_check, Some(RecipeCheck::Stored));
        assert_eq!(
            results[0].key.as_ref().map(|k| k.encode()),
            Some("reports/out.txt".to_string())
        );
        Ok(())
    }

    /// A recipe whose query does not parse fails inside `store_to_key`, before `to_plan` —
    /// the path where a `?` would have been both a compile error and a contract violation.
    #[test]
    fn recipe_with_unparseable_query_is_data_not_err() {
        let cmr = CommandMetadataRegistry::new();
        let result = validate_recipe(&recipe("bad query"), 0, ValidationLevel::Plan, &cmr);

        assert_eq!(result.status, ValidationStatus::Error);
        assert!(result.error.is_some());
        assert!(
            result.recipe_check.is_none(),
            "no check ran, so none is claimed"
        );
    }

    /// U9
    #[test]
    fn recipe_bad_argument_override() {
        let cmr = permissive_registry("greet");
        let mut r = recipe("greet-world");
        r.arguments
            .insert("no_such_argument".to_string(), serde_json::json!(1));

        let result = validate_recipe(&r, 0, ValidationLevel::Plan, &cmr);
        assert_eq!(result.status, ValidationStatus::Error);
        let error = result.error.expect("error");
        assert!(
            error.message.contains("no_such_argument"),
            "names the argument: {error}"
        );
    }

    /// U10
    #[test]
    fn recipes_preserve_order_and_index() {
        let cmr = registry_with("to_text", "root");
        let mut list = RecipeList::new();
        for query in ["-R/a.txt/-/to_text", "bad query", "-R/b.txt/-/to_text"] {
            list.add_recipe(recipe(query));
        }

        let results = validate_recipes(&list, ValidationLevel::Plan, &cmr);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].index, 0);
        assert_eq!(results[1].index, 1);
        assert_eq!(results[2].index, 2);
        assert_eq!(results[1].status, ValidationStatus::Error);
    }

    /// U27 — every cwd conflict is reported, not just the first, and the batch continues.
    #[test]
    fn apply_cwd_reports_all_conflicts() -> Result<(), Error> {
        let mut list = RecipeList::new();
        let mut first = recipe("-R/a.txt");
        first.cwd = Some("elsewhere".to_string());
        let mut third = recipe("-R/c.txt");
        third.cwd = Some("other".to_string());
        list.add_recipe(first);
        list.add_recipe(recipe("-R/b.txt"));
        list.add_recipe(third);

        let conflicts = apply_cwd(&mut list, "reports")?;

        assert_eq!(conflicts, vec![0, 2], "both offenders, not just the first");
        assert_eq!(
            list.recipes[1].cwd,
            Some("reports".to_string()),
            "recipes without their own cwd still get it"
        );
        assert_eq!(list.recipes[0].cwd, Some("elsewhere".to_string()));
        Ok(())
    }

    /// U28 — an unparseable cwd is a caller mistake, so it IS an `Err`, and mutates nothing.
    #[test]
    fn apply_cwd_rejects_unparseable_key() {
        let mut list = RecipeList::new();
        list.add_recipe(recipe("-R/a.txt"));

        let err = apply_cwd(&mut list, "not a key!!").expect_err("must reject");
        assert_eq!(err.error_type, ErrorType::ParseError);
        assert!(list.recipes[0].cwd.is_none(), "nothing was mutated");
    }

    /// U17 + U23 — counts add up, and an empty batch is Ok rather than Error.
    #[test]
    fn build_report_counts_and_empty_batch() {
        let cmr = registry_with("to_text", "root");
        let results = vec![
            validate_query("-R/a.txt/-/to_text", 0, ValidationLevel::Plan, &cmr),
            validate_query("bad query", 1, ValidationLevel::Plan, &cmr),
        ];
        let report = build_report(
            ValidationLevel::Plan,
            RegistryProvenance::default(),
            results,
        );
        assert_eq!(report.counts.total, 2);
        assert_eq!(report.counts.ok, 1);
        assert_eq!(report.counts.error, 1);
        assert_eq!(
            report.counts.ok + report.counts.warning + report.counts.error,
            report.counts.total
        );
        assert_eq!(report.status, ValidationStatus::Error);
        assert_eq!(report.exit_code(), 1);

        let empty = build_report(
            ValidationLevel::Plan,
            RegistryProvenance::default(),
            Vec::new(),
        );
        assert_eq!(empty.status, ValidationStatus::Ok, "empty is not a failure");
        assert_eq!(empty.counts.total, 0);
        assert_eq!(empty.exit_code(), 0);
    }

    /// U24 + C7
    #[test]
    fn query_file_skips_blanks_and_comments() {
        let text = "-R/a.txt/-/to_text\n# a comment\n\n   \nto_text\n";
        let lines = split_query_lines(text);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], (1, "-R/a.txt/-/to_text".to_string()));
        assert_eq!(lines[1], (5, "to_text".to_string()), "true source line");
    }

    /// U26 — cwd changes the reported key and recipe prefix, never the raw query plan.
    #[test]
    fn cwd_changes_key_and_plan_prefix_only() -> Result<(), Error> {
        let cmr = registry_with("to_text", "root");
        let build = |cwd: &str| -> Result<ValidationResult, Error> {
            let mut list = RecipeList::new();
            list.add_recipe(recipe("-R/data/report.txt/-/to_text/out.txt"));
            apply_cwd(&mut list, cwd)?;
            Ok(validate_recipes(&list, ValidationLevel::Plan, &cmr).remove(0))
        };

        let reports = build("reports")?;
        let archive = build("archive")?;

        let reports_plan = reports.plan.expect("reports plan");
        let archive_plan = archive.plan.expect("archive plan");

        assert!(matches!(
            reports_plan.steps.first(),
            Some(crate::plan::Step::SetCwd(key)) if key.encode() == "reports"
        ));
        assert!(matches!(
            archive_plan.steps.first(),
            Some(crate::plan::Step::SetCwd(key)) if key.encode() == "archive"
        ));
        let reports_tail = serde_json::to_string(&reports_plan.steps[1..])
            .map_err(|error| Error::from_error(ErrorType::General, error))?;
        let archive_tail = serde_json::to_string(&archive_plan.steps[1..])
            .map_err(|error| Error::from_error(ErrorType::General, error))?;
        assert_eq!(
            reports_tail, archive_tail,
            "query-derived steps remain source-relative and independent of recipe cwd"
        );
        assert_eq!(reports_plan.query.encode(), archive_plan.query.encode());
        assert!(reports_plan.init_steps.iter().any(
            |step| matches!(step, crate::plan::Step::Info(message) if message == "Recipe set CWD to 'reports'")
        ));
        assert!(archive_plan.init_steps.iter().any(
            |step| matches!(step, crate::plan::Step::Info(message) if message == "Recipe set CWD to 'archive'")
        ));
        assert_ne!(
            reports.key, archive.key,
            "the keyed recipe output also moves"
        );
        Ok(())
    }

    /// T9 - the validator contract for an over-supplied query.
    ///
    /// The issue this design resolves anticipated `Warning` with exit 0. Because the
    /// resolution is an error rather than the proposed warning, an over-supplied query is
    /// `Error` and the CLI exits 1. `ValidationStatus::Warning` is untouched and keeps its
    /// meaning for the header-name case and the other `init_warning` sources.
    #[test]
    fn over_supplied_query_reports_error() {
        let cmr = registry_with("to_text", "");

        let ok = validate_query("to_text", 0, ValidationLevel::Plan, &cmr);
        assert_eq!(ok.status, ValidationStatus::Ok);

        let result = validate_query("to_text-extra", 0, ValidationLevel::Plan, &cmr);
        assert_eq!(result.status, ValidationStatus::Error);
        let error = result
            .error
            .expect("an over-supplied query reports an error");
        assert_eq!(error.error_type, ErrorType::TooManyParameters);
        assert!(
            !error.position.is_unknown(),
            "the excess parameter must be positioned so a caller can point at it"
        );
    }

    /// A permissive command - what `--command` declares - takes a `multiple` argument, so it
    /// must keep accepting any number of parameters. This is the variadic exemption in
    /// production use, not merely in test fixtures.
    #[test]
    fn permissive_command_still_accepts_any_arguments() {
        let cmr = permissive_registry("anything");

        for query in ["anything", "anything-a", "anything-a-b-c-d-e"] {
            let result = validate_query(query, 0, ValidationLevel::Plan, &cmr);
            assert_eq!(
                result.status,
                ValidationStatus::Ok,
                "permissive command must accept '{}'",
                query
            );
        }
    }
}
