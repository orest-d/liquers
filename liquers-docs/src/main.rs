//! `liquers-docs`: builds and checks the user guides under `docs/guides/`.
//!
//! Stdout carries only the summary or the requested report; findings go to stderr.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use liquers_docs::build::{build, check, report};
use liquers_docs::markers::BuildMode;
use liquers_docs::{Finding, Severity};

#[derive(Parser)]
#[command(
    name = "liquers-docs",
    about = "Build and check the Liquers user guides"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Json,
}

#[derive(Subcommand)]
enum Command {
    /// Expand and check every chapter, and write the mdBook and Pandoc sources.
    Build {
        /// The repository root.
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// Where to write the build tree.
        #[arg(long, default_value = "target/docs-build")]
        out: PathBuf,
        /// A release build: only `ready` chapters, writer notes removed.
        #[arg(long)]
        release: bool,
    },
    /// Report every finding; exit 1 on an error.
    Check {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// Apply the release rules (open markers in `ready` chapters are errors).
        #[arg(long)]
        release: bool,
        #[arg(long, value_enum, default_value = "text")]
        format: Format,
    },
    /// Every chapter's status, sources and open markers.
    Report {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long, value_enum, default_value = "text")]
        format: Format,
    },
}

fn mode(release: bool) -> BuildMode {
    if release {
        BuildMode::Release
    } else {
        BuildMode::Draft
    }
}

/// Prints findings to stderr and returns the exit code: 1 when any is an error.
fn finish(findings: &[Finding]) -> ExitCode {
    for finding in findings {
        eprintln!("{finding}");
    }
    let errors = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count();
    let warnings = findings.len() - errors;
    println!("{errors} errors, {warnings} warnings");
    if errors > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Build { root, out, release } => {
            let out = if out.is_absolute() {
                out
            } else {
                root.join(out)
            };
            build(&root, &out, mode(release)).await.map(|f| finish(&f))
        }
        Command::Check {
            root,
            release,
            format,
        } => check(&root, mode(release))
            .await
            .map(|findings| match format {
                Format::Text => finish(&findings),
                Format::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&findings).unwrap_or_default()
                    );
                    if findings.iter().any(|f| f.severity == Severity::Error) {
                        ExitCode::FAILURE
                    } else {
                        ExitCode::SUCCESS
                    }
                }
            }),
        Command::Report { root, format } => report(&root).await.map(|rows| {
            match format {
                Format::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&rows).unwrap_or_default()
                    )
                }
                Format::Text => {
                    for row in &rows {
                        let markers: Vec<String> = row
                            .markers
                            .iter()
                            .map(|(m, n)| format!("{m} {n}"))
                            .collect();
                        println!(
                            "{}/{}\t{}\t{}\t{}",
                            row.guide,
                            row.file,
                            row.status.name(),
                            row.reviewed.as_deref().unwrap_or("-"),
                            markers.join(", ")
                        );
                    }
                }
            }
            ExitCode::SUCCESS
        }),
    };
    match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
    }
}
