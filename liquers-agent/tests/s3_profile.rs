//! AC-20: a collection mounted from an S3-compatible store behaves as a local one and survives a
//! restart of the service. The endpoint is `s3s-fs`, served in process over a loopback port.

use std::path::{Path, PathBuf};

use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ConnBuilder;
use liquers_agent::{build_server, AgentConfig, AgentEnvironment};
use liquers_core::error::Error;
use liquers_mcp::McpServer;
use s3s::auth::SimpleAuth;
use s3s::host::SingleDomain;
use s3s::service::S3ServiceBuilder;
use serde_json::{json, Map, Value};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

type Res = Result<(), Box<dyn std::error::Error>>;

const ACCESS_KEY: &str = "AKIAIOSFODNN7EXAMPLE";
const SECRET_KEY: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
const BUCKET: &str = "mem";

const SCHEMA: &str = r#"
readme: .overview.md
collections:
  - { name: profile, folder: user/memories, file: profile.md }
  - { name: preferences, folder: user/memories/preferences, id: { from: title, case: kebab } }
  - { name: entities, folder: user/memories/entities, id: { from: title, case: kebab } }
  - name: events
    folder: user/memories/events
    id: { from: date_title, case: kebab }
    fields: { date: { type: date, required: true } }
"#;

/// A temporary folder holding the local mount (`local/`) and the S3 endpoint's data (`s3/`).
struct Fixture {
    dir: PathBuf,
    endpoint: String,
    server: JoinHandle<()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn fixture() -> Result<Fixture, Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("liquers-agent-s3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let local = dir.join("local");
    std::fs::create_dir_all(local.join("_schema"))?;
    std::fs::write(local.join("_schema/memory.yaml"), SCHEMA)?;
    std::fs::write(
        local.join(".overview.md"),
        "---\ntitle: Agent memory\ndescription: What the agent remembers about its user\n---\n",
    )?;
    // s3s-fs serves each folder under its root as a bucket.
    std::fs::create_dir_all(dir.join("s3").join(BUCKET))?;

    let fs = s3s_fs::FileSystem::new(dir.join("s3")).map_err(|e| format!("s3s-fs: {e:?}"))?;
    let service = {
        let mut builder = S3ServiceBuilder::new(fs);
        builder.set_auth(SimpleAuth::from_single(ACCESS_KEY, SECRET_KEY));
        builder.set_host(SingleDomain::new("localhost")?);
        builder.build()
    };
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let connection = ConnBuilder::new(TokioExecutor::new())
                .serve_connection(TokioIo::new(socket), service.clone())
                .into_owned();
            tokio::spawn(async move {
                let _ = connection.await;
            });
        }
    });
    Ok(Fixture {
        dir,
        endpoint: format!("http://{address}"),
        server,
    })
}

/// The "openviking" profile of `phase2-architecture.md`: the `user` mount is S3.
fn config(f: &Fixture) -> Result<AgentConfig, Error> {
    AgentConfig::from_yaml(&format!(
        r#"
store:
  stores:
    - type: s3
      prefix: user
      config:
        bucket: {BUCKET}
        root: u/1
        region: us-west-2
        endpoint: "{endpoint}"
        access_key_id: {ACCESS_KEY}
        secret_access_key: "{SECRET_KEY}"
      metadata:
        storage: none
        on_read: "media_type/front_matter/markdown_tiers"
        on_write: "defaults/media_type-fill/front_matter/front_matter_sync"
    - type: filesystem
      prefix: ""
      config: {{ path: "{local}" }}
memory:
  root: ""
"#,
        endpoint = f.endpoint,
        local = f.dir.join("local").display(),
    ))
}

async fn server(f: &Fixture) -> Result<McpServer<AgentEnvironment>, Error> {
    build_server(&config(f)?).await
}

fn args(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// Every file the endpoint holds, relative to the bucket.
fn objects(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, base: &Path, found: &mut Vec<String>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, base, found);
                } else if let Ok(relative) = path.strip_prefix(base) {
                    found.push(relative.display().to_string());
                }
            }
        }
    }
    let mut found = Vec::new();
    walk(root, root, &mut found);
    found.sort();
    found
}

#[tokio::test]
async fn s3_mount_round_trip() -> Res {
    let f = fixture().await?;
    let today = "2026-10-19";
    {
        let server = server(&f).await?;
        let overview = server.call("memory_overview", Map::new()).await?;
        assert!(overview.text.starts_with("# Agent memory"), "{}", overview.text);

        let created = server
            .call(
                "memory_new",
                args(json!({
                    "collection": "events",
                    "title": "Tea tasting",
                    "description": "Went to a tea tasting",
                    "fields": {"date": today},
                    "format": "json",
                })),
            )
            .await?;
        assert!(created.text.contains("user/memories/events/2026-10-19-tea-tasting.md"), "{}", created.text);
        server
            .call(
                "memory_new",
                args(json!({
                    "collection": "preferences",
                    "title": "Tea",
                    "description": "Prefers green tea",
                    "body": "Green, never black.\n",
                })),
            )
            .await?;
        server
            .call(
                "memory_edit",
                args(json!({
                    "key": "user/memories/preferences/tea.md",
                    "old": "never black",
                    "new": "rarely black",
                })),
            )
            .await?;

        let listed = server
            .call("memory_list", args(json!({"path": "user", "depth": 0, "format": "json"})))
            .await?;
        assert!(listed.text.contains("user/memories/preferences/tea.md"), "{}", listed.text);
        assert!(listed.text.contains("2026-10-19-tea-tasting.md"), "{}", listed.text);
        let found = server
            .call("memory_search", args(json!({"text": "tea", "path": "user", "format": "json"})))
            .await?;
        assert!(found.text.contains("user/memories/preferences/tea.md"), "{}", found.text);
    }

    // The data lives on the endpoint, under the mount's root, and nothing sits beside it in the
    // local mount but the schema, the overview and the log.
    let stored = objects(&f.dir.join("s3").join(BUCKET));
    assert!(
        stored.iter().any(|o| o.ends_with("user/memories/preferences/tea.md")),
        "{stored:?}"
    );
    assert!(stored.iter().all(|o| o.starts_with("u/1/")), "{stored:?}");
    assert!(!f.dir.join("local/user").exists());

    // A restart: a new environment over the same endpoint reads what the first one wrote.
    let server = server(&f).await?;
    let read = server
        .call("memory_read", args(json!({"key": "user/memories/preferences/tea.md"})))
        .await?;
    assert!(read.text.contains("Green, rarely black."), "{}", read.text);
    let tier = server
        .call(
            "memory_read",
            args(json!({"key": "user/memories/events/2026-10-19-tea-tasting.md", "tier": "l0"})),
        )
        .await?;
    assert_eq!(tier.text.trim(), "Tea tasting");
    Ok(())
}
