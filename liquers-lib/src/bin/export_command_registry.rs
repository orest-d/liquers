//! `export-command-registry` — dump liquers-lib's registered command metadata.
//!
//! The output feeds `liquers-validate`, so query validation never has to link liquers-lib and
//! its heavy optional dependencies. See `specs/design/query-validation/`.
//!
//! The committed copy lives at `specs/command_registry.yaml`; regenerate it with
//! `--format yaml -o specs/command_registry.yaml` whenever a command signature changes.

use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};

// The `register_*_commands!` macros expand to `register_command!` invocations that name these
// types unqualified, so they must be in scope here even where this file never mentions them.
#[allow(unused_imports)]
use liquers_core::{
    command_metadata::CommandMetadataRegistry,
    command_metadata::{ArgumentType, CommandDefinition, CommandParameterValue},
    context::{Context, Environment},
    error::{Error, ErrorType},
    state::State,
    value::ValueInterface,
};
#[allow(unused_imports)]
use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::payload::SimpleUIPayload;
#[allow(unused_imports)]
use liquers_lib::value::{simple::SimpleValue, Value};

/// Marks the hand-maintained part of the generated header.
const CHANGELOG_BEGIN: &str = "# CHANGELOG-BEGIN";
const CHANGELOG_END: &str = "# CHANGELOG-END";

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Json,
    Yaml,
}

/// Command groups this binary can export.
///
/// `core` and `lui` are always present; the rest exist only when their cargo feature is on,
/// which is why `--list-groups` exists — it reports what *this* binary can actually offer.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum Group {
    Core,
    Lui,
    Egui,
    Image,
    Polars,
}

impl Group {
    fn name(self) -> &'static str {
        match self {
            Group::Core => "core",
            Group::Lui => "lui",
            Group::Egui => "egui",
            Group::Image => "image",
            Group::Polars => "polars",
        }
    }

    /// Whether this binary was compiled with the group's cargo feature.
    fn is_compiled_in(self) -> bool {
        match self {
            Group::Core | Group::Lui => true,
            Group::Egui => cfg!(feature = "egui"),
            Group::Image => cfg!(feature = "image-support"),
            Group::Polars => cfg!(feature = "polars"),
        }
    }

    fn all() -> [Group; 5] {
        [
            Group::Core,
            Group::Lui,
            Group::Egui,
            Group::Image,
            Group::Polars,
        ]
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "export-command-registry",
    about = "Export liquers-lib command metadata for liquers-validate"
)]
struct Cli {
    /// Write to FILE instead of stdout.
    #[arg(long, short = 'o', value_name = "FILE")]
    output: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    format: OutputFormat,

    /// Groups to include. Defaults to every group compiled into this binary.
    #[arg(long, value_enum, value_delimiter = ',')]
    groups: Vec<Group>,

    /// Keep only commands in these namespaces (e.g. `pl,lui`). Defaults to all.
    #[arg(long, value_delimiter = ',', value_name = "NS")]
    namespaces: Vec<String>,

    /// Print the groups this binary was built with, then exit.
    #[arg(long)]
    list_groups: bool,
}

// A tokio runtime is required but never awaited: `DefaultEnvironment::new()` builds a
// `DefaultAssetManager`, which calls `tokio::spawn`, and panics at runtime without one.
// `current_thread` rather than the default multi-thread flavor, because liquers-lib enables
// tokio's `rt` but not `rt-multi-thread` — bare `#[tokio::main]` would not compile.
#[tokio::main(flavor = "current_thread")]
async fn main() {
    match run() {
        Ok(()) => {}
        Err(error) => {
            eprintln!("ERROR    {}", error);
            std::process::exit(2);
        }
    }
}

fn run() -> Result<(), Error> {
    let cli = Cli::parse();

    if cli.list_groups {
        for group in Group::all() {
            let state = if group.is_compiled_in() {
                "available"
            } else {
                "not compiled in"
            };
            // Group listing is this invocation's intended output, so it belongs on stdout.
            println!("{:<8} {}", group.name(), state);
        }
        return Ok(());
    }

    let selected = resolve_groups(&cli)?;
    let mut registry = build_registry(&selected)?;

    if !cli.namespaces.is_empty() {
        registry
            .commands
            .retain(|command| cli.namespaces.contains(&command.namespace));
    }

    let body = match cli.format {
        OutputFormat::Json => serde_json::to_string_pretty(&registry)
            .map_err(|e| Error::from_error(ErrorType::General, e))?,
        OutputFormat::Yaml => serde_yaml::to_string(&registry)
            .map_err(|e| Error::from_error(ErrorType::General, e))?,
    };

    match &cli.output {
        None => println!("{}", body),
        Some(path) => {
            let text = match cli.format {
                // Only YAML can carry the provenance header, which is why the committed
                // artifact is YAML.
                OutputFormat::Yaml => {
                    format!(
                        "{}{}",
                        header(&selected, registry.commands.len(), path),
                        body
                    )
                }
                OutputFormat::Json => body,
            };
            std::fs::write(path, text).map_err(|e| {
                Error::general_error(format!("Cannot write '{}': {}", path.display(), e))
            })?;
            eprintln!(
                "Wrote {} commands to {}",
                registry.commands.len(),
                path.display()
            );
        }
    }

    Ok(())
}

fn resolve_groups(cli: &Cli) -> Result<Vec<Group>, Error> {
    if cli.groups.is_empty() {
        return Ok(Group::all()
            .into_iter()
            .filter(|group| group.is_compiled_in())
            .collect());
    }

    for group in &cli.groups {
        if !group.is_compiled_in() {
            let available: Vec<&str> = Group::all()
                .into_iter()
                .filter(|g| g.is_compiled_in())
                .map(|g| g.name())
                .collect();
            return Err(Error::general_error(format!(
                "Group '{}' is not compiled into this binary. Available: {}. \
                 Rebuild with the matching cargo feature, or run --list-groups.",
                group.name(),
                available.join(", ")
            )));
        }
    }
    Ok(cli.groups.clone())
}

/// Build the registry by invoking each group's macro under its own `cfg`.
///
/// Deliberately not `register_all_commands!`: that macro expands to the egui and polars macros
/// unconditionally, and those macros do not *exist* when their features are off, so it only
/// compiles with every feature enabled.
fn build_registry(groups: &[Group]) -> Result<CommandMetadataRegistry, Error> {
    #[allow(unused_mut)]
    let mut env = DefaultEnvironment::<Value, SimpleUIPayload>::new();
    #[allow(unused_variables)]
    type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

    {
        #[allow(unused_variables)]
        let cr = env.get_mut_command_registry();

        for group in groups {
            match group {
                Group::Core => {
                    liquers_lib::register_core_commands!(cr)?;
                }
                Group::Lui => {
                    liquers_lib::register_lui_commands!(cr)?;
                }
                Group::Egui => {
                    #[cfg(feature = "egui")]
                    liquers_lib::register_egui_commands!(cr)?;
                }
                Group::Image => {
                    #[cfg(feature = "image-support")]
                    liquers_lib::register_image_commands!(cr)?;
                }
                Group::Polars => {
                    #[cfg(feature = "polars")]
                    liquers_lib::register_polars_commands!(cr)?;
                }
            }
        }
    }

    Ok(env.get_command_metadata_registry().clone())
}

/// The generated provenance header.
///
/// `serde_yaml` does not round-trip comments, so the header is written by hand here. When the
/// target already exists, the hand-maintained changelog between the markers is carried over
/// verbatim — that is the whole mechanism keeping a human-edited history across regeneration.
fn header(groups: &[Group], command_count: usize, path: &Path) -> String {
    let group_names: Vec<&str> = groups.iter().map(|g| g.name()).collect();
    let features: Vec<&str> = Group::all()
        .into_iter()
        .filter(|g| !matches!(g, Group::Core | Group::Lui) && g.is_compiled_in())
        .map(|g| g.name())
        .collect();

    let changelog = existing_changelog(path).unwrap_or_else(|| {
        format!("{CHANGELOG_BEGIN}\n# (add a dated line here when regenerating)\n{CHANGELOG_END}\n")
    });

    format!(
        "# Liquers command registry — GENERATED, do not edit by hand.\n\
         # Regenerate: cargo run -p liquers-lib --features cli --bin export-command-registry -- \\\n\
         #               --format yaml -o {}\n\
         # Origin:   groups={}  features={}\n\
         # Commands: {}\n\
         {}",
        path.display(),
        group_names.join(","),
        if features.is_empty() {
            "none".to_string()
        } else {
            features.join(",")
        },
        command_count,
        changelog
    )
}

/// Lift the `CHANGELOG-BEGIN … CHANGELOG-END` block out of an existing file.
fn existing_changelog(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let start = text.find(CHANGELOG_BEGIN)?;
    let end = text[start..].find(CHANGELOG_END)? + start + CHANGELOG_END.len();
    Some(format!("{}\n", &text[start..end]))
}
