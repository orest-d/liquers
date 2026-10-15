//! The `store_config` realm and its plan compiler (`specs/design/store-adapters/`, Phase 2 §4).
//!
//! A store's metadata handlers, storage model and write strategy are named in its configuration by
//! queries of the realm `store_config`. The realm's entries are declared in the ordinary
//! [`CommandMetadataRegistry`], so a spec parses, plans and validates like any query, but they are
//! never executed by the interpreter: their subject is a [`MetadataRecord`], which is not a value.
//! Instead, a spec is planned once, when the store is built, and compiled here into a single
//! closure. Nothing walks the plan afterwards, and nothing on the read or write path takes a lock or
//! looks anything up.
//!
//! Each entry is registered in code, next to its builder ([`register_builtin_store_config`] for the
//! built-ins), as a [`CommandMetadata`] plus either a [`StepBuilder`] (a handler) or a
//! [`ComponentBuilder`] (a storage model or a write strategy).

use crate::command_metadata::{CommandKey, CommandMetadata, CommandMetadataRegistry};
use crate::error::Error;
use crate::metadata::{LogEntryKind, MetadataRecord};
use crate::parse::parse_query;
use crate::plan::{ParameterValue, Plan, PlanBuilder, ResolvedParameterValues, Step};
use crate::query::{Key, Position, Query, QuerySegment, SegmentHeader};
use std::collections::HashMap;

use super::DataFacts;

/// The realm every store configuration entry lives in.
pub const STORE_CONFIG_REALM: &str = "store_config";

/// What a compiled step may read besides the record it shapes.
pub struct FactsIn<'a> {
    /// The key the record describes. A fact, so key-specific steps are possible later.
    pub key: &'a Key,
    /// Size, backend time and ETag of the data object, when the backend reported them.
    pub facts: Option<&'a DataFacts>,
    /// The data, present only when the chain asked for it ([`CompiledChain::needs_bytes`]) or the
    /// operation supplied it (`set`).
    pub bytes: Option<&'a [u8]>,
    /// The record as stored, for steps that compare against it (`content_hash-on_change`).
    pub previous: Option<&'a MetadataRecord>,
    /// The store operation the chain runs for (`get`, `get_metadata`, `set`, `set_metadata`), for
    /// the messages that name it.
    pub operation: &'a str,
}

impl<'a> FactsIn<'a> {
    /// Facts about `key` with nothing else known.
    pub fn new(key: &'a Key) -> Self {
        FactsIn {
            key,
            facts: None,
            bytes: None,
            previous: None,
            operation: "",
        }
    }
}

/// What a compiled step may produce besides the record.
#[derive(Debug, Default)]
pub struct StepOut {
    /// Replacement bytes for the data object (front-matter write-back).
    pub data: Option<Vec<u8>>,
}

/// One compiled step, or a whole compiled chain: shapes the record in place.
#[cfg(not(target_arch = "wasm32"))]
pub type CompiledFn =
    Box<dyn Fn(&mut MetadataRecord, &FactsIn<'_>, &mut StepOut) -> Result<(), Error> + Send + Sync>;
/// One compiled step, or a whole compiled chain: shapes the record in place.
#[cfg(target_arch = "wasm32")]
pub type CompiledFn =
    Box<dyn Fn(&mut MetadataRecord, &FactsIn<'_>, &mut StepOut) -> Result<(), Error>>;

/// Whether a step or chain needs the data bytes for these facts.
#[cfg(not(target_arch = "wasm32"))]
pub type NeedsBytesFn = Box<dyn Fn(&FactsIn<'_>) -> bool + Send + Sync>;
/// Whether a step or chain needs the data bytes for these facts.
#[cfg(target_arch = "wasm32")]
pub type NeedsBytesFn = Box<dyn Fn(&FactsIn<'_>) -> bool>;

/// Where a chain runs. One handler can mean a different thing in each stage, and refuses a stage it
/// has no meaning in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    /// Building the record of an entry that has none (`on_default`).
    Default,
    /// Checking a record before it is written (`on_validate`). An error aborts the write.
    Validate,
    /// Shaping a record as it is written (`on_write`).
    Write,
    /// Shaping a record as it is read (`on_read`). An error becomes a warning in the record.
    Read,
}

impl Stage {
    /// The configuration field the stage's chain is written in.
    pub fn field(&self) -> &'static str {
        match self {
            Stage::Default => "on_default",
            Stage::Validate => "on_validate",
            Stage::Write => "on_write",
            Stage::Read => "on_read",
        }
    }
}

/// One compiled step.
pub struct CompiledStep {
    pub needs_bytes: NeedsBytesFn,
    pub run: CompiledFn,
}

impl CompiledStep {
    /// A step that never needs the bytes.
    pub fn new(run: CompiledFn) -> Self {
        CompiledStep {
            needs_bytes: Box::new(|_| false),
            run,
        }
    }

    /// A step that needs the bytes when `needs_bytes` says so.
    pub fn with_needs_bytes(run: CompiledFn, needs_bytes: NeedsBytesFn) -> Self {
        CompiledStep { needs_bytes, run }
    }
}

/// A compiled spec: every step of it, in one closure.
pub struct CompiledChain {
    pub needs_bytes: NeedsBytesFn,
    pub run: CompiledFn,
}

impl CompiledChain {
    /// The chain of an empty spec: does nothing.
    pub fn empty() -> Self {
        CompiledChain {
            needs_bytes: Box::new(|_| false),
            run: Box::new(|_, _, _| Ok(())),
        }
    }

    /// True when the chain needs the data bytes for these facts; the pipeline fetches them first.
    pub fn needs_bytes(&self, facts: &FactsIn<'_>) -> bool {
        (self.needs_bytes)(facts)
    }

    /// Runs the chain over `record`.
    pub fn run(
        &self,
        record: &mut MetadataRecord,
        facts: &FactsIn<'_>,
        out: &mut StepOut,
    ) -> Result<(), Error> {
        (self.run)(record, facts, out)
    }
}

/// A handler's builder: hard-coded where the handler is defined.
pub type StepBuilder = fn(Stage, &ResolvedParameterValues) -> Result<CompiledStep, Error>;

/// What a component entry builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComponentKind {
    /// A storage model, written in `storage`.
    Storage,
    /// A write strategy, written in `writes`.
    Writes,
}

impl ComponentKind {
    /// The configuration field a component of this kind is written in.
    pub fn field(&self) -> &'static str {
        match self {
            ComponentKind::Storage => "storage",
            ComponentKind::Writes => "writes",
        }
    }

    fn describe(&self) -> &'static str {
        match self {
            ComponentKind::Storage => "a storage model",
            ComponentKind::Writes => "a write strategy",
        }
    }
}

/// A built component. The storage models and write strategies add their variants.
pub enum StoreComponent {}

/// A component's builder: same registration as a handler, different product.
pub type ComponentBuilder = fn(&ResolvedParameterValues) -> Result<StoreComponent, Error>;

/// The builders of the `store_config` entries, by key. Plain `fn` pointers, so cloning is cheap and
/// needs no `Arc`.
#[derive(Default, Clone)]
pub struct StoreConfigFunctions {
    steps: HashMap<CommandKey, StepBuilder>,
    components: HashMap<CommandKey, (ComponentKind, ComponentBuilder)>,
}

/// How an entry is registered: what to say when it appears where it does not belong.
enum EntryKind {
    Handler,
    Component(ComponentKind),
}

impl StoreConfigFunctions {
    pub fn new() -> Self {
        Self::default()
    }

    /// The built-in entries with the registry that declares them, for a builder with no
    /// environment.
    pub fn builtin() -> (CommandMetadataRegistry, Self) {
        let mut cmr = CommandMetadataRegistry::new();
        let mut functions = StoreConfigFunctions::new();
        // The built-in registrations are constant; a failure here is a bug caught by the tests
        // that call `register_builtin_store_config` directly.
        if let Err(error) = register_builtin_store_config(&mut cmr, &mut functions) {
            eprintln!("store_config: built-in registration failed: {error}");
        }
        (cmr, functions)
    }

    /// Declares a handler: its metadata goes into `cmr` (moved into realm `store_config`), its
    /// builder here.
    pub fn register_step(
        &mut self,
        cmr: &mut CommandMetadataRegistry,
        metadata: CommandMetadata,
        builder: StepBuilder,
    ) -> Result<(), Error> {
        let key = self.declare(cmr, metadata)?;
        self.components.remove(&key);
        self.steps.insert(key, builder);
        Ok(())
    }

    /// Declares a component of `kind`: its metadata goes into `cmr`, its builder here.
    pub fn register_component(
        &mut self,
        cmr: &mut CommandMetadataRegistry,
        metadata: CommandMetadata,
        kind: ComponentKind,
        builder: ComponentBuilder,
    ) -> Result<(), Error> {
        let key = self.declare(cmr, metadata)?;
        self.steps.remove(&key);
        self.components.insert(key, (kind, builder));
        Ok(())
    }

    fn declare(
        &self,
        cmr: &mut CommandMetadataRegistry,
        mut metadata: CommandMetadata,
    ) -> Result<CommandKey, Error> {
        if metadata.name.is_empty() {
            return Err(Error::configuration_error(
                "A store_config entry needs a name".to_string(),
            ));
        }
        metadata.with_realm(STORE_CONFIG_REALM);
        if metadata.label.is_empty() {
            let name = metadata.name.clone();
            metadata.with_label(&name);
        }
        cmr.add_command(&metadata);
        Ok(metadata.key())
    }

    /// True when no entry is registered.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty() && self.components.is_empty()
    }

    fn kind_of(&self, key: &CommandKey) -> Option<EntryKind> {
        if self.steps.contains_key(key) {
            return Some(EntryKind::Handler);
        }
        self.components
            .get(key)
            .map(|(kind, _)| EntryKind::Component(*kind))
    }

    /// Compiles a handler chain for `stage`. An empty spec compiles to a chain with no steps.
    pub fn compile(
        &self,
        spec: &str,
        stage: Stage,
        registry: &CommandMetadataRegistry,
    ) -> Result<CompiledChain, Error> {
        let (filled, plan) = plan_spec(spec, stage.field(), registry)?;
        let mut steps = Vec::new();
        let context = CompileContext {
            spec: &filled,
            field: stage.field(),
        };
        self.compile_plan(&plan, stage, &context, &mut steps)?;
        if steps.is_empty() {
            return Ok(CompiledChain::empty());
        }
        let read_stage = stage == Stage::Read;
        let mut predicates = Vec::with_capacity(steps.len());
        let mut runs = Vec::with_capacity(steps.len());
        for step in steps {
            predicates.push(step.needs_bytes);
            runs.push(step.run);
        }
        Ok(CompiledChain {
            needs_bytes: Box::new(move |facts| predicates.iter().any(|p| p(facts))),
            run: Box::new(move |record, facts, out| {
                for run in runs.iter() {
                    if let Err(error) = run(record, facts, out) {
                        if read_stage {
                            // Phase 1 Q9: a read never fails because a handler did.
                            record.warning(&format!("Metadata handler failed: {error}"));
                        } else {
                            return Err(error);
                        }
                    }
                }
                Ok(())
            }),
        })
    }

    /// Compiles a component spec: exactly one entry, of `kind`.
    pub fn compile_component(
        &self,
        spec: &str,
        kind: ComponentKind,
        registry: &CommandMetadataRegistry,
    ) -> Result<StoreComponent, Error> {
        let (filled, plan) = plan_spec(spec, kind.field(), registry)?;
        let context = CompileContext {
            spec: &filled,
            field: kind.field(),
        };
        let mut actions = Vec::new();
        for step in plan.steps.iter() {
            match step {
                Step::Action {
                    realm,
                    ns,
                    action_name,
                    position,
                    parameters,
                    ..
                } => actions.push((realm, ns, action_name, position, parameters)),
                Step::Info(_) | Step::Warning(_) | Step::Error(_) => {}
                Step::GetAsset(_)
                | Step::GetAssetBinary(_)
                | Step::GetAssetMetadata(_)
                | Step::GetAssetRecipe(_)
                | Step::GetAssetDirectory(_)
                | Step::GetResource(_)
                | Step::GetResourceMetadata(_)
                | Step::GetResourceDirectory(_)
                | Step::Evaluate(_)
                | Step::UseQueryValue(_)
                | Step::Filename(_)
                | Step::Plan(_)
                | Step::SetCwd(_)
                | Step::UseKeyValue(_) => {
                    return Err(context.refuse_step(step));
                }
            }
        }
        let [(realm, ns, action_name, position, parameters)] = actions.as_slice() else {
            return Err(context.error(
                &format!("names {} entries; it must name exactly one", actions.len()),
                None,
            ));
        };
        let key = CommandKey::new(realm, ns, action_name);
        match self.kind_of(&key) {
            Some(EntryKind::Component(found)) if found == kind => {}
            Some(EntryKind::Component(found)) => {
                return Err(context.error(
                    &format!(
                        "'{action_name}' is {}, not {}",
                        found.describe(),
                        kind.describe()
                    ),
                    Some(position),
                ))
            }
            Some(EntryKind::Handler) => {
                return Err(context.error(
                    &format!("'{action_name}' is a handler, not {}", kind.describe()),
                    Some(position),
                ))
            }
            None => {
                return Err(context.error(
                    &format!("'{action_name}' is not a registered store_config entry"),
                    Some(position),
                ))
            }
        }
        check_literals(parameters, &context, position)?;
        let builder = match self.components.get(&key) {
            Some((_, builder)) => *builder,
            None => {
                return Err(
                    context.error(&format!("'{action_name}' has no builder"), Some(position))
                )
            }
        };
        builder(parameters).map_err(|e| context.error(&e.message, Some(position)))
    }

    fn compile_plan(
        &self,
        plan: &Plan,
        stage: Stage,
        context: &CompileContext<'_>,
        out: &mut Vec<CompiledStep>,
    ) -> Result<(), Error> {
        for step in plan.init_steps.iter().chain(plan.steps.iter()) {
            match step {
                Step::Action {
                    realm,
                    ns,
                    action_name,
                    position,
                    parameters,
                    ..
                } => {
                    let key = CommandKey::new(realm, ns, action_name);
                    let builder = match (self.steps.get(&key), self.kind_of(&key)) {
                        (Some(builder), _) => *builder,
                        (None, Some(EntryKind::Component(kind))) => {
                            return Err(context.error(
                                &format!(
                                    "'{action_name}' is {}, written in '{}', not a handler",
                                    kind.describe(),
                                    kind.field()
                                ),
                                Some(position),
                            ))
                        }
                        (None, Some(EntryKind::Handler)) | (None, None) => {
                            return Err(context.error(
                                &format!(
                                    "'{action_name}' is not a registered store_config handler"
                                ),
                                Some(position),
                            ))
                        }
                    };
                    check_literals(parameters, context, position)?;
                    let step = builder(stage, parameters)
                        .map_err(|e| context.error(&e.message, Some(position)))?;
                    out.push(step);
                }
                Step::Info(message) => out.push(message_step(LogEntryKind::Info, message)),
                Step::Warning(message) => out.push(message_step(LogEntryKind::Warning, message)),
                Step::Error(message) => out.push(message_step(LogEntryKind::Error, message)),
                Step::Plan(nested) => self.compile_plan(nested, stage, context, out)?,
                Step::SetCwd(_) => {
                    return Err(
                        context.error("sets a working directory, which is not supported", None)
                    )
                }
                Step::GetAsset(_)
                | Step::GetAssetBinary(_)
                | Step::GetAssetMetadata(_)
                | Step::GetAssetRecipe(_)
                | Step::GetAssetDirectory(_)
                | Step::GetResource(_)
                | Step::GetResourceMetadata(_)
                | Step::GetResourceDirectory(_)
                | Step::Evaluate(_)
                | Step::UseQueryValue(_)
                | Step::Filename(_)
                | Step::UseKeyValue(_) => return Err(context.refuse_step(step)),
            }
        }
        Ok(())
    }
}

/// The spec being compiled, for error messages.
struct CompileContext<'a> {
    spec: &'a str,
    field: &'a str,
}

impl CompileContext<'_> {
    fn error(&self, message: &str, position: Option<&Position>) -> Error {
        let at = match position {
            Some(position) if !position.is_unknown() => format!(" (at {position})"),
            Some(_) | None => String::new(),
        };
        Error::configuration_error(format!(
            "Store configuration '{}: {}': {message}{at}",
            self.field, self.spec
        ))
    }

    fn refuse_step(&self, step: &Step) -> Error {
        let what = match step {
            Step::GetResource(_)
            | Step::GetResourceMetadata(_)
            | Step::GetResourceDirectory(_)
            | Step::UseKeyValue(_) => {
                "a resource segment, which is not supported in store configuration yet"
            }
            Step::GetAsset(_)
            | Step::GetAssetBinary(_)
            | Step::GetAssetMetadata(_)
            | Step::GetAssetRecipe(_)
            | Step::GetAssetDirectory(_)
            | Step::Evaluate(_)
            | Step::UseQueryValue(_) => "a step that needs the asset layer",
            Step::Filename(_) => "an output filename",
            Step::SetCwd(_) => "a working directory",
            Step::Plan(_) => "a nested plan",
            Step::Action { .. } | Step::Info(_) | Step::Warning(_) | Step::Error(_) => "a step",
        };
        self.error(&format!("contains {what}"), None)
    }
}

/// A message step: appends its message to the record's log at `kind`, unless that entry is
/// already there, so a record shaped many times carries it once.
fn message_step(kind: LogEntryKind, message: &str) -> CompiledStep {
    let message = message.to_string();
    CompiledStep::new(Box::new(move |record, _facts, _out| {
        if !record
            .log
            .iter()
            .any(|entry| entry.kind == kind && entry.message == message)
        {
            match kind {
                LogEntryKind::Debug => record.debug(&message),
                LogEntryKind::Info => record.info(&message),
                LogEntryKind::Warning => record.warning(&message),
                LogEntryKind::Error => record.error(&message),
            };
        }
        Ok(())
    }))
}

/// Refuses parameters that are not literals: a link, a placeholder or an injected parameter has
/// no meaning when nothing is evaluated.
fn check_literals(
    parameters: &ResolvedParameterValues,
    context: &CompileContext<'_>,
    position: &Position,
) -> Result<(), Error> {
    fn literal(parameter: &ParameterValue) -> Result<(), String> {
        match parameter {
            ParameterValue::DefaultValue(_, _)
            | ParameterValue::ParameterValue(_, _, _)
            | ParameterValue::OverrideValue(_, _) => Ok(()),
            ParameterValue::MultipleParameters(_, elements) => {
                elements.iter().try_for_each(literal)
            }
            ParameterValue::DefaultLink(name, _)
            | ParameterValue::ParameterLink(name, _, _)
            | ParameterValue::OverrideLink(name, _)
            | ParameterValue::EnumLink(name, _, _) => Err(format!(
                "parameter '{name}' is a link; only literals are allowed"
            )),
            ParameterValue::Placeholder(name) => Err(format!("parameter '{name}' is missing")),
            ParameterValue::Injected(name) => Err(format!(
                "parameter '{name}' is injected, which is not supported"
            )),
            ParameterValue::None => Err("a parameter is unresolved".to_string()),
        }
    }
    parameters
        .iter()
        .try_for_each(literal)
        .map_err(|message| context.error(&message, Some(position)))
}

/// Parses a spec, fills the realm in and plans it. Returns the filled spec's text with the plan.
fn plan_spec(
    spec: &str,
    field: &str,
    registry: &CommandMetadataRegistry,
) -> Result<(String, Plan), Error> {
    let query = parse_query(spec).map_err(|e| {
        Error::configuration_error(format!(
            "Store configuration '{field}: {spec}' does not parse: {}",
            e.message
        ))
    })?;
    let query = fill_realm(query)?;
    let filled = query.encode();
    let plan = PlanBuilder::new(query, registry).build().map_err(|e| {
        Error::configuration_error(format!(
            "Store configuration '{field}: {filled}' does not plan: {}",
            e.message
        ))
    })?;
    Ok((filled, plan))
}

/// Fills the realm `store_config` into every transform segment of `query`.
///
/// A segment with no header gets `-store_config`; a header with an empty name gets the name. A
/// header naming another realm, or carrying a level or header parameters, is refused. Filling works
/// on the parsed query, never on its text, so a spec that begins with a resource segment keeps its
/// shape (it is refused later, by the compiler).
pub fn fill_realm(mut query: Query) -> Result<Query, Error> {
    for segment in query.segments.iter_mut() {
        match segment {
            QuerySegment::Transform(transform) => match transform.header.as_mut() {
                None => {
                    transform.header = Some(SegmentHeader {
                        name: STORE_CONFIG_REALM.to_string(),
                        ..Default::default()
                    })
                }
                Some(header) => {
                    if header.level > 0 || !header.parameters.is_empty() {
                        return Err(Error::configuration_error(format!(
                            "Store configuration header '-{}' may not carry a level or parameters",
                            header.name
                        )));
                    }
                    if header.name.is_empty() {
                        header.name = STORE_CONFIG_REALM.to_string();
                    } else if header.name != STORE_CONFIG_REALM {
                        return Err(Error::configuration_error(format!(
                            "Store configuration names realm '{}'; only '{STORE_CONFIG_REALM}' is allowed",
                            header.name
                        )));
                    }
                }
            },
            QuerySegment::Resource(_) => {}
        }
    }
    Ok(query)
}

/// Registers the built-in entries of the realm: the handlers in core, and (from step 6 on) the
/// storage models and write strategies. Called by `CommandRegistry::new()`, so every environment can
/// name the built-ins, and by the registry exporter.
pub fn register_builtin_store_config(
    cmr: &mut CommandMetadataRegistry,
    functions: &mut StoreConfigFunctions,
) -> Result<(), Error> {
    super::handlers::register_builtin_handlers(cmr, functions)?;
    Ok(())
}

/// Reads a string parameter by name; a missing or non-string parameter is an error.
pub fn string_parameter(parameters: &ResolvedParameterValues, name: &str) -> Result<String, Error> {
    for parameter in parameters.iter() {
        match parameter {
            ParameterValue::DefaultValue(n, value)
            | ParameterValue::ParameterValue(n, value, _)
            | ParameterValue::OverrideValue(n, value)
                if n == name =>
            {
                return match value {
                    serde_json::Value::String(s) => Ok(s.clone()),
                    serde_json::Value::Null => Ok(String::new()),
                    serde_json::Value::Bool(_)
                    | serde_json::Value::Number(_)
                    | serde_json::Value::Array(_)
                    | serde_json::Value::Object(_) => Ok(value.to_string()),
                };
            }
            ParameterValue::DefaultValue(_, _)
            | ParameterValue::ParameterValue(_, _, _)
            | ParameterValue::OverrideValue(_, _)
            | ParameterValue::DefaultLink(_, _)
            | ParameterValue::ParameterLink(_, _, _)
            | ParameterValue::OverrideLink(_, _)
            | ParameterValue::Placeholder(_)
            | ParameterValue::EnumLink(_, _, _)
            | ParameterValue::MultipleParameters(_, _)
            | ParameterValue::Injected(_)
            | ParameterValue::None => {}
        }
    }
    Err(Error::configuration_error(format!(
        "parameter '{name}' is missing"
    )))
}

/// Reads a `multiple` string parameter by name; absent means empty.
pub fn string_list_parameter(
    parameters: &ResolvedParameterValues,
    name: &str,
) -> Result<Vec<String>, Error> {
    for parameter in parameters.iter() {
        if let ParameterValue::MultipleParameters(n, elements) = parameter {
            if n == name {
                let mut values = Vec::with_capacity(elements.len());
                for element in elements {
                    match element {
                        ParameterValue::ParameterValue(_, serde_json::Value::String(s), _)
                        | ParameterValue::DefaultValue(_, serde_json::Value::String(s))
                        | ParameterValue::OverrideValue(_, serde_json::Value::String(s)) => {
                            values.push(s.clone())
                        }
                        ParameterValue::ParameterValue(_, other, _)
                        | ParameterValue::DefaultValue(_, other)
                        | ParameterValue::OverrideValue(_, other) => values.push(other.to_string()),
                        ParameterValue::DefaultLink(_, _)
                        | ParameterValue::ParameterLink(_, _, _)
                        | ParameterValue::OverrideLink(_, _)
                        | ParameterValue::Placeholder(_)
                        | ParameterValue::EnumLink(_, _, _)
                        | ParameterValue::MultipleParameters(_, _)
                        | ParameterValue::Injected(_)
                        | ParameterValue::None => {
                            return Err(Error::configuration_error(format!(
                                "parameter '{name}' has an element that is not a literal"
                            )))
                        }
                    }
                }
                return Ok(values);
            }
        }
    }
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorType;
    use crate::metadata::{Status, Version};
    use crate::parse::parse_key;
    use crate::query::Key;
    use crate::store::handlers::changed_since_stored;
    use crate::store::DataFacts;

    fn builtin() -> (CommandMetadataRegistry, StoreConfigFunctions) {
        let mut cmr = CommandMetadataRegistry::new();
        let mut functions = StoreConfigFunctions::new();
        register_builtin_store_config(&mut cmr, &mut functions).expect("built-in registration");
        (cmr, functions)
    }

    fn compile(spec: &str, stage: Stage) -> Result<CompiledChain, Error> {
        let (cmr, functions) = builtin();
        functions.compile(spec, stage, &cmr)
    }

    fn run(
        chain: &CompiledChain,
        record: &mut MetadataRecord,
        facts: &FactsIn<'_>,
    ) -> Result<(), Error> {
        let mut out = StepOut::default();
        chain.run(record, facts, &mut out)
    }

    fn header_name(query: &Query) -> Option<String> {
        match query.segments.first() {
            Some(QuerySegment::Transform(t)) => t.header.as_ref().map(|h| h.name.clone()),
            Some(QuerySegment::Resource(_)) | None => None,
        }
    }

    fn is_configuration_error(result: &Result<CompiledChain, Error>) -> bool {
        matches!(result, Err(e) if e.error_type == ErrorType::ConfigurationError)
    }

    fn message(result: Result<CompiledChain, Error>) -> String {
        match result {
            Ok(_) => String::new(),
            Err(e) => e.message.clone(),
        }
    }

    #[test]
    fn fill_realm_adds_header_to_headerless_spec() -> Result<(), Error> {
        let filled = fill_realm(parse_query("media_type-fill/file_facts")?)?;
        assert_eq!(header_name(&filled).as_deref(), Some(STORE_CONFIG_REALM));
        assert_eq!(filled.encode(), "-store_config/media_type-fill/file_facts");
        Ok(())
    }

    #[test]
    fn fill_realm_keeps_explicit_header() -> Result<(), Error> {
        let filled = fill_realm(parse_query("-store_config/defaults")?)?;
        assert_eq!(filled.encode(), "-store_config/defaults");
        Ok(())
    }

    #[test]
    fn fill_realm_fills_an_empty_header_name() -> Result<(), Error> {
        let filled = fill_realm(parse_query("-/defaults")?)?;
        assert_eq!(header_name(&filled).as_deref(), Some(STORE_CONFIG_REALM));
        Ok(())
    }

    #[test]
    fn fill_realm_refuses_another_realm() -> Result<(), Error> {
        let refused = fill_realm(parse_query("-lui/front_matter")?);
        assert!(matches!(refused, Err(e) if e.error_type == ErrorType::ConfigurationError));
        Ok(())
    }

    #[test]
    fn compile_refuses_unknown_entry_with_position() {
        let result = compile("media_type/no_such_handler", Stage::Read);
        assert!(is_configuration_error(&result));
        let text = message(result);
        assert!(text.contains("no_such_handler"), "{text}");
        assert!(text.contains("on_read"), "{text}");
    }

    #[test]
    fn compile_refuses_an_invalid_option() {
        let result = compile("media_type-sometimes", Stage::Read);
        assert!(is_configuration_error(&result));
        let result = compile("content_hash-never", Stage::Read);
        assert!(is_configuration_error(&result));
        let result = compile("set-colour-red", Stage::Read);
        assert!(message(result).contains("colour"));
    }

    #[test]
    fn compile_refuses_link_parameters() {
        let result = compile("set-title-~X~-R/title.txt~E", Stage::Read);
        assert!(is_configuration_error(&result));
        assert!(message(result).contains("link"));
    }

    #[test]
    fn compile_refuses_entry_in_a_stage_it_does_not_allow() {
        assert!(is_configuration_error(&compile(
            "require-title",
            Stage::Read
        )));
        assert!(is_configuration_error(&compile("updated", Stage::Read)));
        assert!(is_configuration_error(&compile(
            "defaults",
            Stage::Validate
        )));
        assert!(is_configuration_error(&compile(
            "file_facts",
            Stage::Validate
        )));
        assert!(compile("require-title", Stage::Validate).is_ok());
        assert!(compile("updated", Stage::Write).is_ok());
    }

    #[test]
    fn require_without_fields_is_refused() {
        let result = compile("require", Stage::Validate);
        assert!(is_configuration_error(&result));
        assert!(message(result).contains("at least one field"));
    }

    #[test]
    fn compile_refuses_set_cwd_and_resource_steps() {
        let resource = compile("-R-key/docs/notes/-store_config/media_type", Stage::Read);
        assert!(is_configuration_error(&resource));
        assert!(message(resource).contains("resource"));
        let cwd = compile("-R-cwd/docs/-store_config/media_type", Stage::Read);
        assert!(is_configuration_error(&cwd));
        let filename = compile("media_type/out.txt", Stage::Read);
        assert!(is_configuration_error(&filename));
    }

    #[test]
    fn chain_runs_in_order_and_the_later_step_wins() -> Result<(), Error> {
        let key = parse_key("a.txt")?;
        let facts = FactsIn::new(&key);
        for stage in [Stage::Write, Stage::Read] {
            let mut record = MetadataRecord::new();
            run(
                &compile("set-title-A/set-title-B", stage)?,
                &mut record,
                &facts,
            )?;
            assert_eq!(record.title, "B");
            let mut record = MetadataRecord::new();
            run(
                &compile("set-title-B/set-title-A", stage)?,
                &mut record,
                &facts,
            )?;
            assert_eq!(record.title, "A");
        }
        Ok(())
    }

    #[test]
    fn second_step_receives_the_first_steps_output() -> Result<(), Error> {
        // Validate runs `require` on the field `set` left there, in the same record.
        let (cmr, functions) = builtin();
        let key = parse_key("notes.md")?;
        let facts = FactsIn::new(&key);
        let write = functions.compile("media_type", Stage::Write, &cmr)?;
        let validate = functions.compile("require-media_type", Stage::Validate, &cmr)?;
        let mut record = MetadataRecord::new();
        assert!(run(&validate, &mut record, &facts).is_err());
        run(&write, &mut record, &facts)?;
        assert_eq!(record.media_type.as_deref(), Some("text/markdown"));
        run(&validate, &mut record, &facts)?;
        // A literal with `/` is written with `~I`.
        let mut record = MetadataRecord::new();
        run(
            &compile("set-media_type-text~Iplain", Stage::Write)?,
            &mut record,
            &facts,
        )?;
        assert_eq!(record.media_type.as_deref(), Some("text/plain"));
        Ok(())
    }

    #[test]
    fn fill_mode_keeps_a_present_value() -> Result<(), Error> {
        let key = parse_key("a.txt")?;
        let facts = FactsIn::new(&key);
        let mut record = MetadataRecord::new();
        record.media_type = Some("application/x-custom".to_string());
        run(
            &compile("media_type-fill", Stage::Read)?,
            &mut record,
            &facts,
        )?;
        assert_eq!(record.media_type.as_deref(), Some("application/x-custom"));
        run(&compile("media_type", Stage::Read)?, &mut record, &facts)?;
        assert_eq!(record.media_type.as_deref(), Some("text/plain"));
        let mut record = MetadataRecord::new();
        run(
            &compile("set-title-T-fill", Stage::Read)?,
            &mut record,
            &facts,
        )?;
        assert_eq!(record.title, "T");
        Ok(())
    }

    #[test]
    fn planner_messages_are_logged_once() -> Result<(), Error> {
        // A message step is compiled like a handler; build one from a plan the planner annotated.
        let (cmr, functions) = builtin();
        let mut plan = PlanBuilder::new(fill_realm(parse_query("media_type")?)?, &cmr).build()?;
        plan.init_steps
            .push(Step::Warning("planner noticed".to_string()));
        let mut steps = Vec::new();
        let context = CompileContext {
            spec: "media_type",
            field: "on_read",
        };
        functions.compile_plan(&plan, Stage::Read, &context, &mut steps)?;
        let key = parse_key("a.txt")?;
        let facts = FactsIn::new(&key);
        let mut record = MetadataRecord::new();
        for _ in 0..2 {
            for step in &steps {
                (step.run)(&mut record, &facts, &mut StepOut::default())?;
            }
        }
        let count = record
            .log
            .iter()
            .filter(|e| e.message == "planner noticed")
            .count();
        assert_eq!(count, 1);
        Ok(())
    }

    #[test]
    fn read_stage_error_becomes_a_warning() -> Result<(), Error> {
        // `require` is refused outside Validate at build, so a failing read step is made here:
        // a chain compiled for Read wraps every step, whatever it is.
        let (cmr, mut functions) = builtin();
        fn failing(_stage: Stage, _p: &ResolvedParameterValues) -> Result<CompiledStep, Error> {
            Ok(CompiledStep::new(Box::new(|_r, _f, _o| {
                Err(Error::general_error("boom".to_string()))
            })))
        }
        let mut cmr = cmr;
        functions.register_step(&mut cmr, CommandMetadata::new("fails"), failing)?;
        let key = parse_key("a.txt")?;
        let facts = FactsIn::new(&key);
        let mut record = MetadataRecord::new();
        let chain = functions.compile("fails/set-title-T", Stage::Read, &cmr)?;
        run(&chain, &mut record, &facts)?;
        assert_eq!(record.title, "T");
        assert!(record.log.iter().any(|e| e.message.contains("boom")));
        let write = functions.compile("fails/set-title-T", Stage::Write, &cmr)?;
        assert!(run(&write, &mut MetadataRecord::new(), &facts).is_err());
        Ok(())
    }

    #[test]
    fn validate_stage_error_aborts() -> Result<(), Error> {
        let key = parse_key("a.txt")?;
        let facts = FactsIn::new(&key);
        let mut record = MetadataRecord::new();
        record.media_type = Some("image/png".to_string());
        let chain = compile("media_type/require-title", Stage::Validate)?;
        let error = run(&chain, &mut record, &facts)
            .err()
            .map(|e| e.message.clone());
        assert!(error.unwrap_or_default().contains("disagrees"));
        Ok(())
    }

    #[test]
    fn needs_bytes_only_when_a_step_needs_them() -> Result<(), Error> {
        let key = parse_key("a.txt")?;
        let facts = FactsIn::new(&key);
        assert!(!compile("media_type/file_facts", Stage::Read)?.needs_bytes(&facts));
        assert!(compile("content_hash-always", Stage::Read)?.needs_bytes(&facts));
        assert!(!compile("content_hash-off", Stage::Read)?.needs_bytes(&facts));
        // Bytes supplied by the operation are never asked for again.
        let mut with_bytes = FactsIn::new(&key);
        with_bytes.bytes = Some(b"x");
        assert!(!compile("content_hash-always", Stage::Write)?.needs_bytes(&with_bytes));
        Ok(())
    }

    fn facts(size: u64, modified: &str, etag: Option<&str>) -> DataFacts {
        DataFacts {
            size,
            modified: Some(crate::metadata::BackendTime::new(
                chrono::DateTime::parse_from_rfc3339(modified)
                    .map(|t| t.with_timezone(&chrono::Utc))
                    .unwrap_or_default(),
                crate::metadata::TimePrecision::Nanos,
            )),
            etag: etag.map(str::to_string),
        }
    }

    fn stored(data_facts: &DataFacts) -> MetadataRecord {
        let mut record = MetadataRecord::new();
        record.file_size = Some(data_facts.size);
        record.created = data_facts.modified.clone();
        record.etag = data_facts.etag.clone();
        record.version = Some(Version::from_bytes(b"old"));
        record
    }

    #[test]
    fn content_hash_on_change_follows_size_time_and_etag() -> Result<(), Error> {
        let key = parse_key("a.txt")?;
        let chain = compile("content_hash", Stage::Read)?;
        let before = facts(3, "2026-10-15T10:00:00Z", Some("\"1\""));
        let previous = stored(&before);
        let mut same = FactsIn::new(&key);
        same.facts = Some(&before);
        same.previous = Some(&previous);
        assert!(!chain.needs_bytes(&same));
        for after in [
            facts(4, "2026-10-15T10:00:00Z", Some("\"1\"")),
            facts(3, "2026-10-15T10:00:01Z", Some("\"1\"")),
            facts(3, "2026-10-15T10:00:00Z", Some("\"2\"")),
        ] {
            let mut changed = FactsIn::new(&key);
            changed.facts = Some(&after);
            changed.previous = Some(&previous);
            assert!(chain.needs_bytes(&changed));
            changed.bytes = Some(b"new");
            let mut record = previous.clone();
            run(&chain, &mut record, &changed)?;
            assert_eq!(record.version, Some(Version::from_bytes(b"new")));
        }
        // No stored version: hash.
        let mut unversioned = previous.clone();
        unversioned.version = None;
        let mut first = FactsIn::new(&key);
        first.facts = Some(&before);
        first.previous = Some(&unversioned);
        assert!(chain.needs_bytes(&first));
        Ok(())
    }

    /// The documented limit: `cp -p` keeps size and modification time, and a backend with no ETag
    /// cannot tell the rewrite apart.
    #[test]
    fn content_hash_on_change_misses_cp_p_rewrite() -> Result<(), Error> {
        let key = parse_key("a.txt")?;
        let before = facts(3, "2026-10-15T10:00:00Z", None);
        let previous = stored(&before);
        let rewritten = facts(3, "2026-10-15T10:00:00Z", None);
        let mut f = FactsIn::new(&key);
        f.facts = Some(&rewritten);
        f.previous = Some(&previous);
        assert!(!changed_since_stored(&f));
        Ok(())
    }

    #[test]
    fn defaults_matches_today() -> Result<(), Error> {
        use crate::metadata::Metadata;
        use crate::store::{AsyncMemoryStore, AsyncStore, NoAsyncStore};
        let key = parse_key("dir/a.txt")?;
        let data = b"hello";
        let data_facts = DataFacts {
            size: data.len() as u64,
            modified: None,
            etag: None,
        };
        let _ = AsyncMemoryStore::new(&Key::new());

        // Default: today's trait default_metadata + finalize_metadata(update = false), plus the
        // warning today's file store adds.
        let mut today = Metadata::MetadataRecord(NoAsyncStore.default_metadata(&key, false));
        NoAsyncStore.finalize_metadata(&mut today, &key, data, false);
        let Metadata::MetadataRecord(mut today) = today else {
            return Err(Error::general_error("legacy".to_string()));
        };
        today.warning("New metadata has been created. (get)");

        let chain = compile("defaults", Stage::Default)?;
        let mut facts = FactsIn::new(&key);
        facts.facts = Some(&data_facts);
        facts.operation = "get";
        let mut record = MetadataRecord::new();
        run(&chain, &mut record, &facts)?;
        assert!(!record.updated.is_empty());
        let mut normalized = record.clone();
        normalized.updated = today.updated.clone();
        for (a, b) in normalized.log.iter_mut().zip(today.log.iter()) {
            a.timestamp = b.timestamp.clone();
        }
        assert_eq!(
            serde_json::to_value(&normalized).ok(),
            serde_json::to_value(&today).ok()
        );
        assert_eq!(record.status, Status::Source);

        // Write: finalize_metadata(update = true).
        let mut written = Metadata::MetadataRecord(MetadataRecord::new());
        NoAsyncStore.finalize_metadata(&mut written, &key, data, true);
        let Metadata::MetadataRecord(written) = written else {
            return Err(Error::general_error("legacy".to_string()));
        };
        let chain = compile("defaults", Stage::Write)?;
        let mut facts = FactsIn::new(&key);
        facts.bytes = Some(data);
        let mut record = MetadataRecord::new();
        run(&chain, &mut record, &facts)?;
        record.updated = written.updated.clone();
        assert_eq!(
            serde_json::to_value(&record).ok(),
            serde_json::to_value(&written).ok()
        );
        Ok(())
    }

    #[test]
    fn empty_spec_means_no_steps() -> Result<(), Error> {
        let key = parse_key("a.txt")?;
        let facts = FactsIn::new(&key);
        let chain = compile("", Stage::Read)?;
        let mut record = MetadataRecord::new();
        let before = serde_json::to_value(&record).ok();
        run(&chain, &mut record, &facts)?;
        assert_eq!(serde_json::to_value(&record).ok(), before);
        assert!(!chain.needs_bytes(&facts));
        Ok(())
    }

    #[test]
    fn registered_entry_is_named_like_a_builtin() -> Result<(), Error> {
        let (mut cmr, mut functions) = builtin();
        fn shout(stage: Stage, _p: &ResolvedParameterValues) -> Result<CompiledStep, Error> {
            crate::store::handlers::check_stage("shout", stage, &[Stage::Read])?;
            Ok(CompiledStep::new(Box::new(|record, _f, _o| {
                record.title = record.title.to_uppercase();
                Ok(())
            })))
        }
        functions.register_step(&mut cmr, CommandMetadata::new("shout"), shout)?;
        let declared = cmr
            .commands
            .iter()
            .any(|c| c.name == "shout" && c.realm == STORE_CONFIG_REALM);
        assert!(declared);
        let key = parse_key("a.txt")?;
        let mut record = MetadataRecord::new();
        run(
            &functions.compile("set-title-hi/shout", Stage::Read, &cmr)?,
            &mut record,
            &FactsIn::new(&key),
        )?;
        assert_eq!(record.title, "HI");
        // The same spelling with its realm written out.
        assert!(functions
            .compile("-store_config/set-title-hi/shout", Stage::Read, &cmr)
            .is_ok());
        Ok(())
    }

    #[test]
    fn builtin_pair_declares_every_handler() {
        let (cmr, functions) = StoreConfigFunctions::builtin();
        assert!(!functions.is_empty());
        for name in [
            "defaults",
            "set",
            "require",
            "media_type",
            "file_facts",
            "content_hash",
            "updated",
        ] {
            assert!(
                cmr.commands.iter().any(|c| c.name == name
                    && c.realm == STORE_CONFIG_REALM
                    && c.namespace.is_empty()),
                "{name}"
            );
        }
    }
}
