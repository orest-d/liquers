//! The built-in memory tool set, in process and through the `liquers-agent mcp` binary
//! (`specs/design/agent-memory-mvp/phase3-examples.md`, "mcp_tools.rs").

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use liquers_agent::{build_server, AgentConfig, AgentEnvironment, QUERY_GUIDE};
use liquers_core::error::Error;
use liquers_mcp::McpServer;
use serde_json::{json, Map, Value};

type Res = Result<(), Box<dyn std::error::Error>>;

/// A copy of `tests/fixtures/memory` under `<tmp>/mem`, mounted at the store root with the memory
/// rooted at `mem`, so every key goes through the tool set's `cwd`.
struct Fixture {
    dir: PathBuf,
    config: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn fixture(name: &str) -> Result<Fixture, Box<dyn std::error::Error>> {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "liquers-agent-{name}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/memory");
    copy_dir(&source, &dir.join("mem"))?;
    let config = dir.join("liquers-agent.yaml");
    std::fs::write(
        &config,
        format!(
            r#"
store:
  stores:
    - type: filesystem
      prefix: ""
      config: {{ path: "{dir}" }}
      metadata:
        storage: none
        on_read: "media_type/file_facts/content_hash-on_change/front_matter/markdown_tiers"
        on_write: "defaults/media_type-fill/file_facts/front_matter/front_matter_sync"
memory:
  root: mem
"#,
            dir = dir.display()
        ),
    )?;
    Ok(Fixture { dir, config })
}

async fn server(f: &Fixture) -> Result<McpServer<AgentEnvironment>, Error> {
    build_server(&AgentConfig::load(&f.config)?).await
}

fn args(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

fn schema_properties(tool: &rmcp::model::Tool) -> Vec<String> {
    tool.input_schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|p| p.keys().cloned().collect())
        .unwrap_or_default()
}

#[tokio::test]
async fn memory_tools_match_the_tool_table() -> Res {
    use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
    use rmcp::ServiceExt;

    let f = fixture("table")?;
    let config = f.config.clone();
    let transport = TokioChildProcess::new(
        tokio::process::Command::new(env!("CARGO_BIN_EXE_liquers-agent")).configure(|cmd| {
            cmd.arg("--config").arg(&config).arg("mcp");
        }),
    )?;
    let client = ().serve(transport).await?;

    let info = client.peer_info().map(|i| i.instructions.clone().unwrap_or_default());
    let instructions = info.unwrap_or_default();
    assert!(instructions.contains("Fixture memory"), "{instructions}");
    assert!(instructions.contains("memory_query"), "{instructions}");

    let tools = client.list_all_tools().await?;
    let mut names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "memory_check", "memory_describe", "memory_edit", "memory_list", "memory_new",
            "memory_overview", "memory_query", "memory_read", "memory_search", "memory_write",
        ]
    );
    let tool = |name: &str| tools.iter().find(|t| t.name == name).cloned();
    let expected: [(&str, &[&str]); 9] = [
        ("memory_overview", &["format", "log_lines"]),
        ("memory_describe", &["collection", "format"]),
        ("memory_list", &["depth", "format", "include_superseded", "limit", "path"]),
        ("memory_search", &["filter", "format", "include_superseded", "limit", "path", "text", "unit"]),
        ("memory_read", &["format", "key", "lines", "section", "tier"]),
        ("memory_new", &["body", "collection", "description", "fields", "force", "format", "reason", "title"]),
        ("memory_write", &["body", "description", "expected_version", "fields", "format", "key", "reason", "supersedes", "title"]),
        ("memory_edit", &["expected_version", "field", "format", "key", "new", "old", "reason", "row", "section", "value"]),
        ("memory_check", &["format", "path"]),
    ];
    for (name, properties) in expected {
        let found = tool(name).ok_or("missing tool")?;
        assert_eq!(schema_properties(&found), properties.to_vec(), "{name}");
        let read_only = found.annotations.as_ref().and_then(|a| a.read_only_hint);
        let writes = ["memory_new", "memory_write", "memory_edit"].contains(&name);
        assert_eq!(read_only, Some(!writes), "{name}");
    }
    let read = tool("memory_read").ok_or("missing tool")?;
    assert_eq!(read.input_schema.get("required"), Some(&json!(["key"])));

    let mut arguments = Map::new();
    arguments.insert("text".to_string(), json!("expiration"));
    let result = client
        .call_tool(rmcp::model::CallToolRequestParams::new("memory_search").with_arguments(arguments))
        .await?;
    assert_ne!(result.is_error, Some(true));
    let text = result.content[0].as_text().map(|t| t.text.clone()).unwrap_or_default();
    assert!(text.contains("CACHE-SERVES-STALE"), "{text}");
    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn keys_resolve_under_the_memory_root() -> Res {
    let f = fixture("root")?;
    let server = server(&f).await?;
    let read = server
        .call("memory_read", args(json!({"key": "notes/idea.md", "tier": "l1"})))
        .await?;
    assert_eq!(read.text.trim(), "Idea\n\nCheck expiration policy on every read.");
    assert!(read.notes.iter().any(|n| n.contains("version")), "{:?}", read.notes);
    let listed = server.call("memory_list", args(json!({"path": "issues", "format": "json"}))).await?;
    assert!(listed.text.contains("issues/WRITES-RACE.md"), "{}", listed.text);
    Ok(())
}

#[tokio::test]
async fn query_tool_refuses_a_write_command() -> Res {
    let f = fixture("refuse")?;
    let server = server(&f).await?;
    let refused = server
        .call(
            "memory_query",
            args(json!({"query": "-R-key/./notes/idea.md/-/ns-mem/edit-x-y/edit.json"})),
        )
        .await
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(refused.contains("-mem-edit is not allowed"), "{refused}");
    let original = std::fs::read_to_string(f.dir.join("mem/notes/idea.md"))?;
    assert!(original.contains("Check expiration policy"));
    Ok(())
}

#[tokio::test]
async fn allow_list_namespaces_contain_no_writing_command() -> Res {
    let f = fixture("allow")?;
    let server = server(&f).await?;
    let cmr = server.envref().get_command_metadata_registry();
    let verbs = [
        "write", "edit", "new", "save", "store", "set", "remove", "delete", "put", "upload",
        "rename", "move",
    ];
    let allow = vec!["-rec-*".to_string(), "-search-*".to_string()];
    let mut checked = 0;
    for command in &cmr.commands {
        let shown = command.key().to_string();
        if !allow.iter().any(|p| shown.starts_with(p.trim_end_matches('*'))) {
            continue;
        }
        checked += 1;
        for verb in verbs {
            assert!(
                !command.name.split('_').any(|word| word == verb),
                "{shown} looks like a writing command"
            );
        }
    }
    assert!(checked > 10, "only {checked} commands matched the allow-list");
    Ok(())
}

#[tokio::test]
async fn query_guide_queries_validate() -> Res {
    let f = fixture("guide")?;
    let server = server(&f).await?;
    let mut in_block = false;
    let mut queries = Vec::new();
    for line in QUERY_GUIDE.lines() {
        if line.starts_with("```") {
            in_block = !in_block;
        } else if in_block && !line.trim().is_empty() {
            queries.push(line.trim().to_string());
        }
    }
    assert!(queries.len() >= 5);
    for query in queries {
        server
            .plan("memory_query", &args(json!({ "query": query })))
            .map_err(|e| format!("{query}: {e}"))?;
    }
    Ok(())
}

#[tokio::test]
async fn list_composes_with_rec_and_formats() -> Res {
    let f = fixture("compose")?;
    let server = server(&f).await?;
    let output = server
        .call(
            "memory_query",
            args(json!({
                "query": "-R-key/./-/ns-mem/list-0-false-1000/ns-rec/eq-status-draft/ns-rec/sort-l0/drafts.csv"
            })),
        )
        .await?;
    assert_eq!(output.format, "csv");
    let mut lines = output.text.lines();
    let header = lines.next().unwrap_or_default();
    assert!(header.starts_with("key,l0,l1"), "{header}");
    let rows: Vec<&str> = lines.collect();
    assert_eq!(rows.len(), 1, "{}", output.text);
    assert!(rows[0].contains("CACHE-SERVES-STALE"));
    assert!(!output.text.contains("shown "), "notes stay out of the CSV");
    Ok(())
}
