//! Commands defined by declaration, with no Python callable: template commands (aliases of
//! `template/render`) and pipeline commands.
//!
//! An environment rebuilds its registry from scratch whenever a declaration changes (a new Python
//! command, an import), so a command registered only in the built registry would vanish at the
//! next rebuild. A declared command is kept in the environment's declarations instead and applied
//! at every build, after the Python commands; the last registration under a key wins, whichever
//! kind it is.
//!
//! # Main components
//!
//! - [`DeclaredCommand`]: one declared command; [`DeclaredCommand::apply`] registers it.
//! - [`Overrides`]: the metadata a declaration sets on the registered command (label, doc,
//!   filename).
//! - [`template_command`]: the declaration `Environment.template_command` makes.
//! - [`DeclaredCommand::Pipeline`]: the declaration `Environment.pipeline_command` makes.
//!
//! # Where it is used
//!
//! - `environment.rs`: `EnvCell::declare` stores one after a trial build; `configured_builder`
//!   applies them; `register` and `unregister` remove one under the same key.
//!
//! # Related concepts
//!
//! - `liquers_core::commands::CommandRegistry::register_alias`: what an alias declaration calls;
//!   `register_pipeline`: what a pipeline declaration calls.
//! - `specs/guides/TEMPLATE_GUIDE.md` §3: template commands; `specs/guides/PIPELINE_GUIDE.md`:
//!   pipeline commands.
//!
//! # Examples
//!
//! ```python
//! env.store_set("templates/greet.md", "Hello, {{.}}!")
//! env.template_command("greeting", template="-R/templates/greet.md")
//! assert env.evaluate("hello/greeting") == "Hello, hello!"
//! ```
//!
//! Executed in `liquers-py/tests/test_TEMPLATE_commands.py`.

use liquers_core::command_metadata::{ArgumentInfo, CommandKey, CommandMetadata, CommandParameterValue};
use liquers_core::commands::CommandRegistry;
use liquers_core::context::Environment;
use liquers_core::error::Error;
use liquers_core::pipeline::Pipeline;
use liquers_core::query::Query;

/// Metadata a declaration sets on its command; `None` keeps what registration made.
#[derive(Debug, Clone, Default)]
pub(crate) struct Overrides {
    pub label: Option<String>,
    pub doc: Option<String>,
    pub filename: Option<String>,
}

impl Overrides {
    fn apply(&self, metadata: &mut CommandMetadata) {
        if let Some(label) = &self.label {
            metadata.label = label.clone();
        }
        if let Some(doc) = &self.doc {
            metadata.doc = doc.clone();
        }
        if let Some(filename) = &self.filename {
            metadata.filename = filename.clone();
        }
    }
}

/// A command defined by declaration, re-applied at every build.
#[derive(Debug, Clone)]
pub(crate) enum DeclaredCommand {
    /// An alias of `target` with `head_parameters` fixed. `arguments` `None` takes the target's
    /// arguments after the head.
    Alias {
        key: CommandKey,
        target: CommandKey,
        head_parameters: Vec<CommandParameterValue>,
        arguments: Option<Vec<ArgumentInfo>>,
        overrides: Overrides,
    },
    /// A pipeline command: `pipeline` run with `arguments` bound to its placeholders.
    Pipeline {
        key: CommandKey,
        pipeline: Pipeline,
        arguments: Vec<ArgumentInfo>,
        overrides: Overrides,
    },
}

impl DeclaredCommand {
    pub(crate) fn key(&self) -> &CommandKey {
        match self {
            DeclaredCommand::Alias { key, .. } | DeclaredCommand::Pipeline { key, .. } => key,
        }
    }

    /// Registers the command in `registry`, replacing whatever was under its key.
    pub(crate) fn apply<E: Environment>(&self, registry: &mut CommandRegistry<E>) -> Result<(), Error> {
        match self {
            DeclaredCommand::Alias {
                key,
                target,
                head_parameters,
                arguments,
                overrides,
            } => {
                let arguments = match arguments {
                    Some(arguments) => arguments.clone(),
                    None => registry
                        .command_metadata_registry
                        .find_command(&target.realm, &target.namespace, &target.name)
                        .map(|metadata| {
                            metadata
                                .arguments
                                .get(head_parameters.len()..)
                                .unwrap_or_default()
                                .to_vec()
                        })
                        .ok_or_else(|| Error::alias_target_not_registered(key, target))?,
                };
                let metadata = registry.register_alias(
                    key.clone(),
                    target.clone(),
                    head_parameters.clone(),
                    arguments,
                )?;
                overrides.apply(metadata);
                Ok(())
            }
            DeclaredCommand::Pipeline {
                key,
                pipeline,
                arguments,
                overrides,
            } => {
                let metadata = registry.register_pipeline(key.clone(), pipeline.clone(), arguments.clone())?;
                overrides.apply(metadata);
                Ok(())
            }
        }
    }
}

/// A template command: an alias of `template/render` whose head is the template link.
pub(crate) fn template_command(key: CommandKey, link: Query, overrides: Overrides) -> DeclaredCommand {
    DeclaredCommand::Alias {
        key,
        target: CommandKey::new("", "template", "render"),
        head_parameters: vec![CommandParameterValue::Query(link)],
        arguments: None,
        overrides,
    }
}
