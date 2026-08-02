//! `liquers-validate` — check that Liquers queries and recipes are well-formed.
//!
//! Intended for coding agents and developers writing examples, doc snippets and unit tests.
//! See `specs/query-validation/` for the design.
//!
//! **A green result means "here is exactly what your query means", not "your query is right".**
//! `-R/data/report.txt/to_text` and `-R/data/report.txt/-/to_text` both validate cleanly; the
//! first fetches a file *named* `to_text`. Compare `encoded` (or the plan's steps) against intent.

use std::io::Read;
use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};

use liquers_core::command_metadata::CommandMetadataRegistry;
use liquers_core::error::{Error, ErrorType};
use liquers_core::recipes::RecipeList;
use liquers_core::validate::{
    apply_cwd, build_report, cwd_conflict_error, from_json_or_yaml, split_query_lines,
    validate_query, validate_recipes, RegistryProvenance, ValidationLevel,
    ValidationRegistryBuilder, ValidationResult, ValidationStatus,
};

/// Environment variable consulted when no `--registry-file` is given.
const REGISTRY_ENV: &str = "LIQUERS_COMMAND_REGISTRY";
/// Committed registry, searched for by walking up from the working directory.
const REGISTRY_DEFAULT_PATH: &str = "specs/command_registry.yaml";

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Json,
    Yaml,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum Detail {
    /// Status, source, encoded, error and warnings — no Query, no Plan.
    Summary,
    /// Everything, including the serialized Query and Plan.
    Full,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum LevelArg {
    Parse,
    Plan,
}

impl From<LevelArg> for ValidationLevel {
    fn from(value: LevelArg) -> Self {
        match value {
            LevelArg::Parse => ValidationLevel::Parse,
            LevelArg::Plan => ValidationLevel::Plan,
        }
    }
}

/// Validate Liquers queries and recipes without evaluating them.
///
/// There are deliberately **no short flags**: Liquers resource queries begin with `-` (e.g.
/// `-R/data/x.csv`), so a short `-R` would swallow a query as an attached flag value. Use `--`
/// before a query as a habit: `liquers-validate -- '-R/data/x.csv'`.
#[derive(Parser, Debug)]
#[command(
    name = "liquers-validate",
    about = "Validate Liquers queries and recipes (parse, or parse and plan)",
    after_help = "A clean result tells you what your query MEANS, not that it is correct.\n\
                  '-R/a/b/to_text' and '-R/a/b/-/to_text' both validate; only the second treats\n\
                  to_text as an action. Compare the 'encoded' field against your intent.\n\n\
                  Queries begin with '-', so pass them after '--':\n  \
                  liquers-validate -- '-R/data/report.txt/-/to_text'"
)]
struct Cli {
    /// Queries to validate. Use `--` first, since queries start with `-`.
    #[arg(allow_hyphen_values = true)]
    queries: Vec<String>,

    /// Newline-separated queries from FILE, or `-` for stdin. Blank and `#` lines are skipped.
    #[arg(long, value_name = "FILE", conflicts_with_all = ["queries", "recipes"])]
    query_file: Option<String>,

    /// A RecipeList (recipes.yaml shape) from FILE, or `-` for stdin.
    #[arg(long, value_name = "FILE", conflicts_with = "queries")]
    recipes: Option<String>,

    /// Merge a serialized command registry (JSON or YAML). Repeatable, applied in order.
    #[arg(long, value_name = "FILE")]
    registry_file: Vec<PathBuf>,

    /// Declare a permissive command that accepts any arguments: `name`, `ns/name`,
    /// `realm/ns/name`. Repeatable.
    #[arg(long, value_name = "SPEC")]
    command: Vec<String>,

    /// Permit a merged command to replace one already defined.
    #[arg(long)]
    allow_overwrite: bool,

    /// Ignore the environment variable and the committed registry; use an empty registry.
    #[arg(long, conflicts_with = "registry_file")]
    no_registry: bool,

    /// How far to validate. Defaults to `plan` when a registry source is present, else `parse`.
    #[arg(long, value_enum)]
    level: Option<LevelArg>,

    /// Working directory for recipes, as `DefaultRecipeProvider` would supply it.
    #[arg(long, value_name = "KEY", requires = "recipes")]
    cwd: Option<String>,

    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    format: OutputFormat,

    #[arg(long, value_enum, default_value_t = Detail::Full)]
    detail: Detail,

    /// Suppress the human-readable diagnostic lines on stderr.
    #[arg(long)]
    quiet: bool,
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            // Exit 2 means the tool was invoked wrongly. stdout stays empty, so a caller that
            // always parses stdout can treat "empty" as "I got the invocation wrong".
            eprintln!("ERROR    {}", error);
            std::process::exit(2);
        }
    }
}

fn run() -> Result<i32, Error> {
    let cli = Cli::parse();

    let (registry, provenance) = assemble_registry(&cli)?;
    let level: ValidationLevel = match cli.level {
        Some(level) => level.into(),
        // Level 2 is meaningless without a registry: every action would be unregistered.
        None if provenance.command_count > 0 => ValidationLevel::Plan,
        None => ValidationLevel::Parse,
    };

    let results = if let Some(source) = &cli.recipes {
        validate_recipe_input(source, cli.cwd.as_deref(), level, &registry)?
    } else if let Some(source) = &cli.query_file {
        let text = read_input(source)?;
        split_query_lines(&text)
            .into_iter()
            .enumerate()
            .map(|(index, (line, query))| {
                validate_query(&query, index, level, &registry).with_line(line)
            })
            .collect()
    } else if !cli.queries.is_empty() {
        cli.queries
            .iter()
            .enumerate()
            .map(|(index, query)| validate_query(query, index, level, &registry))
            .collect()
    } else {
        return Err(Error::general_error(
            "No input. Pass a query after `--`, or use --query-file or --recipes.".to_string(),
        ));
    };

    let mut report = build_report(level, provenance, results);

    if cli.detail == Detail::Summary {
        for result in &mut report.results {
            result.query = None;
            result.plan = None;
        }
    }

    let serialized = match cli.format {
        OutputFormat::Json => report.to_json()?,
        OutputFormat::Yaml => report.to_yaml()?,
    };
    // stdout carries the envelope and nothing else.
    println!("{}", serialized);

    if !cli.quiet {
        for line in report.diagnostic_lines() {
            eprintln!("{}", line);
        }
    }

    Ok(report.exit_code())
}

fn assemble_registry(cli: &Cli) -> Result<(CommandMetadataRegistry, RegistryProvenance), Error> {
    let mut builder = ValidationRegistryBuilder::new().with_overwrite_allowed(cli.allow_overwrite);

    for path in resolve_registry_sources(cli) {
        let text = read_file(&path)?;
        builder.merge_str(&path.display().to_string(), &text)?;
    }
    for spec in &cli.command {
        builder.add_permissive_command(spec)?;
    }

    Ok(builder.build())
}

/// Explicit flags win; then the environment; then the committed registry; then nothing.
fn resolve_registry_sources(cli: &Cli) -> Vec<PathBuf> {
    if cli.no_registry {
        return Vec::new();
    }
    if !cli.registry_file.is_empty() {
        return cli.registry_file.clone();
    }
    if let Ok(path) = std::env::var(REGISTRY_ENV) {
        if !path.is_empty() {
            return vec![PathBuf::from(path)];
        }
    }
    find_committed_registry().into_iter().collect()
}

/// Walk up from the working directory looking for the committed registry, so the tool works
/// from anywhere inside a checkout without setup.
fn find_committed_registry() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    for directory in cwd.ancestors() {
        let path = directory.join(REGISTRY_DEFAULT_PATH);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn validate_recipe_input(
    source: &str,
    cwd: Option<&str>,
    level: ValidationLevel,
    registry: &CommandMetadataRegistry,
) -> Result<Vec<ValidationResult>, Error> {
    let text = read_input(source)?;
    let mut recipes: RecipeList = from_json_or_yaml(source, &text)?;

    // A recipe that declares its own cwd is a genuine finding — `DefaultRecipeProvider` refuses
    // it too — so it is reported per recipe rather than aborting the whole batch.
    let conflicts = match cwd {
        Some(cwd) => apply_cwd(&mut recipes, cwd)?,
        None => Vec::new(),
    };

    let mut results = validate_recipes(&recipes, level, registry);
    for index in conflicts {
        if let Some(result) = results.get_mut(index) {
            result.status = ValidationStatus::Error;
            result.error = Some(cwd_conflict_error());
        }
    }
    Ok(results)
}

fn read_input(source: &str) -> Result<String, Error> {
    if source == "-" {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| Error::from_error(ErrorType::General, e))?;
        Ok(text)
    } else {
        read_file(Path::new(source))
    }
}

fn read_file(path: &Path) -> Result<String, Error> {
    std::fs::read_to_string(path)
        .map_err(|e| Error::general_error(format!("Cannot read '{}': {}", path.display(), e)))
}
