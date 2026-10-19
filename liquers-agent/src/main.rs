//! `liquers-agent`: the memory over MCP (`mcp`) or from the command line.
//!
//! Every subcommand calls a tool of the same server as MCP does. Results go to stdout as JSON (or
//! YAML with `--format yaml`); notes and errors go to stderr.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use liquers_agent::{build_server, AgentConfig};
use liquers_core::error::Error;
use liquers_mcp::ToolOutput;
use serde_json::{json, Map, Value};

#[derive(Parser)]
#[command(name = "liquers-agent", about = "A file-based agent memory over MCP and the command line")]
struct Cli {
    /// The configuration file: the Liquers environment plus a `memory` section.
    #[arg(long, env = "LIQUERS_AGENT_CONFIG", default_value = "liquers-agent.yaml")]
    config: PathBuf,
    /// Output format of the CLI subcommands.
    #[arg(long, value_enum, default_value_t = Format::Json)]
    format: Format,
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Json,
    Yaml,
}

#[derive(Subcommand)]
enum Command {
    /// Serve the tools over MCP on standard input and output.
    Mcp,
    /// Check the memories under a folder; exits 1 when an error is found.
    Check { path: Option<String> },
    /// Search the memories.
    Search {
        text: String,
        /// Folder to search, relative to the memory root.
        #[arg(long = "in")]
        folder: Option<String>,
        #[arg(long)]
        limit: Option<i64>,
    },
    /// Read a document, or its tier or section.
    Read {
        key: String,
        #[arg(long)]
        tier: Option<String>,
        #[arg(long)]
        section: Option<String>,
    },
    /// Evaluate a read-only query; its filename picks the format.
    Query {
        /// Liquers queries begin with `-` (`-R/...`), so hyphen values are accepted here.
        #[arg(allow_hyphen_values = true)]
        query: String,
    },
}

fn arguments(pairs: Vec<(&str, Option<Value>)>) -> Map<String, Value> {
    pairs
        .into_iter()
        .filter_map(|(name, value)| value.map(|v| (name.to_string(), v)))
        .collect()
}

fn print(output: &ToolOutput, format: Format, raw: bool) -> Result<(), Error> {
    for note in &output.notes {
        eprintln!("{note}");
    }
    if raw {
        println!("{}", output.text);
        return Ok(());
    }
    match format {
        Format::Json => println!("{}", output.text),
        Format::Yaml => {
            let value: Value = serde_json::from_str(&output.text)
                .map_err(|e| Error::general_error(format!("The result is not JSON: {e}")))?;
            let text = serde_yaml::to_string(&value)
                .map_err(|e| Error::general_error(format!("Cannot write YAML: {e}")))?;
            print!("{text}");
        }
    }
    Ok(())
}

fn has_errors(output: &ToolOutput) -> bool {
    match serde_json::from_str::<Value>(&output.text) {
        Ok(Value::Array(rows)) => rows
            .iter()
            .any(|row| row.get("severity").and_then(Value::as_str) == Some("error")),
        Ok(_) | Err(_) => false,
    }
}

async fn run(cli: Cli) -> Result<ExitCode, Error> {
    let config = AgentConfig::load(&cli.config)?;
    let server = build_server(&config).await?;
    let json = Some(json!("json"));
    let (tool, args) = match cli.command {
        Command::Mcp => {
            liquers_mcp::serve_stdio(server).await?;
            return Ok(ExitCode::SUCCESS);
        }
        Command::Check { path } => (
            "memory_check",
            arguments(vec![("path", path.map(Value::String)), ("format", json)]),
        ),
        Command::Search {
            text,
            folder,
            limit,
        } => (
            "memory_search",
            arguments(vec![
                ("text", Some(Value::String(text))),
                ("path", folder.map(Value::String)),
                ("limit", limit.map(|n| json!(n))),
                ("format", json),
            ]),
        ),
        Command::Read { key, tier, section } => (
            "memory_read",
            arguments(vec![
                ("key", Some(Value::String(key))),
                ("tier", tier.map(Value::String)),
                ("section", section.map(Value::String)),
                ("format", json),
            ]),
        ),
        Command::Query { query } => {
            let output = server
                .call("memory_query", arguments(vec![("query", Some(Value::String(query)))]))
                .await?;
            print(&output, cli.format, true)?;
            return Ok(ExitCode::SUCCESS);
        }
    };
    let output = server.call(tool, args).await?;
    print(&output, cli.format, false)?;
    if tool == "memory_check" && has_errors(&output) {
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(1)
        }
    }
}
