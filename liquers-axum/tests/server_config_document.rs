//! A server from one configuration document (`specs/design/configuration-document/` Phase 3,
//! test 8: AC-6 and AC-7).

use std::time::Duration;

use liquers_axum::{Api, LiquersServerBuilder, ServerConfig, WebSetting};
use liquers_core::command_metadata::{CommandKey, CommandMetadataRegistry};
use liquers_core::context::GenericEnvironment;
use liquers_core::environment_builder::{EnvironmentBuilder, Inline};
use liquers_core::environment_config::ConfigDocument;
use liquers_core::error::ErrorType;
use liquers_core::store_factory::default_store_factory;
use liquers_core::validate::registry::from_json_or_yaml;
use liquers_core::value::Value;

type Env = GenericEnvironment<Value, (), Inline>;

#[test]
fn server_config_defaults_from_an_empty_document() {
    for text in ["", "{}", "store: {stores: []}\n"] {
        let mut doc = ConfigDocument::from_yaml(text).unwrap();
        assert_eq!(ServerConfig::from_document(&mut doc).unwrap(), ServerConfig::default());
        doc.finish().unwrap();
    }
    let mut doc = ConfigDocument::from_yaml("server: {apis: [query, assets], web: site}\n").unwrap();
    let config = ServerConfig::from_document(&mut doc).unwrap();
    assert_eq!(config.apis, vec![Api::Query, Api::Assets]);
    assert_eq!(config.web, WebSetting::Root("site".into()));
}

#[test]
fn an_unknown_section_still_fails_after_the_server_is_taken() {
    let mut doc = ConfigDocument::from_yaml("server: {port: 0}\nsever: {port: 1}\n").unwrap();
    ServerConfig::from_document(&mut doc).unwrap();
    let error = doc.finish().unwrap_err();
    assert_eq!(error.error_type, ErrorType::ConfigurationError);
    assert!(error.to_string().contains("'sever'") && error.to_string().contains("server"), "{error}");
}

#[tokio::test]
async fn rust_host_builds_from_one_document() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("commands.yaml"),
        "commands:\n  - name: hello\n    label: Say hello\n    doc: Greets.\n    state_argument: null\n    arguments: []\ndefault_namespaces: ['', root]\nglobal_enums: {}\n",
    )
    .unwrap();
    let path = dir.path().join("liquers.yaml");
    std::fs::write(
        &path,
        "store:\n  stores:\n    - {type: memory, prefix: tmp}\nrecipes: trivial\ncommand_registry: commands.yaml\nserver:\n  port: 0\n  prefix: /api\n  apis: [query]\n  web: false\n",
    )
    .unwrap();

    let mut doc = ConfigDocument::load(&path).unwrap();
    let server = ServerConfig::from_document(&mut doc).unwrap();
    let cmr_path = doc.command_registry_path().unwrap();
    assert_eq!(cmr_path, dir.path().join("commands.yaml"));
    let environment = doc.finish().unwrap();

    let mut builder = EnvironmentBuilder::<Value, (), Inline>::new()
        .with_config(environment, Box::new(default_store_factory()));
    builder
        .command_registry
        .register_command(CommandKey::new_name("hello"), |_, _, _| Ok(Value::from("Hello")))
        .unwrap();
    let text = std::fs::read_to_string(&cmr_path).unwrap();
    let cmr: CommandMetadataRegistry = from_json_or_yaml(&cmr_path.to_string_lossy(), &text).unwrap();
    for command in &cmr.commands {
        builder.command_registry.command_metadata_registry.add_command(command);
    }
    let envref = builder.build().unwrap();
    let label = envref
        .get_command_metadata_registry()
        .find_command("", "", "hello")
        .map(|c| c.label);
    assert_eq!(label.as_deref(), Some("Say hello"));

    let running = LiquersServerBuilder::<Env>::from_config(server).bind(envref).await.unwrap();
    let base = format!("http://{}", running.local_addr());
    assert_eq!(reqwest::get(format!("{base}/api/q/hello")).await.unwrap().text().await.unwrap(), "Hello");
    assert_eq!(reqwest::get(format!("{base}/")).await.unwrap().status(), reqwest::StatusCode::NOT_FOUND);
    assert_eq!(
        reqwest::get(format!("{base}/api/api/store/data/tmp/x")).await.unwrap().status(),
        reqwest::StatusCode::NOT_FOUND
    );
    running.shutdown(Duration::from_secs(1)).await;
}
