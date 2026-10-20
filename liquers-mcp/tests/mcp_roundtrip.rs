//! The engine end to end: tool sets over a small core environment, called directly and over an
//! in-process MCP connection.

use liquers_core::{
    context::{Context, EnvRef, Environment, SimpleEnvironment},
    error::Error,
    metadata::{Metadata, MetadataRecord, Status},
    parse::parse_key,
    query::Key,
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};
use liquers_macro::register_command;
use liquers_mcp::{
    decl::{KeyArgumentDecl, QueryToolDecl, ResourceDecl, ToolDecl, ToolSetDecl},
    check_allowed, McpServer, ToolRegistry,
};
use rmcp::{
    model::{CallToolRequestParams, ReadResourceRequestParams},
    ServiceExt,
};
use serde_json::{json, Map};

type CommandEnvironment = SimpleEnvironment<Value>;

fn hello(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("hello"))
}

fn upper(state: &State<Value>, context: Context<CommandEnvironment>) -> Result<Value, Error> {
    context.info("upper ran")?;
    Ok(Value::from(state.try_into_string()?.to_uppercase()))
}

fn greet(state: &State<Value>, greeting: String) -> Result<Value, Error> {
    Ok(Value::from(format!("{greeting}, {}!", state.try_into_string()?)))
}

fn located(state: &State<Value>, root: Key) -> Result<Value, Error> {
    Ok(Value::from(format!(
        "{} under {}",
        state.try_into_string()?,
        root.encode()
    )))
}

fn wrap(state: &State<Value>, other: String) -> Result<Value, Error> {
    Ok(Value::from(format!("{}+{other}", state.try_into_string()?)))
}

fn fail(_state: &State<Value>) -> Result<Value, Error> {
    Err(Error::general_error("this command always fails".to_string()))
}

#[allow(clippy::panic)]
fn boom(_state: &State<Value>) -> Result<Value, Error> {
    panic!("boom")
}

fn source_metadata(key: &Key) -> Metadata {
    let mut record = MetadataRecord::new();
    record
        .with_key(key.clone())
        .with_type_identifier("Text".to_owned())
        .with_status(Status::Source);
    Metadata::MetadataRecord(record)
}

async fn environment() -> Result<EnvRef<CommandEnvironment>, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    let key = parse_key("root/notes/a.txt")?;
    store.set(&key, b"note a", &source_metadata(&key)).await?;
    let mut env = CommandEnvironment::new();
    env.with_async_store(Box::new(store));
    let cr = &mut env.command_registry;
    register_command!(cr, fn hello(state) -> result)?;
    register_command!(cr, fn upper(state, context) -> result)?;
    register_command!(cr, fn greet(state, greeting: String = "Hello") -> result)?;
    register_command!(cr, fn located(state, root: Key = query "-R-key/.") -> result)?;
    register_command!(cr, fn wrap(state, other: String = query "hello/upper") -> result)?;
    register_command!(cr, fn fail(state) -> result)?;
    register_command!(cr, fn boom(state) -> result)?;
    Ok(env.to_ref())
}

fn tool(name: &str, query: &str) -> ToolDecl {
    ToolDecl {
        name: name.to_string(),
        query: query.to_string(),
        key: None,
        description: None,
        fixed: Default::default(),
        read_only: true,
    }
}

fn key_argument(name: &str, required: bool) -> Option<KeyArgumentDecl> {
    Some(KeyArgumentDecl {
        name: name.to_string(),
        description: None,
        required,
    })
}

fn tool_set() -> ToolSetDecl {
    ToolSetDecl {
        prefix: "t_".to_string(),
        cwd: Some("root".to_string()),
        instructions: "Test tools.".to_string(),
        tools: vec![
            ToolDecl {
                key: key_argument("key", true),
                ..tool("read", "-R/./-/upper/read.txt")
            },
            tool("greet", "-R/./notes/a.txt/-/greet/greet.txt"),
            ToolDecl {
                key: key_argument("path", false),
                ..tool("where", "-R/./notes/a.txt/-/located/where.txt")
            },
            tool("fail", "-R/./notes/a.txt/-/fail/fail.txt"),
            tool("boom", "-R/./notes/a.txt/-/boom/boom.txt"),
        ],
        query_tool: Some(QueryToolDecl {
            name: "query".to_string(),
            allow: vec!["--hello".to_string(), "--upper".to_string()],
            description: None,
        }),
        resources: vec![
            ResourceDecl::Tool {
                uri_template: "liquers://t/{+key}".to_string(),
                tool: "read".to_string(),
                argument: "key".to_string(),
            },
            ResourceDecl::Text {
                uri: "liquers://t/guide".to_string(),
                name: "guide".to_string(),
                description: "A guide".to_string(),
                text: "Guide text".to_string(),
            },
            ResourceDecl::Commands {
                uri_template: "liquers://commands/{namespace}".to_string(),
                namespaces: vec!["".to_string()],
            },
        ],
    }
}

async fn server() -> Result<McpServer<CommandEnvironment>, Error> {
    let envref = environment().await?;
    let registry = ToolRegistry::build(vec![tool_set()], envref.get_command_metadata_registry())?;
    Ok(McpServer::new(envref, registry))
}

fn args(value: serde_json::Value) -> Map<String, serde_json::Value> {
    match value {
        serde_json::Value::Object(map) => map,
        _ => Map::new(),
    }
}

#[tokio::test]
async fn key_argument_reads_under_the_root() -> Result<(), Error> {
    let server = server().await?;
    let output = server.call("t_read", args(json!({"key": "notes/a.txt"}))).await?;
    assert_eq!(output.text, "NOTE A");
    assert_eq!(output.notes, vec!["upper ran".to_string()]);
    Ok(())
}

#[tokio::test]
async fn format_argument_changes_the_filename() -> Result<(), Error> {
    let server = server().await?;
    let plan = server.plan("t_greet", &args(json!({"format": "json"})))?;
    let filename = plan.steps.iter().find_map(|step| match step {
        liquers_core::plan::Step::Filename(name) => Some(name.name.clone()),
        _ => None,
    });
    assert_eq!(filename.as_deref(), Some("greet.json"));
    let output = server.call("t_greet", args(json!({"format": "json"}))).await?;
    assert_eq!(output.format, "json");
    assert_eq!(output.structured(), Some(json!("Hello, note a!")));
    Ok(())
}

#[tokio::test]
async fn overrides_reach_the_command() -> Result<(), Error> {
    let server = server().await?;
    let output = server.call("t_greet", args(json!({"greeting": "Hi"}))).await?;
    assert_eq!(output.text, "Hi, note a!");
    Ok(())
}

#[tokio::test]
async fn hidden_argument_is_refused() -> Result<(), Error> {
    let server = server().await?;
    let schema = &server.registry().mcp_tools()[0];
    assert!(schema.name.starts_with("t_"));
    let error = server
        .call("t_where", args(json!({"root": "elsewhere"})))
        .await
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(error.contains("cannot be set"), "{error}");
    let output = server.call("t_where", Map::new()).await?;
    assert_eq!(output.text, "note a under root");
    Ok(())
}

#[tokio::test]
async fn required_key_is_required() -> Result<(), Error> {
    let server = server().await?;
    assert!(server.call("t_read", Map::new()).await.is_err());
    assert!(server
        .call("t_read", args(json!({"key": "../outside.txt"})))
        .await
        .is_err());
    Ok(())
}

#[tokio::test]
async fn build_refuses_multi_action_tool_queries() -> Result<(), Error> {
    let envref = environment().await?;
    let cmr = envref.get_command_metadata_registry();
    let mut set = tool_set();
    set.tools = vec![tool("two", "hello/upper/two.txt")];
    let error = ToolRegistry::build(vec![set], cmr).err().map(|e| e.to_string());
    assert!(error.unwrap_or_default().contains("exactly one"));

    let mut set = tool_set();
    set.tools.push(tool("read", "hello/greet/again.txt"));
    assert!(ToolRegistry::build(vec![set], cmr).is_err(), "duplicate name");

    let mut set = tool_set();
    set.tools = vec![ToolDecl {
        key: key_argument("greeting", false),
        ..tool("clash", "-R/./-/greet/x.txt")
    }];
    assert!(ToolRegistry::build(vec![set], cmr).is_err(), "key clashes");

    let mut set = tool_set();
    let mut fixed = tool("fixed", "hello/greet/x.txt");
    fixed.fixed.insert("nonexistent".to_string(), json!(1));
    set.tools = vec![fixed];
    assert!(ToolRegistry::build(vec![set], cmr).is_err(), "unknown fixed");
    Ok(())
}

#[tokio::test]
async fn check_allowed_walks_link_queries() -> Result<(), Error> {
    let envref = environment().await?;
    let cmr = envref.get_command_metadata_registry();
    let plan = liquers_core::plan::PlanBuilder::new(
        liquers_core::parse::parse_query("hello/wrap")?,
        cmr,
    )
    .build()?;
    let only_outer = vec!["--hello".to_string(), "--wrap".to_string()];
    assert!(check_allowed(&plan, &only_outer, cmr).is_err());
    let all = vec!["--*".to_string()];
    assert!(check_allowed(&plan, &all, cmr).is_ok());
    Ok(())
}

#[tokio::test]
async fn query_tool_honours_the_allow_list() -> Result<(), Error> {
    let server = server().await?;
    let output = server
        .call("t_query", args(json!({"query": "hello/upper/x.txt"})))
        .await?;
    assert_eq!(output.text, "HELLO");
    let refused = server
        .call("t_query", args(json!({"query": "hello/fail/x.txt"})))
        .await
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(refused.contains("not allowed"), "{refused}");
    Ok(())
}

async fn connect() -> Result<rmcp::service::RunningService<rmcp::RoleClient, ()>, Box<dyn std::error::Error>>
{
    let server = server().await?;
    let (server_io, client_io) = tokio::io::duplex(65536);
    tokio::spawn(async move {
        if let Ok(running) = server.serve(server_io).await {
            let _ = running.waiting().await;
        }
    });
    Ok(().serve(client_io).await?)
}

fn texts(result: &rmcp::model::CallToolResult) -> Vec<String> {
    result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect()
}

#[tokio::test]
async fn tool_error_is_a_result_not_a_protocol_error() -> Result<(), Box<dyn std::error::Error>> {
    let client = connect().await?;
    let result = client
        .call_tool(CallToolRequestParams::new("t_fail"))
        .await?;
    assert_eq!(result.is_error, Some(true));
    assert!(texts(&result)[0].contains("always fails"));
    assert!(client
        .call_tool(CallToolRequestParams::new("no_such_tool"))
        .await
        .is_err());
    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn notes_are_a_separate_content_block() -> Result<(), Box<dyn std::error::Error>> {
    let client = connect().await?;
    let result = client
        .call_tool(CallToolRequestParams::new("t_read").with_arguments(args(json!({"key": "notes/a.txt"}))))
        .await?;
    assert_eq!(texts(&result), vec!["NOTE A".to_string(), "upper ran".to_string()]);
    let tools = client.list_all_tools().await?;
    let names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
    assert!(names.contains(&"t_query".to_string()));
    let resource = client
        .read_resource(ReadResourceRequestParams::new("liquers://t/notes/a.txt"))
        .await?;
    assert_eq!(resource.contents.len(), 1);
    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn panicking_command_is_a_tool_error() -> Result<(), Box<dyn std::error::Error>> {
    let client = connect().await?;
    let result = client
        .call_tool(CallToolRequestParams::new("t_boom"))
        .await?;
    assert_eq!(result.is_error, Some(true));
    // The server survives the panic and answers the next call.
    let next = client
        .call_tool(CallToolRequestParams::new("t_greet"))
        .await?;
    assert_eq!(texts(&next)[0], "Hello, note a!");
    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn resources_render_text_and_commands() -> Result<(), Error> {
    let server = server().await?;
    assert_eq!(server.read("liquers://t/guide").await?, Some("Guide text".to_string()));
    let commands = server.read("liquers://commands/").await?;
    assert_eq!(commands, None, "an empty namespace variable does not match");
    let markdown = server.registry().commands_markdown("");
    assert!(markdown.contains("`--greet`"), "{markdown}");
    assert!(server.instructions().contains("Test tools."));
    Ok(())
}
