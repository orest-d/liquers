//! The tool registry, the key and allow-list checks, and the MCP server.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use liquers_core::command_metadata::{
    ArgumentType, CommandKey, CommandMetadata, CommandMetadataRegistry, CommandParameterValue,
};
use liquers_core::assets::AssetManager;
use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::Error;
use liquers_core::metadata::{LogEntryKind, Metadata};
use liquers_core::parse::{parse_key, parse_query};
use liquers_core::plan::{ParameterValue, Plan, PlanBuilder, Step};
use liquers_core::query::{Query, QuerySegment, ResourceName};
use liquers_core::recipes::Recipe;
use liquers_core::state::State;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock,
    ListResourceTemplatesResult, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
    ReadResourceRequestParams, Resource, ReadResourceResponse, ReadResourceResult,
    ResourceContents, ResourceTemplate, ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde_json::{Map, Value};

use crate::decl::{KeyArgumentDecl, QueryToolDecl, ResourceDecl, ToolDecl, ToolSetDecl};
use crate::schema::{argument_schema, format_property, FORMAT_ARGUMENT};

/// The argument of a query tool.
pub const QUERY_ARGUMENT: &str = "query";

/// A tool built from a [`ToolDecl`]: one query, one command.
#[derive(Clone, Debug)]
pub struct BasicTool {
    pub decl: ToolDecl,
    pub set: usize,
    pub query: Query,
    pub command: CommandKey,
    pub input_schema: Arc<Map<String, Value>>,
    /// Arguments a caller may not set: injected, fixed, and relative default links.
    pub hidden: BTreeSet<String>,
    /// Whether the engine's `format` argument is offered (the query has a filename).
    pub has_format: bool,
}

/// A set's query tool.
#[derive(Clone, Debug)]
pub struct QueryTool {
    pub decl: QueryToolDecl,
    pub set: usize,
    pub input_schema: Arc<Map<String, Value>>,
}

#[derive(Clone, Debug)]
pub enum ToolEntry {
    Basic(BasicTool),
    Query(QueryTool),
}

impl ToolEntry {
    fn set(&self) -> usize {
        match self {
            ToolEntry::Basic(tool) => tool.set,
            ToolEntry::Query(tool) => tool.set,
        }
    }
}

/// A resource with the tool name it refers to already prefixed.
#[derive(Clone, Debug)]
struct ResourceEntry {
    decl: ResourceDecl,
    /// For [`ResourceDecl::Tool`], the full tool name.
    tool: Option<String>,
}

/// Every tool of every set, checked against the command registry once, at start.
#[derive(Clone, Debug)]
pub struct ToolRegistry {
    pub sets: Vec<ToolSetDecl>,
    pub tools: BTreeMap<String, ToolEntry>,
    resources: Vec<ResourceEntry>,
    /// Command metadata the `Commands` resources render, captured at build time.
    commands: Vec<CommandMetadata>,
}

impl ToolRegistry {
    /// Plans every tool query once and derives its schema.
    ///
    /// Refused with a configuration error: a duplicate full name, an unparsable query, a query
    /// whose plan does not have exactly one action, an unregistered command, a `fixed` name the
    /// command does not have, a `key` on a query that does not begin with a resource segment, a
    /// `key` or `format` name that clashes with a command argument, and a resource naming an
    /// unknown tool.
    pub fn build(sets: Vec<ToolSetDecl>, cmr: &CommandMetadataRegistry) -> Result<Self, Error> {
        let mut tools = BTreeMap::new();
        let mut resources = Vec::new();
        for (index, set) in sets.iter().enumerate() {
            for decl in &set.tools {
                let name = format!("{}{}", set.prefix, decl.name);
                let tool = build_tool(index, set, decl, cmr)
                    .map_err(|e| Error::configuration_error(format!("Tool {name}: {e}")))?;
                insert_unique(&mut tools, name, ToolEntry::Basic(tool))?;
            }
            if let Some(decl) = &set.query_tool {
                let name = format!("{}{}", set.prefix, decl.name);
                let tool = QueryTool {
                    decl: decl.clone(),
                    set: index,
                    input_schema: Arc::new(query_tool_schema()),
                };
                insert_unique(&mut tools, name, ToolEntry::Query(tool))?;
            }
        }
        for set in &sets {
            for decl in &set.resources {
                let tool = match decl {
                    ResourceDecl::Tool { tool, uri_template, .. } => {
                        let full = format!("{}{}", set.prefix, tool);
                        if !tools.contains_key(&full) {
                            return Err(Error::configuration_error(format!(
                                "Resource {uri_template} names an unknown tool {full}"
                            )));
                        }
                        template_variable(uri_template)?;
                        Some(full)
                    }
                    ResourceDecl::Commands { uri_template, .. } => {
                        template_variable(uri_template)?;
                        None
                    }
                    ResourceDecl::Text { .. } => None,
                };
                resources.push(ResourceEntry {
                    decl: decl.clone(),
                    tool,
                });
            }
        }
        Ok(ToolRegistry {
            sets,
            tools,
            resources,
            commands: cmr.commands.clone(),
        })
    }

    /// The tool sets' instructions, joined by blank lines.
    pub fn instructions(&self) -> String {
        self.sets
            .iter()
            .map(|set| set.instructions.trim())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// The rmcp description of every tool.
    pub fn mcp_tools(&self) -> Vec<Tool> {
        self.tools
            .iter()
            .map(|(name, entry)| match entry {
                ToolEntry::Basic(tool) => {
                    let description = tool.decl.description.clone().unwrap_or_else(|| {
                        self.commands
                            .iter()
                            .find(|c| c.key() == tool.command)
                            .map(|c| c.doc.clone())
                            .unwrap_or_default()
                    });
                    Tool::new(name.clone(), description, tool.input_schema.clone())
                        .with_annotations(ToolAnnotations::new().read_only(tool.decl.read_only))
                }
                ToolEntry::Query(tool) => {
                    let description = tool.decl.description.clone().unwrap_or_else(|| {
                        format!(
                            "Evaluate a Liquers query. Allowed commands: {}.",
                            tool.decl.allow.join(", ")
                        )
                    });
                    Tool::new(name.clone(), description, tool.input_schema.clone())
                        .with_annotations(ToolAnnotations::new().read_only(true))
                }
            })
            .collect()
    }

    /// The Markdown reference of one namespace, rendered from the registry.
    pub fn commands_markdown(&self, namespace: &str) -> String {
        let mut text = format!("# Commands of namespace `{namespace}`\n");
        for command in self.commands.iter().filter(|c| c.namespace == namespace) {
            text.push_str(&format!("\n## `{}`\n\n", command.key()));
            let mut signature = format!("ns-{namespace}/{}", command.name);
            for argument in command.arguments.iter().filter(|a| !a.injected) {
                signature.push_str(&format!("-<{}>", argument.name));
            }
            text.push_str(&format!("`{signature}`\n\n"));
            if !command.doc.is_empty() {
                text.push_str(command.doc.trim());
                text.push_str("\n\n");
            }
            let arguments: Vec<_> = command.arguments.iter().filter(|a| !a.injected).collect();
            if !arguments.is_empty() {
                text.push_str("| Argument | Type | Default | Description |\n|---|---|---|---|\n");
                for argument in arguments {
                    let default = match &argument.default {
                        CommandParameterValue::Value(v) => format!("`{v}`"),
                        CommandParameterValue::Query(q) => format!("link `{}`", q.encode()),
                        CommandParameterValue::None => String::new(),
                    };
                    let mut kind = type_name(&argument.argument_type);
                    if argument.multiple {
                        kind = format!("{kind}…");
                    }
                    text.push_str(&format!(
                        "| `{}` | {} | {} | {} |\n",
                        argument.name,
                        kind,
                        default,
                        argument.description.replace('|', "\\|")
                    ));
                }
            }
        }
        text
    }
}

fn type_name(argument_type: &ArgumentType) -> String {
    match argument_type {
        ArgumentType::String => "string".to_string(),
        ArgumentType::Integer => "int".to_string(),
        ArgumentType::IntegerOption => "int?".to_string(),
        ArgumentType::Float => "float".to_string(),
        ArgumentType::FloatOption => "float?".to_string(),
        ArgumentType::Boolean => "bool".to_string(),
        ArgumentType::BooleanOption => "bool?".to_string(),
        ArgumentType::Enum(e) => e
            .values
            .iter()
            .map(|v| v.alias.clone())
            .collect::<Vec<_>>()
            .join(" \\| "),
        ArgumentType::GlobalEnum(name) => name.clone(),
        ArgumentType::Any => "any".to_string(),
        ArgumentType::None => "none".to_string(),
    }
}

fn insert_unique(
    tools: &mut BTreeMap<String, ToolEntry>,
    name: String,
    entry: ToolEntry,
) -> Result<(), Error> {
    if tools.contains_key(&name) {
        return Err(Error::configuration_error(format!(
            "Tool {name} is declared twice"
        )));
    }
    tools.insert(name, entry);
    Ok(())
}

fn query_tool_schema() -> Map<String, Value> {
    let mut schema = Map::new();
    schema.insert("type".to_string(), Value::String("object".to_string()));
    schema.insert(
        "properties".to_string(),
        serde_json::json!({QUERY_ARGUMENT: {
            "type": "string",
            "description": "An encoded Liquers query; its filename picks the output format"
        }}),
    );
    schema.insert("required".to_string(), serde_json::json!([QUERY_ARGUMENT]));
    schema.insert("additionalProperties".to_string(), Value::Bool(false));
    schema
}

fn recipe_for(query: &Query, cwd: &Option<String>) -> Result<Recipe, Error> {
    let mut recipe = Recipe::new(query.encode(), String::new(), String::new())?;
    recipe.cwd = cwd.clone();
    Ok(recipe)
}

fn build_tool(
    set_index: usize,
    set: &ToolSetDecl,
    decl: &ToolDecl,
    cmr: &CommandMetadataRegistry,
) -> Result<BasicTool, Error> {
    let query = parse_query(&decl.query)?;
    let plan = recipe_for(&query, &set.cwd)?.to_plan(cmr)?;
    let actions = plan.steps.iter().filter(|s| s.is_action()).count();
    if actions != 1 {
        return Err(Error::configuration_error(format!(
            "its query {} has {actions} actions; a tool query has exactly one",
            decl.query
        )));
    }
    let command = plan.last_action_command(cmr).ok_or_else(|| {
        Error::configuration_error(format!("the command of {} is not registered", decl.query))
    })?;
    let command = command.resolve_global_enums(cmr)?;
    let names: BTreeSet<&str> = command.arguments.iter().map(|a| a.name.as_str()).collect();
    for name in decl.fixed.keys() {
        if !names.contains(name.as_str()) {
            return Err(Error::configuration_error(format!(
                "fixed argument {name} is not an argument of {}",
                command.key()
            )));
        }
    }
    if let Some(key) = &decl.key {
        match query.segments.first() {
            Some(QuerySegment::Resource(_)) => {}
            Some(QuerySegment::Transform(_)) | None => {
                return Err(Error::configuration_error(format!(
                    "key argument {} needs a query beginning with a resource segment",
                    key.name
                )))
            }
        }
        if names.contains(key.name.as_str()) {
            return Err(Error::configuration_error(format!(
                "key argument {} clashes with an argument of {}",
                key.name,
                command.key()
            )));
        }
    }
    let has_format = query.filename().is_some();
    if has_format && names.contains(FORMAT_ARGUMENT) {
        return Err(Error::configuration_error(format!(
            "{} has its own argument {FORMAT_ARGUMENT}, which clashes with the engine's",
            command.key()
        )));
    }
    let mut hidden: BTreeSet<String> = decl.fixed.keys().cloned().collect();
    for argument in &command.arguments {
        let relative_link = match &argument.default {
            CommandParameterValue::Query(link) => link.has_relative_operand(),
            CommandParameterValue::Value(_) | CommandParameterValue::None => false,
        };
        if argument.injected || relative_link {
            hidden.insert(argument.name.clone());
        }
    }
    let mut schema = argument_schema(&command, decl.key.as_ref(), &hidden);
    if has_format {
        let extension = query
            .filename()
            .and_then(|f| f.extension())
            .unwrap_or_default();
        if let Some(Value::Object(properties)) = schema.get_mut("properties") {
            properties.insert(FORMAT_ARGUMENT.to_string(), format_property(&extension));
        }
    }
    Ok(BasicTool {
        decl: decl.clone(),
        set: set_index,
        query,
        command: command.key(),
        input_schema: Arc::new(schema),
        hidden,
        has_format,
    })
}

/// Joins a caller-supplied key onto the key of the query's leading resource segment.
///
/// The argument is parsed as a key and refused when it is absolute (begins with `/`), has an
/// empty, `.` or `..` element, or an element beginning with `-` (which would read as a query
/// segment). A single trailing `/` is accepted. The join happens in the parsed query, never in
/// its text, and the result must survive an encode–parse round trip unchanged.
pub fn fill_key(query: &Query, argument: &str) -> Result<Query, Error> {
    let refuse = |why: &str| {
        Error::parse_error(format!(
            "Key argument '{argument}' is refused: {why}; give a key relative to the root, \
             such as notes/idea.md"
        ))
    };
    if argument.starts_with('/') {
        return Err(refuse("it is absolute"));
    }
    let trimmed = argument.strip_suffix('/').unwrap_or(argument);
    if trimmed.is_empty() {
        return Ok(query.clone());
    }
    for element in trimmed.split('/') {
        if element.is_empty() {
            return Err(refuse("it has an empty element"));
        }
        if element == "." || element == ".." {
            return Err(refuse("it has a '.' or '..' element"));
        }
        if element.starts_with('-') {
            return Err(refuse("an element begins with '-'"));
        }
    }
    let key = parse_key(trimmed).map_err(|e| refuse(&e.to_string()))?;
    if key.len() != trimmed.split('/').count() {
        return Err(refuse("it does not parse as a plain key"));
    }
    let mut filled = query.clone();
    match filled.segments.first_mut() {
        Some(QuerySegment::Resource(resource)) => resource.key.0.extend(key.0),
        Some(QuerySegment::Transform(_)) | None => {
            return Err(Error::configuration_error(format!(
                "Query {} does not begin with a resource segment",
                query.encode()
            )))
        }
    }
    let encoded = filled.encode();
    let reparsed = parse_query(&encoded).map_err(|e| refuse(&e.to_string()))?;
    if reparsed.encode() != encoded {
        return Err(refuse("it does not survive encoding"));
    }
    Ok(reparsed)
}

/// Replaces the extension of the query's filename.
pub fn with_format(query: &Query, format: &str) -> Result<Query, Error> {
    if format.is_empty() || !format.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(Error::parse_error(format!(
            "Format '{format}' is refused: give a file extension such as md, csv or json"
        )));
    }
    let mut changed = query.clone();
    match changed.segments.last_mut() {
        Some(QuerySegment::Transform(transform)) => match &transform.filename {
            Some(name) => {
                let stem = match name.name.rsplit_once('.') {
                    Some((stem, _)) => stem.to_string(),
                    None => name.name.clone(),
                };
                transform.filename = Some(ResourceName::new(format!("{stem}.{format}")));
            }
            None => {
                return Err(Error::configuration_error(format!(
                    "Query {} has no filename to change",
                    query.encode()
                )))
            }
        },
        Some(QuerySegment::Resource(_)) | None => {
            return Err(Error::configuration_error(format!(
                "Query {} has no filename to change",
                query.encode()
            )))
        }
    }
    Ok(changed)
}

/// Whether `command` matches an allow-list entry: an exact `CommandKey` Display, or a pattern
/// ending in `*` matching by prefix.
fn allowed(command: &CommandKey, allow: &[String]) -> bool {
    let shown = command.to_string();
    allow.iter().any(|pattern| match pattern.strip_suffix('*') {
        Some(prefix) => shown.starts_with(prefix),
        None => shown == *pattern,
    })
}

/// Refuses a plan running any command the allow-list does not match.
///
/// Walks nested plans, evaluated queries, query values and every link parameter, planning link
/// queries with `cmr`. Resource steps are allowed.
pub fn check_allowed(
    plan: &Plan,
    allow: &[String],
    cmr: &CommandMetadataRegistry,
) -> Result<(), Error> {
    for step in &plan.steps {
        match step {
            Step::Action {
                realm,
                ns,
                action_name,
                parameters,
                ..
            } => {
                let command = CommandKey::new(realm, ns, action_name);
                if !allowed(&command, allow) {
                    return Err(Error::not_supported(format!(
                        "Command {command} is not allowed in this query; allowed: {}",
                        allow.join(", ")
                    )));
                }
                for parameter in &parameters.0 {
                    check_parameter(parameter, allow, cmr)?;
                }
            }
            Step::Evaluate(query) | Step::UseQueryValue(query) => {
                check_query(query, allow, cmr)?;
            }
            Step::Plan(nested) => check_allowed(nested, allow, cmr)?,
            Step::GetAsset(_)
            | Step::GetAssetBinary(_)
            | Step::GetAssetMetadata(_)
            | Step::GetAssetRecipe(_)
            | Step::GetAssetDirectory(_)
            | Step::GetResource(_)
            | Step::GetResourceMetadata(_)
            | Step::GetResourceDirectory(_)
            | Step::Filename(_)
            | Step::Info(_)
            | Step::Warning(_)
            | Step::Error(_)
            | Step::SetCwd(_)
            | Step::UseKeyValue(_) => {}
        }
    }
    Ok(())
}

fn check_query(query: &Query, allow: &[String], cmr: &CommandMetadataRegistry) -> Result<(), Error> {
    let plan = PlanBuilder::new(query.clone(), cmr)
        .with_placeholders_allowed()
        .build()?;
    check_allowed(&plan, allow, cmr)
}

fn check_parameter(
    parameter: &ParameterValue,
    allow: &[String],
    cmr: &CommandMetadataRegistry,
) -> Result<(), Error> {
    match parameter {
        ParameterValue::DefaultLink(_, query)
        | ParameterValue::ParameterLink(_, query, _)
        | ParameterValue::OverrideLink(_, query)
        | ParameterValue::EnumLink(_, query, _) => check_query(query, allow, cmr),
        ParameterValue::MultipleParameters(_, items) => {
            for item in items {
                check_parameter(item, allow, cmr)?;
            }
            Ok(())
        }
        ParameterValue::DefaultValue(_, _)
        | ParameterValue::ParameterValue(_, _, _)
        | ParameterValue::OverrideValue(_, _)
        | ParameterValue::Placeholder(_)
        | ParameterValue::Injected(_)
        | ParameterValue::None => Ok(()),
    }
}

/// The literal text before a URI template's single variable, the variable's name, and the
/// literal text after it.
fn template_variable(template: &str) -> Result<(String, String, String), Error> {
    let refuse = || {
        Error::configuration_error(format!(
            "URI template {template} must have exactly one variable, such as {{+key}}"
        ))
    };
    let (before, rest) = template.split_once('{').ok_or_else(refuse)?;
    let (variable, after) = rest.split_once('}').ok_or_else(refuse)?;
    if after.contains('{') {
        return Err(refuse());
    }
    let variable = variable.trim_start_matches(['+', '#']);
    Ok((before.to_string(), variable.to_string(), after.to_string()))
}

fn match_template(template: &str, uri: &str) -> Option<(String, String)> {
    let (before, variable, after) = template_variable(template).ok()?;
    let value = uri.strip_prefix(&before)?.strip_suffix(&after)?;
    if value.is_empty() {
        return None;
    }
    Some((variable, value.to_string()))
}

/// What a tool call produced.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolOutput {
    /// The value serialized in `format`.
    pub text: String,
    /// The data format the query's filename (or the value's default) selected.
    pub format: String,
    /// Info and warning lines the evaluation logged, kept out of `text`.
    pub notes: Vec<String>,
}

impl ToolOutput {
    /// The parsed value, when the format is JSON.
    pub fn structured(&self) -> Option<Value> {
        if self.format == "json" {
            serde_json::from_str(&self.text).ok()
        } else {
            None
        }
    }
}

/// Log lines the planner and interpreter write about the evaluation itself, which are not the
/// command's notes. A log entry does not record who wrote it
/// (`LOG-ENTRY-HAS-NO-SOURCE`), so they are recognised by their text.
const ENGINE_NOTE_PREFIXES: [&str; 3] = [
    "Recipe set CWD to ",
    "Dependency detected: ",
    "Predecessor boundary not cut",
];

fn is_engine_note(message: &str) -> bool {
    ENGINE_NOTE_PREFIXES
        .iter()
        .any(|prefix| message.starts_with(prefix))
}

fn notes_of<V: liquers_core::value::ValueInterface>(state: &State<V>) -> Vec<String> {
    match &*state.metadata {
        Metadata::MetadataRecord(record) => record
            .log
            .iter()
            .filter(|entry| match entry.kind {
                LogEntryKind::Info | LogEntryKind::Warning => true,
                LogEntryKind::Debug | LogEntryKind::Error => false,
            })
            .filter(|entry| !is_engine_note(&entry.message))
            .map(|entry| entry.message.clone())
            .collect(),
        Metadata::LegacyMetadata(_) => Vec::new(),
    }
}

/// Serves the tools of a [`ToolRegistry`] over an environment.
pub struct McpServer<E: Environment> {
    envref: EnvRef<E>,
    tools: Arc<ToolRegistry>,
    instructions: String,
}

impl<E: Environment> Clone for McpServer<E> {
    fn clone(&self) -> Self {
        McpServer {
            envref: self.envref.clone(),
            tools: self.tools.clone(),
            instructions: self.instructions.clone(),
        }
    }
}

impl<E: Environment> McpServer<E> {
    pub fn new(envref: EnvRef<E>, tools: ToolRegistry) -> Self {
        let instructions = tools.instructions();
        McpServer {
            envref,
            tools: Arc::new(tools),
            instructions,
        }
    }

    /// Appends a paragraph to the server's instructions, e.g. text a binary computed at start.
    pub fn with_instructions(mut self, text: &str) -> Self {
        let text = text.trim();
        if !text.is_empty() {
            if !self.instructions.is_empty() {
                self.instructions.push_str("\n\n");
            }
            self.instructions.push_str(text);
        }
        self
    }

    pub fn instructions(&self) -> &str {
        &self.instructions
    }

    pub fn registry(&self) -> &ToolRegistry {
        &self.tools
    }

    pub fn envref(&self) -> &EnvRef<E> {
        &self.envref
    }

    /// The recipe a call would run, refused as the call would be.
    pub fn recipe(&self, tool: &str, arguments: &Map<String, Value>) -> Result<Recipe, Error> {
        let entry = self
            .tools
            .tools
            .get(tool)
            .ok_or_else(|| Error::general_error(format!("Unknown tool {tool}")))?;
        let set = &self.tools.sets[entry.set()];
        match entry {
            ToolEntry::Basic(basic) => self.basic_recipe(basic, set, arguments),
            ToolEntry::Query(query_tool) => {
                let mut text = None;
                for (name, value) in arguments {
                    match (name.as_str(), value) {
                        (QUERY_ARGUMENT, Value::String(q)) => text = Some(q.clone()),
                        (QUERY_ARGUMENT, other) => {
                            return Err(Error::parse_error(format!(
                                "Argument query must be a string, not {other}"
                            )))
                        }
                        (other, _) => {
                            return Err(Error::general_error(format!(
                                "Tool {tool} takes only the argument query, not {other}"
                            )))
                        }
                    }
                }
                let text = text.ok_or_else(|| {
                    Error::general_error(format!("Tool {tool} needs the argument query"))
                })?;
                let query = parse_query(&text)?;
                let recipe = recipe_for(&query, &set.cwd)?;
                let cmr = self.envref.get_command_metadata_registry();
                check_allowed(&recipe.to_plan(cmr)?, &query_tool.decl.allow, cmr)?;
                Ok(recipe)
            }
        }
    }

    fn basic_recipe(
        &self,
        tool: &BasicTool,
        set: &ToolSetDecl,
        arguments: &Map<String, Value>,
    ) -> Result<Recipe, Error> {
        let mut rest = arguments.clone();
        let mut query = tool.query.clone();
        if let Some(KeyArgumentDecl { name, required, .. }) = &tool.decl.key {
            let key = match rest.remove(name) {
                Some(Value::String(text)) => text,
                Some(Value::Null) | None => String::new(),
                Some(other) => {
                    return Err(Error::parse_error(format!(
                        "Argument {name} must be a string, not {other}"
                    )))
                }
            };
            if *required && key.trim_end_matches('/').is_empty() {
                return Err(Error::general_error(format!(
                    "Argument {name} is required: a key relative to the root"
                )));
            }
            query = fill_key(&query, &key)?;
        }
        if tool.has_format {
            match rest.remove(FORMAT_ARGUMENT) {
                Some(Value::String(format)) if !format.is_empty() => {
                    query = with_format(&query, &format)?
                }
                Some(Value::String(_)) | Some(Value::Null) | None => {}
                Some(other) => {
                    return Err(Error::parse_error(format!(
                        "Argument {FORMAT_ARGUMENT} must be a string, not {other}"
                    )))
                }
            }
        }
        let mut recipe = recipe_for(&query, &set.cwd)?;
        for (name, value) in rest {
            if tool.hidden.contains(&name) {
                return Err(Error::general_error(format!(
                    "Argument {name} cannot be set through this tool"
                )));
            }
            if value.is_null() {
                continue;
            }
            recipe.arguments.insert(name, value);
        }
        for (name, value) in &tool.decl.fixed {
            recipe.arguments.insert(name.clone(), value.clone());
        }
        Ok(recipe)
    }

    /// Plans a call without running it.
    pub fn plan(&self, tool: &str, arguments: &Map<String, Value>) -> Result<Plan, Error> {
        let recipe = self.recipe(tool, arguments)?;
        recipe.to_plan(self.envref.get_command_metadata_registry())
    }

    /// Runs a tool and serializes its value in the format the query's filename selects.
    ///
    /// The evaluation runs in a spawned task, so a panicking command becomes an error.
    pub async fn call(&self, tool: &str, arguments: Map<String, Value>) -> Result<ToolOutput, Error> {
        let recipe = self.recipe(tool, &arguments)?;
        // Plan once here so a refused override is reported before anything runs.
        recipe.to_plan(self.envref.get_command_metadata_registry())?;
        let envref = self.envref.clone();
        let task = tokio::spawn(async move {
            let asset = envref
                .get_asset_manager()
                .apply(recipe, State::new(), None)
                .await?;
            let state = asset.get().await?;
            let format = state.get_data_format();
            let bytes = state.as_bytes()?;
            let notes = notes_of(&state);
            let text = String::from_utf8(bytes).map_err(|_| {
                Error::general_error(format!(
                    "The result is binary ({format}); ask for a text format such as md or json"
                ))
            })?;
            Ok::<ToolOutput, Error>(ToolOutput {
                text,
                format,
                notes,
            })
        });
        match task.await {
            Ok(result) => result,
            Err(join_error) => Err(Error::execution_error(format!(
                "Tool {tool} failed: {}",
                if join_error.is_panic() {
                    "the command panicked".to_string()
                } else {
                    join_error.to_string()
                }
            ))),
        }
    }

    /// The text of a resource.
    pub async fn read(&self, uri: &str) -> Result<Option<String>, Error> {
        // A fixed URI wins over a template that would also match it.
        for resource in &self.tools.resources {
            if let ResourceDecl::Text { uri: own, text, .. } = &resource.decl {
                if own == uri {
                    return Ok(Some(text.clone()));
                }
            }
        }
        for resource in &self.tools.resources {
            match &resource.decl {
                ResourceDecl::Text { .. } => {}
                ResourceDecl::Tool {
                    uri_template,
                    argument,
                    ..
                } => {
                    if let (Some((_, value)), Some(tool)) =
                        (match_template(uri_template, uri), &resource.tool)
                    {
                        let mut arguments = Map::new();
                        arguments.insert(argument.clone(), Value::String(value));
                        return Ok(Some(self.call(tool, arguments).await?.text));
                    }
                }
                ResourceDecl::Commands {
                    uri_template,
                    namespaces,
                } => {
                    if let Some((_, namespace)) = match_template(uri_template, uri) {
                        if namespaces.contains(&namespace) {
                            return Ok(Some(self.tools.commands_markdown(&namespace)));
                        }
                    }
                }
            }
        }
        Ok(None)
    }
}

/// The MCP result of a call: the text, then the notes as a second block.
pub fn tool_result(output: Result<ToolOutput, Error>) -> CallToolResult {
    match output {
        Ok(output) => {
            let mut content = vec![ContentBlock::text(output.text.clone())];
            if !output.notes.is_empty() {
                content.push(ContentBlock::text(output.notes.join("\n")));
            }
            let mut result = CallToolResult::success(content);
            result.structured_content = output.structured();
            result
        }
        Err(error) => CallToolResult::error(vec![ContentBlock::text(error.to_string())]),
    }
}

impl<E: Environment> ServerHandler for McpServer<E> {
    fn get_info(&self) -> ServerConfig {
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .build();
        ServerConfig::new(capabilities).with_instructions(self.instructions.clone())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(self.tools.mcp_tools()))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools
            .mcp_tools()
            .into_iter()
            .find(|tool| tool.name == name)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let name = request.name.to_string();
        if !self.tools.tools.contains_key(&name) {
            return Err(McpError::invalid_params(format!("Unknown tool {name}"), None));
        }
        let arguments = request.arguments.unwrap_or_default();
        Ok(tool_result(self.call(&name, arguments).await).into())
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let resources = self
            .tools
            .resources
            .iter()
            .filter_map(|resource| match &resource.decl {
                ResourceDecl::Text {
                    uri,
                    name,
                    description,
                    ..
                } => Some(Resource::new(uri.clone(), name.clone()).with_description(description.clone())),
                ResourceDecl::Tool { .. } | ResourceDecl::Commands { .. } => None,
            })
            .collect();
        Ok(ListResourcesResult::with_all_items(resources))
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let templates = self
            .tools
            .resources
            .iter()
            .filter_map(|resource| match &resource.decl {
                ResourceDecl::Tool {
                    uri_template, tool, ..
                } => Some(
                    ResourceTemplate::new(uri_template.clone(), tool.clone())
                        .with_description(format!("Read through the tool {tool}")),
                ),
                ResourceDecl::Commands {
                    uri_template,
                    namespaces,
                } => Some(
                    ResourceTemplate::new(uri_template.clone(), "commands".to_string())
                        .with_description(format!(
                            "Command reference of a namespace: {}",
                            namespaces.join(", ")
                        )),
                ),
                ResourceDecl::Text { .. } => None,
            })
            .collect();
        Ok(ListResourceTemplatesResult::with_all_items(templates))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let uri = request.uri;
        match self.read(&uri).await {
            Ok(Some(text)) => {
                Ok(ReadResourceResult::new(vec![ResourceContents::text(text, uri)]).into())
            }
            Ok(None) => Err(McpError::resource_not_found(
                format!("No resource {uri}"),
                None,
            )),
            Err(error) => Err(McpError::internal_error(error.to_string(), None)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_key_rejects_query_shaped_keys() {
        let base = parse_query("-R/./-/read/read.md").unwrap();
        for bad in ["/etc/passwd", "a//b", "../x", "a/./b", "a/-/ns-x/y", "-R/x", "a/-x"] {
            assert!(fill_key(&base, bad).is_err(), "{bad} should be refused");
        }
    }

    #[test]
    fn fill_key_encodes_relative_to_the_base() {
        let base = parse_query("-R/./-/read/read.md").unwrap();
        let filled = fill_key(&base, "notes/idea.md").unwrap();
        assert_eq!(filled.encode(), "-R/./notes/idea.md/-/read/read.md");
        let folder = parse_query("-R-key/./-/list/list.md").unwrap();
        assert_eq!(
            fill_key(&folder, "notes/").unwrap().encode(),
            "-R-key/./notes/-/list/list.md"
        );
        assert_eq!(fill_key(&folder, "").unwrap().encode(), folder.encode());
    }

    #[test]
    fn with_format_changes_the_extension() {
        let query = parse_query("-R-key/./-/list/list.md").unwrap();
        assert_eq!(
            with_format(&query, "csv").unwrap().encode(),
            "-R-key/./-/list/list.csv"
        );
        assert!(with_format(&query, "../x").is_err());
    }

    #[test]
    fn templates_match_one_variable() {
        assert_eq!(
            match_template("liquers://memory/{+key}", "liquers://memory/notes/a.md"),
            Some(("key".to_string(), "notes/a.md".to_string()))
        );
        assert_eq!(match_template("liquers://memory/{+key}", "liquers://other/a"), None);
    }

    #[test]
    fn allow_patterns_match_command_keys() {
        let allow = vec!["-mem-list".to_string(), "-rec-*".to_string()];
        assert!(allowed(&CommandKey::new("", "mem", "list"), &allow));
        assert!(allowed(&CommandKey::new("", "rec", "eq"), &allow));
        assert!(!allowed(&CommandKey::new("", "mem", "write"), &allow));
    }
}
