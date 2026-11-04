//! Planning a pipeline: [`PipelinePlanBuilder`] and [`Pipeline::to_plan`].
//!
//! Every plan entry point goes through this builder. It has two modes:
//!
//! - [`PlanningMode::Delegate`] hands each representable run of elements to the query planner
//!   ([`PlanBuilder`]) on the shared [`PlanAssembly`], and plans only what has no query
//!   spelling itself: structured values, named arguments, nested pipelines, state elements.
//! - [`PlanningMode::Native`] plans every element itself.
//!
//! A query planned in `Delegate` mode is planned exactly as the query planner plans it, so the
//! switch changes nothing until `Native` is chosen.

use std::collections::BTreeMap;

use serde_json::Value as Json;

use crate::command_metadata::{
    ArgumentInfo, CommandDefinition, CommandMetadata, CommandMetadataRegistry,
};
use crate::error::Error;
use crate::parse::{parse_key, parse_query};
use crate::plan::{
    ActionOrigin, ParameterValue, Plan, PlanAssembly, PlanBuilder, ResolvedParameterValues,
    Step, VolatilitySource,
};
use crate::query::{ActionRequest, Position, Query};
use crate::recipes::{convert_scalar, ConvertedOverride};

use super::{
    ActionElement, ElementKind, Operand, Pipeline, PipelineElement, QueryOrPipeline, Read, Source,
};

/// How [`PipelinePlanBuilder`] plans the elements that have a query spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanningMode {
    /// Representable runs are converted to their query and planned by [`PlanBuilder`].
    Delegate,
    /// Every element is planned by the pipeline planner itself.
    Native,
}

/// The mode every entry point uses. `Delegate` until the dual-mode test is green on the whole
/// corpus; switching is this one line.
pub const DEFAULT_PLANNING_MODE: PlanningMode = PlanningMode::Native;

/// Builds a [`Plan`] from a query or a pipeline (`specs/design/composite-command-definition/`).
#[derive(Debug, Clone)]
pub struct PipelinePlanBuilder<'c> {
    cmr: &'c CommandMetadataRegistry,
    mode: PlanningMode,
    allow_placeholders: bool,
    /// Placeholder values, present while planning a pipeline command's body.
    bindings: Option<BTreeMap<String, Json>>,
}

impl<'c> PipelinePlanBuilder<'c> {
    /// A builder in [`DEFAULT_PLANNING_MODE`] that refuses missing arguments and placeholders.
    pub fn new(cmr: &'c CommandMetadataRegistry) -> Self {
        PipelinePlanBuilder {
            cmr,
            mode: DEFAULT_PLANNING_MODE,
            allow_placeholders: false,
            bindings: None,
        }
    }

    /// Chooses the planning mode.
    pub fn with_mode(mut self, mode: PlanningMode) -> Self {
        self.mode = mode;
        self
    }

    /// Lets a missing required argument become a named placeholder, which a recipe override
    /// fills afterwards (as [`PlanBuilder::with_placeholders_allowed`]).
    pub fn with_placeholders_allowed(mut self) -> Self {
        self.allow_placeholders = true;
        self
    }

    /// Plans a pipeline command's body: `bindings` fill its placeholders, and the input state
    /// (`{state: true}`) may be read.
    #[allow(dead_code)] // used by pipeline command expansion
    pub(crate) fn for_definition(mut self, bindings: BTreeMap<String, Json>) -> Self {
        self.bindings = Some(bindings);
        self
    }

    /// The planning mode in use.
    pub fn mode(&self) -> PlanningMode {
        self.mode
    }

    /// Plans a query.
    pub fn plan_query(&self, query: &Query) -> Result<Plan, Error> {
        match self.mode {
            PlanningMode::Delegate => {
                let mut builder = PlanBuilder::new(query.clone(), self.cmr);
                if self.allow_placeholders {
                    builder = builder.with_placeholders_allowed();
                }
                builder.build()
            }
            PlanningMode::Native => {
                let pipeline = Pipeline::try_from(query)?;
                let mut asm = PlanAssembly::new(self.cmr);
                let top = |elements: Vec<PipelineElement>| Pipeline {
                    elements,
                    filename: None,
                    ..pipeline.clone()
                };
                let mut state = NativeState {
                    prefixes: unit_prefixes(query),
                    next: 0,
                    last_unit_steps: 0,
                };
                self.native_elements(
                    &mut asm,
                    &pipeline.realm,
                    &pipeline.elements,
                    &top,
                    &mut state,
                )?;
                if let Some(filename) = &pipeline.filename {
                    asm.push(Step::Filename(filename_step(filename)?));
                }
                if let Some(p) = recorded_predecessor(query) {
                    asm.plan.predecessor =
                        Some(crate::plan::promote_relative_default_links(&p, self.cmr)?);
                    asm.plan.predecessor_steps = state.last_unit_steps;
                }
                asm.finish(query.clone())
            }
        }
    }

    /// Plans a pipeline. A pipeline with a query form plans exactly as that query (AC-18);
    /// otherwise its elements are planned in order and the plan has no query.
    pub fn plan_pipeline(&self, pipeline: &Pipeline) -> Result<Plan, Error> {
        let pipeline = self.bound(pipeline)?;
        if let Ok(query) = pipeline.to_query_with(self.cmr) {
            return self.plan_query(&query);
        }
        let mut asm = PlanAssembly::new(self.cmr);
        self.plan_elements(&mut asm, &pipeline.realm, &pipeline.elements, true)?;
        if let Some(filename) = &pipeline.filename {
            asm.push(Step::Filename(filename_step(filename)?));
        }
        // Without a query, no prefix of this plan names an asset a cut could evaluate.
        asm.plan.predecessor = None;
        asm.plan.predecessor_steps = 0;
        asm.finish(Query::default())
    }

    /// `pipeline` with its placeholders filled; outside a definition a placeholder is an error
    /// (AC-11).
    fn bound(&self, pipeline: &Pipeline) -> Result<Pipeline, Error> {
        match &self.bindings {
            Some(bindings) => pipeline.bind(bindings),
            None => match pipeline.placeholders().into_iter().next() {
                Some(name) => Err(Error::invalid_pipeline(format!(
                    "placeholder '{name}' outside a pipeline command definition"
                ))),
                None => Ok(pipeline.clone()),
            },
        }
    }

    /// Plans `elements` of a pipeline in `realm`. `open_prefix` is whether the steps planned
    /// before them are all the steps of a query, so the first run can record its prefixes.
    fn plan_elements(
        &self,
        asm: &mut PlanAssembly<'c>,
        realm: &str,
        elements: &[PipelineElement],
        open_prefix: bool,
    ) -> Result<(), Error> {
        let mut prefix_open = open_prefix;
        let mut i = 0;
        while i < elements.len() {
            if let Some((end, query)) = self.longest_run(realm, &elements[i..]) {
                self.plan_run(asm, &query, prefix_open)?;
                i += end;
                continue;
            }
            self.plan_element(asm, realm, &elements[i])?;
            prefix_open = false;
            i += 1;
        }
        Ok(())
    }

    /// The longest representable run at the start of `elements`, as its length and query.
    fn longest_run(&self, realm: &str, elements: &[PipelineElement]) -> Option<(usize, Query)> {
        (1..=elements.len()).rev().find_map(|end| {
            let run = Pipeline {
                realm: realm.to_owned(),
                ..Pipeline::new(elements[..end].to_vec())
            };
            run.to_query_with(self.cmr).ok().map(|q| (end, q))
        })
    }

    /// Plans a representable run as a query (with the query planner in `Delegate` mode). After an element that has no query
    /// form, a run's prefixes do not name what ran, so its actions record no query.
    fn plan_run(
        &self,
        asm: &mut PlanAssembly<'c>,
        query: &Query,
        prefix_open: bool,
    ) -> Result<(), Error> {
        let before = asm.plan.steps.len();
        match self.mode {
            PlanningMode::Delegate => {
                let taken = std::mem::replace(asm, PlanAssembly::new(self.cmr));
                let (returned, result) =
                    PlanBuilder::process_on(taken, query, self.allow_placeholders);
                *asm = returned;
                result?;
            }
            PlanningMode::Native => {
                let run = Pipeline::try_from(query)?;
                let top = |elements: Vec<PipelineElement>| Pipeline {
                    elements,
                    filename: None,
                    ..run.clone()
                };
                let mut state = NativeState {
                    prefixes: unit_prefixes(query),
                    next: 0,
                    last_unit_steps: 0,
                };
                self.native_elements(asm, &run.realm, &run.elements, &top, &mut state)?;
                if let Some(filename) = &run.filename {
                    asm.push(Step::Filename(filename_step(filename)?));
                }
            }
        }
        if !prefix_open {
            for step in asm.plan.steps[before..].iter_mut() {
                if let Step::Action { query, .. } = step {
                    *query = None;
                }
            }
        }
        Ok(())
    }

    /// Plans the elements of a representable pipeline natively, as the query planner plans its
    /// query (`Native` mode). `wrap` turns a prefix of `elements` into the whole pipeline up to
    /// it, so every action records the prefix query it completes and the last one records its
    /// predecessor, as [`PlanBuilder`] does.
    fn native_elements(
        &self,
        asm: &mut PlanAssembly<'c>,
        realm: &str,
        elements: &[PipelineElement],
        wrap: &dyn Fn(Vec<PipelineElement>) -> Pipeline,
        state: &mut NativeState,
    ) -> Result<(), Error> {
        let prefix_of = |end: usize| -> Result<Query, Error> {
            wrap(elements[..end].to_vec()).to_query_with(self.cmr)
        };
        // A `q` quotes everything before it: those elements never run (the last `q` wins).
        let base = asm.plan.steps.len();
        let start = match elements.iter().rposition(|e| e.item == ElementKind::Quote) {
            Some(q) => {
                asm.plan.steps.truncate(base);
                // Quoted actions are not resolved: the quote is their text, whatever they name.
                let quoted = wrap(elements[..q].to_vec()).to_query_lenient(self.cmr)?;
                if !quoted.is_empty() {
                    asm.push(Step::UseQueryValue(quoted));
                }
                asm.plan.uncached_by = None;
                q + 1
            }
            None => 0,
        };
        for i in start..elements.len() {
            let element = &elements[i];
            let position = element.pos.clone().unwrap_or_else(Position::unknown);
            let is_unit = matches!(
                element.item,
                ElementKind::Action(_)
                    | ElementKind::Volatile
                    | ElementKind::Resource { .. }
                    | ElementKind::KeyValue(_)
                    | ElementKind::Cwd(_)
            );
            let unit_prefix = if is_unit {
                state.last_unit_steps = asm.plan.steps.len();
                state.next += 1;
                state.prefixes.get(state.next - 1).cloned()
            } else {
                None
            };
            match &element.item {
                ElementKind::Action(action) => {
                    let prefix = match unit_prefix {
                        Some(prefix) => prefix,
                        None => prefix_of(i + 1)?,
                    };
                    let prefix = crate::plan::promote_relative_default_links(&prefix, self.cmr)?;
                    self.plan_action_with_prefix(asm, realm, action, &position, Some(prefix))?;
                }
                ElementKind::Pipeline(nested) => {
                    let nested_realm = nested.realm.clone();
                    let head: Vec<PipelineElement> = elements[..i].to_vec();
                    let shell = nested.clone();
                    let inner = move |els: Vec<PipelineElement>| {
                        let mut all = head.clone();
                        all.push(crate::pipeline::Located {
                            item: ElementKind::Pipeline(Pipeline {
                                elements: els,
                                ..shell.clone()
                            }),
                            pos: None,
                        });
                        wrap(all)
                    };
                    self.native_elements(asm, &nested_realm, &nested.elements, &inner, state)?;
                }
                ElementKind::Text(text) => {
                    let parsed = Pipeline::try_from(&parse_query(text)?)?;
                    self.plan_elements(asm, realm, &parsed.elements, false)?;
                }
                ElementKind::Quote => {}
                ElementKind::Value(_)
                | ElementKind::Resource { .. }
                | ElementKind::KeyValue(_)
                | ElementKind::Cwd(_)
                | ElementKind::Evaluate(_)
                | ElementKind::QueryValue(_)
                | ElementKind::Volatile => {
                    let reads_resource = matches!(
                        element.item,
                        ElementKind::Resource { .. } | ElementKind::KeyValue(_) | ElementKind::Cwd(_)
                    );
                    if reads_resource && !realm.is_empty() {
                        // As the query planner: a resource segment's header name is not a realm.
                        asm.plan
                            .init_warning(format!("Resource header name is ignored: '{realm}'"));
                    }
                    self.plan_element(asm, realm, element)?
                }
            }
        }
        Ok(())
    }

    /// Plans one element itself.
    fn plan_element(
        &self,
        asm: &mut PlanAssembly<'c>,
        realm: &str,
        element: &PipelineElement,
    ) -> Result<(), Error> {
        let position = element.pos.clone().unwrap_or_else(Position::unknown);
        match &element.item {
            ElementKind::Text(text) => {
                let query = parse_query(text)?;
                let pipeline = Pipeline {
                    realm: realm.to_owned(),
                    ..Pipeline::try_from(&query)?
                };
                self.plan_elements(asm, &pipeline.realm, &pipeline.elements, false)
            }
            ElementKind::Action(action) => self.plan_action(asm, realm, action, &position),
            ElementKind::Value(operand) => {
                let value = literal(operand, "value")?;
                asm.push(Step::UseValue(value.clone()));
                asm.plan.uncached_by = None;
                Ok(())
            }
            ElementKind::Resource { resource, read } => {
                let key = parse_key(literal(resource, "resource")?)?;
                asm.push(read_step(*read, key));
                asm.plan.uncached_by = None;
                Ok(())
            }
            ElementKind::KeyValue(operand) => {
                asm.push(Step::UseKeyValue(parse_key(literal(operand, "key_value")?)?));
                asm.plan.uncached_by = None;
                Ok(())
            }
            ElementKind::Cwd(operand) => {
                asm.push(Step::SetCwd(parse_key(literal(operand, "cwd")?)?));
                Ok(())
            }
            ElementKind::Evaluate(operand) => {
                match literal(operand, "evaluate")? {
                    QueryOrPipeline::Query(text) => asm.push(Step::Evaluate(parse_query(text)?)),
                    QueryOrPipeline::Pipeline(p) => {
                        let p = self.bound(p)?;
                        match p.to_query_with(self.cmr) {
                            Ok(query) => asm.push(Step::Evaluate(query)),
                            Err(_) => asm.push(Step::Plan(self.evaluated_plan(&p)?)),
                        }
                    }
                }
                asm.plan.uncached_by = None;
                Ok(())
            }
            ElementKind::QueryValue(operand) => {
                let query = match literal(operand, "query_value")? {
                    QueryOrPipeline::Query(text) => parse_query(text)?,
                    QueryOrPipeline::Pipeline(p) => self.bound(p)?.to_query_with(self.cmr)?,
                };
                asm.push(Step::UseQueryValue(query));
                asm.plan.uncached_by = None;
                Ok(())
            }
            ElementKind::Quote => Err(Error::invalid_pipeline(
                "{quote: true} needs the elements before it to have a query form".to_string(),
            )
            .with_position(&position)),
            ElementKind::Volatile => {
                asm.mark_volatile("Volatile due to instruction 'v'", VolatilitySource::Positional);
                Ok(())
            }
            // A nested pipeline names its realm; an empty one is the root realm.
            ElementKind::Pipeline(nested) => {
                let nested = self.bound(nested)?;
                self.plan_elements(asm, &nested.realm, &nested.elements, false)
            }
        }
    }

    /// The plan of an evaluated pipeline with no query form: it starts from an empty state, as
    /// a query evaluated with `Evaluate` does.
    fn evaluated_plan(&self, pipeline: &Pipeline) -> Result<Plan, Error> {
        let mut asm = PlanAssembly::new(self.cmr);
        asm.push(Step::UseValue(Json::Null));
        self.plan_elements(&mut asm, &pipeline.realm, &pipeline.elements, false)?;
        asm.finish(Query::default())
    }

    /// Plans an action the query planner cannot: one with named arguments, structured values,
    /// links, nested pipelines or the input state.
    fn plan_action(
        &self,
        asm: &mut PlanAssembly<'c>,
        realm: &str,
        action: &ActionElement,
        position: &Position,
    ) -> Result<(), Error> {
        self.plan_action_with_prefix(asm, realm, action, position, None)
    }

    /// [`Self::plan_action`], recording `prefix` as the query the action completes. An action
    /// whose arguments are all positional text or links is resolved exactly as the query planner
    /// resolves it; any other goes through the argument assignment below.
    fn plan_action_with_prefix(
        &self,
        asm: &mut PlanAssembly<'c>,
        realm: &str,
        action: &ActionElement,
        position: &Position,
        prefix: Option<Query>,
    ) -> Result<(), Error> {
        let explicit = action.ns.as_ref().map(|n| n.0.clone()).unwrap_or_default();
        let namespaces = self.cmr.lookup_namespaces(&explicit);
        let textual = query_parameters(action);
        let request = ActionRequest {
            name: action.action.clone(),
            parameters: textual.clone().unwrap_or_default(),
            position: position.clone(),
        };
        let allow = textual.is_none() || self.allow_placeholders;
        let metadata = self
            .cmr
            .find_command_in_namespaces(realm, &namespaces, &action.action)
            .ok_or_else(|| {
                Error::action_not_registered(&request, realm, &namespaces).with_position(position)
            })?
            .resolve_global_enums(self.cmr)?;

        // `uncached_by` describes the last action only: this action's commands decide it afresh.
        asm.plan.uncached_by = None;
        let key = metadata.key();
        asm.apply_command_flags(&key);

        let (mut parameters, step_key, origin) = match &metadata.definition {
            CommandDefinition::Registered => (
                ResolvedParameterValues::from_action(&request, &metadata, allow)?,
                key.clone(),
                ActionOrigin::Direct,
            ),
            CommandDefinition::Alias {
                command,
                head_parameters,
            } => {
                let target = self
                    .cmr
                    .alias_target(&metadata)?
                    .ok_or_else(|| Error::alias_target_not_registered(&key, command))?;
                asm.plan
                    .init_info(format!("Command '{}' is an alias of '{}'", key, command));
                asm.apply_command_flags(command);
                (
                    ResolvedParameterValues::from_alias_action(
                        &request,
                        &metadata,
                        &target,
                        head_parameters,
                        allow,
                    )?,
                    command.clone(),
                    ActionOrigin::Alias { command: key.clone() },
                )
            }
        };

        let assigned = match textual {
            Some(_) => Vec::new(),
            None => assign_arguments(&metadata, action, position)?,
        };
        for (argument, sources, pos) in assigned {
            let value = self.argument_value(asm, argument, &sources, &pos)?;
            set_parameter(&mut parameters, &argument.name, value);
        }
        if !self.allow_placeholders {
            for (i, parameter) in parameters.0.iter().enumerate() {
                match parameter {
                    ParameterValue::Placeholder(name) => {
                        return Err(Error::missing_argument(i, name, position)
                            .with_command_key(&key));
                    }
                    ParameterValue::None => {
                        let name = metadata
                            .arguments
                            .get(i)
                            .map(|a| a.name.clone())
                            .unwrap_or_default();
                        return Err(Error::missing_argument(i, &name, position)
                            .with_command_key(&key));
                    }
                    ParameterValue::DefaultValue(_, _)
                    | ParameterValue::DefaultLink(_, _)
                    | ParameterValue::ParameterValue(_, _, _)
                    | ParameterValue::ParameterLink(_, _, _)
                    | ParameterValue::OverrideValue(_, _)
                    | ParameterValue::OverrideLink(_, _)
                    | ParameterValue::EnumLink(_, _, _)
                    | ParameterValue::MultipleParameters(_, _)
                    | ParameterValue::Injected(_)
                    | ParameterValue::ParameterPipeline(_, _, _)
                    | ParameterValue::OverridePipeline(_, _)
                    | ParameterValue::DefaultPipeline(_, _)
                    | ParameterValue::InputState(_) => {}
                }
            }
        }
        asm.check_parameters_for_volatile_links(&parameters)?;
        asm.push(Step::Action {
            realm: step_key.realm.clone(),
            ns: step_key.namespace.clone(),
            action_name: step_key.name.clone(),
            position: position.clone(),
            parameters,
            origin,
            query: prefix,
        });
        Ok(())
    }

    /// The parameter value of one argument from its sources.
    fn argument_value(
        &self,
        asm: &mut PlanAssembly<'c>,
        argument: &ArgumentInfo,
        sources: &[Source],
        position: &Position,
    ) -> Result<ParameterValue, Error> {
        if argument.multiple {
            let mut element = argument.clone();
            element.multiple = false;
            let mut values = Vec::new();
            for source in sources {
                match source {
                    Source::Multiple(items) => {
                        for item in items {
                            values.push(self.single_value(asm, &element, item, position)?);
                        }
                    }
                    Source::Value(Json::Array(items)) => {
                        for item in items {
                            values.push(self.single_value(
                                asm,
                                &element,
                                &Source::Value(item.clone()),
                                position,
                            )?);
                        }
                    }
                    Source::Value(_)
                    | Source::Placeholder(_)
                    | Source::Link(_)
                    | Source::Pipeline(_)
                    | Source::State(_) => {
                        values.push(self.single_value(asm, &element, source, position)?)
                    }
                }
            }
            return Ok(ParameterValue::MultipleParameters(argument.name.clone(), values));
        }
        match sources {
            [source] => self.single_value(asm, argument, source, position),
            _ => Err(Error::invalid_argument(
                "",
                &argument.name,
                "takes one value, but several were given",
            )
            .with_position(position)),
        }
    }

    /// The parameter value of one source for a single (not variadic) argument.
    fn single_value(
        &self,
        asm: &mut PlanAssembly<'c>,
        argument: &ArgumentInfo,
        source: &Source,
        position: &Position,
    ) -> Result<ParameterValue, Error> {
        let name = argument.name.clone();
        match source {
            Source::Value(value) => match convert_scalar(argument, value)? {
                ConvertedOverride::Value(v, note) => {
                    if let Some(note) = note {
                        asm.plan.init_info(note);
                    }
                    Ok(ParameterValue::ParameterValue(name, v, position.clone()))
                }
                ConvertedOverride::Link(q, note) => {
                    if let Some(note) = note {
                        asm.plan.init_info(note);
                    }
                    Ok(ParameterValue::EnumLink(name, q, position.clone()))
                }
                ConvertedOverride::Pipeline(p, note) => {
                    if let Some(note) = note {
                        asm.plan.init_info(note);
                    }
                    Ok(ParameterValue::ParameterPipeline(name, p, position.clone()))
                }
            },
            Source::Link(text) => Ok(ParameterValue::ParameterLink(
                name,
                parse_query(text)?,
                position.clone(),
            )),
            Source::Pipeline(p) => {
                let p = self.bound(p)?;
                match p.to_query_with(self.cmr) {
                    Ok(query) => Ok(ParameterValue::ParameterLink(name, query, position.clone())),
                    Err(_) => Ok(ParameterValue::ParameterPipeline(name, p, position.clone())),
                }
            }
            Source::State(true) if self.bindings.is_some() => Ok(ParameterValue::InputState(name)),
            Source::State(true) => Err(Error::invalid_pipeline(format!(
                "argument '{name}' reads the input state, which only a pipeline command has"
            ))
            .with_position(position)),
            Source::State(false) => Err(Error::invalid_pipeline(format!(
                "argument '{name}': {{state: false}} is not a source"
            ))
            .with_position(position)),
            Source::Placeholder(placeholder) => Err(Error::invalid_pipeline(format!(
                "placeholder '{placeholder}' outside a pipeline command definition"
            ))
            .with_position(position)),
            Source::Multiple(_) => Err(Error::invalid_argument(
                "",
                &name,
                "is not variadic, so it takes no {multiple: …}",
            )
            .with_position(position)),
        }
    }
}

/// The resource name of a pipeline's `filename`, checked as a query filename is (P2).
fn filename_step(filename: &str) -> Result<crate::query::ResourceName, Error> {
    if !crate::escape::is_filename(filename) {
        return Err(Error::invalid_token(filename, "filename"));
    }
    Ok(crate::query::ResourceName::new(filename.to_owned()))
}

/// The step that reads `key` as `read` says.
fn read_step(read: Read, key: crate::query::Key) -> Step {
    match read {
        Read::Asset => Step::GetAsset(key),
        Read::Binary => Step::GetAssetBinary(key),
        Read::Metadata => Step::GetAssetMetadata(key),
        Read::Recipe => Step::GetAssetRecipe(key),
        Read::Directory => Step::GetAssetDirectory(key),
        Read::Stored => Step::GetResource(key),
        Read::StoredMetadata => Step::GetResourceMetadata(key),
        Read::StoredDirectory => Step::GetResourceDirectory(key),
    }
}

/// A literal operand; a placeholder left here was not bound.
fn literal<'a, T>(operand: &'a Operand<T>, what: &str) -> Result<&'a T, Error> {
    match operand {
        Operand::Literal(value) => Ok(value),
        Operand::Placeholder(name) => Err(Error::invalid_pipeline(format!(
            "placeholder '{name}' in a {what} element outside a pipeline command definition"
        ))),
    }
}

/// What native planning carries across nested pipelines.
struct NativeState {
    /// The prefix query of each unit (action, `v`, resource segment), in order.
    prefixes: Vec<Query>,
    next: usize,
    /// The number of steps planned before the last unit.
    last_unit_steps: usize,
}

/// Whether a query's last segment ends in an action that is a unit of the plan (not `ns`).
fn remainder_is_unit(remainder: &Option<crate::query::QuerySegment>) -> Option<bool> {
    use crate::query::QuerySegment;
    match remainder {
        Some(QuerySegment::Resource(_)) => Some(true),
        Some(QuerySegment::Transform(t)) => match t.action() {
            Some(a) => Some(!a.is_ns()),
            None => Some(false),
        },
        None => None,
    }
}

/// The prefix query of each unit of `query`, in order: the query cut after that unit, as the
/// query planner's recursion sees it.
fn unit_prefixes(query: &Query) -> Vec<Query> {
    let mut out = Vec::new();
    let mut current = query.clone();
    loop {
        if current.is_empty() || current.is_ns() || current.is_q() {
            break;
        }
        if current.resource_query().is_some() {
            out.push(current.clone());
            break;
        }
        if let Some(transform) = current.transform_query() {
            if let Some(action) = transform.action() {
                if !action.is_ns() {
                    out.push(current.clone());
                }
                break;
            }
            if transform.is_filename() {
                break;
            }
        }
        let (p, r) = current.predecessor();
        match remainder_is_unit(&r) {
            Some(true) => out.push(current.clone()),
            Some(false) => {}
            None => break,
        }
        match p {
            Some(p) => current = p,
            None => break,
        }
    }
    out.reverse();
    out
}

/// The predecessor the query planner records for `query`: the outermost split whose remainder
/// is an action or a resource segment, unless its prefix is empty or ends with `q`.
fn recorded_predecessor(query: &Query) -> Option<Query> {
    if query.is_empty() || query.is_ns() || query.is_q() || query.resource_query().is_some() {
        return None;
    }
    if let Some(transform) = query.transform_query() {
        if transform.action().is_some() || transform.is_filename() {
            return None;
        }
    }
    let (p, r) = query.predecessor();
    let p = p.filter(|p| !p.is_empty())?;
    if p.is_q() {
        return None;
    }
    match remainder_is_unit(&r) {
        Some(true) => Some(p),
        Some(false) => {
            // An `ns` remainder still counts as an action for the record.
            match &r {
                Some(crate::query::QuerySegment::Transform(t)) if t.action().is_some() => Some(p),
                _ => recorded_predecessor(&p),
            }
        }
        None => None,
    }
}

/// An action's parameters as query parameters, when it has only positional text and links:
/// such an action is resolved exactly as the query planner resolves it.
fn query_parameters(action: &ActionElement) -> Option<Vec<crate::query::ActionParameter>> {
    use crate::query::ActionParameter;
    if !action.arguments.is_empty() {
        return None;
    }
    action
        .parameters
        .iter()
        .map(|p| {
            let pos = p.pos.clone().unwrap_or_else(Position::unknown);
            match &p.item {
                Source::Value(Json::String(text)) => Some(ActionParameter::String(text.clone(), pos)),
                Source::Link(text) => parse_query(text).ok().map(|q| ActionParameter::Link(q, pos)),
                Source::Value(_)
                | Source::Placeholder(_)
                | Source::Pipeline(_)
                | Source::State(_)
                | Source::Multiple(_) => None,
            }
        })
        .collect()
}

/// Replaces the parameter named `name`.
fn set_parameter(parameters: &mut ResolvedParameterValues, name: &str, value: ParameterValue) {
    for parameter in parameters.0.iter_mut() {
        if parameter.name().as_deref() == Some(name) {
            *parameter = value;
            return;
        }
    }
}

/// Assigns an action's sources to its command's arguments: positional parameters fill the
/// arguments that consume query parameters, in order (from a variadic one on, every remaining
/// source is one of its elements); named arguments fill the rest (V1, V2).
fn assign_arguments<'m>(
    metadata: &'m CommandMetadata,
    action: &ActionElement,
    position: &Position,
) -> Result<Vec<(&'m ArgumentInfo, Vec<Source>, Position)>, Error> {
    let slots: Vec<&ArgumentInfo> = metadata.arguments.iter().filter(|a| !a.injected).collect();
    let mut assigned: Vec<(&ArgumentInfo, Vec<Source>, Position)> = Vec::new();
    let mut parameters = action.parameters.iter();
    let mut taken = 0;
    for slot in slots.iter() {
        if slot.multiple {
            let rest: Vec<_> = parameters.by_ref().collect();
            if !rest.is_empty() {
                let pos = rest[0].pos.clone().unwrap_or_else(|| position.clone());
                assigned.push((slot, rest.into_iter().map(|a| a.item.clone()).collect(), pos));
            }
            break;
        }
        match parameters.next() {
            Some(argument) => {
                let pos = argument.pos.clone().unwrap_or_else(|| position.clone());
                assigned.push((slot, vec![argument.item.clone()], pos));
                taken += 1;
            }
            None => break,
        }
    }
    if let Some(excess) = parameters.next() {
        return Err(Error::too_many_parameters(
            &format!("command '{}'", metadata.name),
            slots.len(),
            taken + 1,
            &excess.item.to_json().to_string(),
            &excess.pos.clone().unwrap_or_else(|| position.clone()),
        ));
    }
    for (name, argument) in action.arguments.iter() {
        let Some(slot) = slots.iter().find(|s| &s.name == name) else {
            return Err(Error::invalid_argument(
                &metadata.name,
                name,
                "the command has no such argument",
            )
            .with_position(position));
        };
        if assigned.iter().any(|(a, _, _)| &a.name == name) {
            return Err(Error::invalid_argument(
                &metadata.name,
                name,
                "is given both by position and by name",
            )
            .with_position(position));
        }
        let pos = argument.pos.clone().unwrap_or_else(|| position.clone());
        assigned.push((slot, vec![argument.item.clone()], pos));
    }
    Ok(assigned)
}

impl Pipeline {
    /// Plans the pipeline with [`PipelinePlanBuilder`] in its default mode (AC-18).
    pub fn to_plan(&self, cmr: &CommandMetadataRegistry) -> Result<Plan, Error> {
        PipelinePlanBuilder::new(cmr).plan_pipeline(self)
    }
}

#[cfg(test)]
mod tests;
