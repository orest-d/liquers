//! The Liquers environment the guides are expanded in: read-only mounts of `docs/` and `specs/`,
//! the library's commands, and the `ns-docs` commands.

use std::path::Path;

use liquers_core::commands::CommandRegistry;
use liquers_core::context::EnvRef;
use liquers_core::error::Error;
use liquers_core::query::Key;
use liquers_core::store_config::StoreRouterConfig;
use liquers_lib::environment::{default_environment_builder, DefaultEnvironment};
use liquers_lib::ui::SimpleUIPayload;
use liquers_lib::value::Value;

/// The environment chapters are expanded in. The UI payload is the one the `lui` commands of
/// `register_all_commands!` need.
pub type DocsEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

/// Registers the `ns-docs` commands: `include` and `command_table`.
pub fn register_docs_commands(cr: &mut CommandRegistry<DocsEnvironment>) -> Result<(), Error> {
    // Named by the code `register_command!` expands to.
    #[allow(unused_imports)]
    use liquers_core::context::Context;
    #[allow(dead_code)]
    type CommandEnvironment = DocsEnvironment;
    use crate::include::{command_table, include};
    use liquers_macro::register_command;

    register_command!(cr,
        fn include(state, heading: String) -> result
        namespace: "docs"
        label: "Include a section"
        doc: "The body of the Markdown section with this heading (number optional), subsections included, own heading dropped; fails when absent or when it holds a relative link"
    )?;
    register_command!(cr,
        fn command_table(state) -> result
        namespace: "docs"
        label: "Command table"
        doc: "A registry export (specs/command_registry.yaml) as Markdown tables, one per namespace"
    )?;
    Ok(())
}

/// The store configuration: `docs` and `specs` mounted read-only from the repository root.
pub fn store_config(repo_root: &Path) -> Result<StoreRouterConfig, Error> {
    let root = repo_root.display().to_string();
    let json = serde_json::json!({
        "stores": [
            { "type": "filesystem", "prefix": "docs", "read_only": true, "config": { "path": root } },
            { "type": "filesystem", "prefix": "specs", "read_only": true, "config": { "path": root } },
        ]
    });
    serde_json::from_value(json)
        .map_err(|e| Error::general_error(format!("Invalid docs store configuration: {e}")))
}

/// Builds the environment over the repository at `repo_root`.
pub fn build_environment(repo_root: &Path) -> Result<EnvRef<DocsEnvironment>, Error> {
    let mut builder = default_environment_builder::<Value, SimpleUIPayload>();
    {
        #[allow(unused_imports)]
        use liquers_core::context::Context;
        #[allow(dead_code)]
        type CommandEnvironment = DocsEnvironment;
        let cr = &mut builder.command_registry;
        liquers_lib::register_all_commands!(cr)?;
        register_docs_commands(cr)?;
    }
    builder
        .with_store_config_unexpanded(
            store_config(repo_root)?,
            Box::new(liquers_store::store_factory::default_store_factory()),
        )
        .build()
}

/// Expands one chapter: evaluates `-R/<key>/-/ns-template/expand`, with the chapter's folder as
/// the working directory of its relative partials and queries.
pub async fn expand_chapter(env: &EnvRef<DocsEnvironment>, key: &Key) -> Result<String, Error> {
    // The default `cwd` link (`-R-key/.`) has no working directory in an ad-hoc query, so the
    // chapter's folder is passed explicitly.
    let cwd = liquers_core::escape::encode_token(key.parent().encode());
    let query = format!("-R/{}/-/ns-template/expand--strict-{cwd}", key.encode());
    let state = env.evaluate(&query).await?.get().await?;
    state.error_result()?;
    state.try_into_string()
}
