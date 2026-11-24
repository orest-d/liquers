//! Editing pipelines and queries through the command registry.
//!
//! [`PipelineEditor`] holds a pipeline and an `Arc<CommandMetadataRegistry>`. Every edit is
//! checked before it applies, and a refused edit leaves the pipeline unchanged (AC-31): token
//! classes, command and namespace lookup, argument names, `injected` arguments, literal types, and
//! placeholders (refused unless the editor is for a command definition, AC-11).
//!
//! [`QueryEditor`] is a pipeline editor that also keeps its pipeline representable as a query:
//! an edit that would break a representability rule is refused (AC-32), so
//! [`QueryEditor::query`] always succeeds. After each edit it re-encodes and re-parses the query,
//! so element positions and error positions point into the current query (AC-41).
//!
//! [`ActionEditor`] is the control of one action, with no user interface: set, link, reset an
//! argument to its default, insert, remove and move the items of a variadic argument, and list
//! the parameters beyond what the command takes ([`ActionEditor::excess`], which the query editor
//! keeps). [`PipelineEditor::namespace_impact`] and [`PipelineEditor::set_namespaces`] make `ns`
//! edits safe: an edit that keeps every action on its command applies, one that moves an action to
//! another command needs acceptance ([`NamespaceImpact::Changed`]), one that breaks the query is
//! refused.
//!
//! Value accessors use both: the query steps of [`crate::accessor::steps`] read an action or an
//! argument ([`PipelineEditor::argument_source`]) and write a part back through these edits, so an
//! edit the registry refuses is refused before anything is written to where the query lives.
//!
//! ```
//! use std::sync::Arc;
//! use liquers_core::command_metadata::{ArgumentInfo, CommandMetadata, CommandMetadataRegistry};
//! use liquers_core::pipeline::QueryEditor;
//!
//! let mut cmr = CommandMetadataRegistry::new();
//! let mut cm = CommandMetadata::new("is_in");
//! cm.with_namespace("pl")
//!     .with_argument(ArgumentInfo::string_argument("column"))
//!     .with_argument(ArgumentInfo::string_argument("values").set_multiple());
//! cmr.add_command(&cm);
//!
//! let mut q = QueryEditor::new(Arc::new(cmr));
//! q.resource("data/sales.csv", Default::default())?;
//! q.action("is_in")?.ns("pl")?.set("column", "region")?.push("EU")?.push("US")?;
//! assert_eq!(q.query().encode(), "-R/data/sales.csv/-/ns-pl/is_in-region-EU-US");
//! # Ok::<(), liquers_core::error::Error>(())
//! ```

use std::collections::BTreeSet;
use std::sync::Arc;

use serde_json::Value as Json;

use crate::command_metadata::{
    ArgumentInfo, ArgumentType, CommandKey, CommandMetadata, CommandMetadataRegistry,
};
use crate::error::Error;
use crate::escape::{is_filename, is_identifier, is_realm_name, is_resource_name};
use crate::parse::{parse_key, parse_query};
use crate::query::{Query, NS_INSTRUCTION, Q_INSTRUCTION, V_INSTRUCTION};

use super::convert::{check_argument_names, positional_arguments};
use super::{
    ActionElement, ArgRef, ElementKind, Located, Namespaces, Operand, Pipeline,
    PipelineElement, QueryOrPipeline, Read, Source,
};

/// Edits a pipeline, checking each edit against the registry.
#[derive(Debug, Clone)]
pub struct PipelineEditor {
    pipeline: Pipeline,
    cmr: Arc<CommandMetadataRegistry>,
    /// The query the editor was opened from, or (for a query editor) the current query.
    origin: Option<Query>,
    default_ns: Option<Namespaces>,
    /// Declared placeholder names; `None` outside a command definition, where placeholders and
    /// the input state are refused.
    placeholders: Option<BTreeSet<String>>,
    /// Keep the pipeline representable as a query (the query editor).
    query_mode: bool,
}

/// What an `ns` edit would do to the actions it covers (AC-25 of `query-pipeline-editor`).
#[derive(Debug, Clone, PartialEq)]
pub enum NamespaceImpact {
    /// Every action resolves to the same command.
    Same,
    /// Every action still resolves and its arguments fit, but these resolve to another command.
    Changed(Vec<CommandChange>),
    /// An action no longer resolves, or its arguments no longer fit.
    Invalid(Error),
}

/// One action that an `ns` edit makes resolve to another command.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandChange {
    /// The element index of the action.
    pub at: usize,
    pub before: CommandKey,
    pub after: CommandKey,
}

impl PipelineEditor {
    /// An empty pipeline.
    pub fn new(cmr: Arc<CommandMetadataRegistry>) -> Self {
        PipelineEditor {
            pipeline: Pipeline::default(),
            cmr,
            origin: None,
            default_ns: None,
            placeholders: None,
            query_mode: false,
        }
    }

    /// The pipeline of `query`, with each text parameter typed by its argument (S1).
    pub fn from_query(query: &Query, cmr: Arc<CommandMetadataRegistry>) -> Result<Self, Error> {
        let pipeline = Pipeline::try_from(query)
            .map_err(|e| e.with_query(query))?
            .typed_with(&cmr);
        Ok(PipelineEditor {
            pipeline,
            origin: Some(query.clone()),
            ..PipelineEditor::new(cmr)
        })
    }

    /// Edits `pipeline`, which is checked first.
    pub fn from_pipeline(pipeline: Pipeline, cmr: Arc<CommandMetadataRegistry>) -> Result<Self, Error> {
        let editor = PipelineEditor {
            pipeline,
            ..PipelineEditor::new(cmr)
        };
        editor.check_pipeline(&editor.pipeline, true)?;
        Ok(editor)
    }

    /// An editor for the body of a pipeline command whose arguments are `arguments`: their names
    /// are the placeholders it accepts, and the input state is allowed.
    pub fn for_definition(cmr: Arc<CommandMetadataRegistry>, arguments: &[ArgumentInfo]) -> Self {
        PipelineEditor {
            placeholders: Some(arguments.iter().map(|a| a.name.clone()).collect()),
            ..PipelineEditor::new(cmr)
        }
    }

    /// The pipeline being edited.
    pub fn pipeline(&self) -> &Pipeline {
        &self.pipeline
    }
    /// Takes the pipeline out of the editor.
    pub fn into_pipeline(self) -> Pipeline {
        self.pipeline
    }
    /// The registry the editor checks against.
    pub fn registry(&self) -> &Arc<CommandMetadataRegistry> {
        &self.cmr
    }
    /// The query the editor was opened from (for a query editor: the current query).
    pub fn origin(&self) -> Option<&Query> {
        self.origin.as_ref()
    }

    /// The namespaces later [`PipelineEditor::action`] calls give their actions.
    pub fn default_ns(&mut self, ns: Option<Namespaces>) -> &mut Self {
        self.default_ns = ns;
        self
    }

    // -----------------------------------------------------------------------------------------
    // Structure
    // -----------------------------------------------------------------------------------------

    /// Appends an element.
    pub fn push(&mut self, element: PipelineElement) -> Result<&mut Self, Error> {
        self.check_element(&element, &self.pipeline.realm.clone(), false)?;
        self.apply(|p| {
            p.elements.push(element);
            Ok(())
        })
    }
    /// Inserts an element before `at`.
    pub fn insert(&mut self, at: usize, element: PipelineElement) -> Result<&mut Self, Error> {
        if at > self.pipeline.elements.len() {
            return Err(self.out_of_range(at));
        }
        self.check_element(&element, &self.pipeline.realm.clone(), false)?;
        self.apply(|p| {
            p.elements.insert(at, element);
            Ok(())
        })
    }
    /// Replaces the element at `at`.
    pub fn replace(&mut self, at: usize, element: PipelineElement) -> Result<&mut Self, Error> {
        if at >= self.pipeline.elements.len() {
            return Err(self.out_of_range(at));
        }
        self.check_element(&element, &self.pipeline.realm.clone(), false)?;
        self.apply(|p| {
            p.elements[at] = element;
            Ok(())
        })
    }
    /// Removes the element at `at`.
    pub fn remove(&mut self, at: usize) -> Result<&mut Self, Error> {
        if at >= self.pipeline.elements.len() {
            return Err(self.out_of_range(at));
        }
        self.apply(|p| {
            p.elements.remove(at);
            Ok(())
        })
    }
    /// Moves the element at `from` to position `to`.
    pub fn move_to(&mut self, from: usize, to: usize) -> Result<&mut Self, Error> {
        let len = self.pipeline.elements.len();
        if from >= len {
            return Err(self.out_of_range(from));
        }
        if to >= len {
            return Err(self.out_of_range(to));
        }
        self.apply(|p| {
            let e = p.elements.remove(from);
            p.elements.insert(to, e);
            Ok(())
        })
    }
    /// Sets the pipeline's realm.
    pub fn set_realm(&mut self, realm: &str) -> Result<&mut Self, Error> {
        if !is_realm_name(realm) {
            return Err(self.annotate(Error::invalid_token(realm, "a realm name"), None));
        }
        let realm = realm.to_owned();
        self.apply(|p| {
            p.realm = realm;
            Ok(())
        })
    }
    /// Sets or clears the output filename.
    pub fn set_filename(&mut self, name: Option<&str>) -> Result<&mut Self, Error> {
        if let Some(name) = name {
            if !is_filename(name) {
                return Err(self.annotate(Error::invalid_token(name, "a filename"), None));
            }
        }
        let name = name.map(str::to_owned);
        self.apply(|p| {
            p.filename = name;
            Ok(())
        })
    }

    // -----------------------------------------------------------------------------------------
    // Shortcuts
    // -----------------------------------------------------------------------------------------

    /// Appends a resource read of `key`.
    pub fn resource(&mut self, key: &str, read: Read) -> Result<&mut Self, Error> {
        self.push(Located::new(ElementKind::Resource {
            resource: Operand::Literal(key.to_owned()),
            read,
        }))
    }

    /// Appends an action named `name` with the editor's default namespaces and returns its
    /// editor. Only the name is checked here: the command is looked up when first needed, so
    /// `.ns(…)` can follow.
    pub fn action(&mut self, name: &str) -> Result<ActionEditor<'_>, Error> {
        check_action_name(name).map_err(|e| self.annotate(e, None))?;
        let mut element = ActionElement::new(name);
        element.ns = self.default_ns.clone();
        self.apply(|p| {
            p.elements.push(Located::new(ElementKind::Action(element)));
            Ok(())
        })?;
        let index = self.pipeline.elements.len() - 1;
        Ok(ActionEditor { editor: self, index })
    }

    /// The editor of the action at `at`.
    pub fn edit_action(&mut self, at: usize) -> Result<ActionEditor<'_>, Error> {
        self.action_at(at)?;
        Ok(ActionEditor {
            editor: self,
            index: at,
        })
    }

    // -----------------------------------------------------------------------------------------
    // Registry access (AC-37)
    // -----------------------------------------------------------------------------------------

    /// The command the action at `at` resolves to.
    pub fn command_metadata(&self, at: usize) -> Result<&CommandMetadata, Error> {
        let action = self.action_at(at)?;
        self.lookup(action).map_err(|e| self.annotate(e, Some(at)))
    }
    /// The key of the command the action at `at` resolves to.
    pub fn command_key(&self, at: usize) -> Result<CommandKey, Error> {
        Ok(self.command_metadata(at)?.key())
    }
    /// An argument of the action at `at`, by position (counting only arguments a query
    /// parameter fills) or by name.
    pub fn argument_info(&self, at: usize, arg: impl Into<ArgRef>) -> Result<&ArgumentInfo, Error> {
        let md = self.command_metadata(at)?;
        argument_info_of(md, &arg.into()).map_err(|e| self.annotate(e, Some(at)))
    }
    /// What the action at `at` gives for an argument (by position or by name), or `None` when it
    /// gives nothing. A variadic argument given by position is the [`Source::Multiple`] of its
    /// parameters.
    pub fn argument_source(&self, at: usize, arg: impl Into<ArgRef>) -> Result<Option<Source>, Error> {
        let action = self.action_at(at)?;
        let md = self.command_metadata(at)?;
        let arg = arg.into();
        let info = argument_info_of(md, &arg).map_err(|e| self.annotate(e, Some(at)))?;
        if let Some(given) = action.arguments.get(&info.name) {
            return Ok(Some(given.item.clone()));
        }
        let slots = positional_arguments(md);
        let Some(slot) = slots.iter().position(|s| s.name == info.name) else {
            return Ok(None);
        };
        if info.multiple {
            let items: Vec<Source> = action
                .parameters
                .iter()
                .skip(slot)
                .map(|p| p.item.clone())
                .collect();
            return Ok(if items.is_empty() { None } else { Some(Source::Multiple(items)) });
        }
        Ok(action.parameters.get(slot).map(|p| p.item.clone()))
    }
    /// The arguments of the action at `at` that are given neither by position nor by name.
    pub fn unset_arguments(&self, at: usize) -> Result<Vec<&ArgumentInfo>, Error> {
        let action = self.action_at(at)?;
        let md = self.command_metadata(at)?;
        let slots = positional_arguments(md);
        let variadic_at = slots.iter().position(|s| s.multiple);
        Ok(slots
            .into_iter()
            .enumerate()
            .filter(|(i, s)| {
                let by_position = match variadic_at {
                    Some(v) if *i >= v => action.parameters.len() > v,
                    Some(_) | None => action.parameters.len() > *i,
                };
                !by_position && !action.arguments.contains_key(&s.name)
            })
            .map(|(_, s)| s)
            .collect())
    }
    /// The namespaces the action at `at` is looked up in, in order.
    pub fn resolved_namespaces(&self, at: usize) -> Result<Vec<String>, Error> {
        let action = self.action_at(at)?;
        let explicit = action.ns.as_ref().map(|n| n.0.clone()).unwrap_or_default();
        Ok(self.cmr.lookup_namespaces(&explicit))
    }

    /// The actions an `ns` at action `at` covers: the run of actions around `at` that share its
    /// namespaces, with `v` directives between them (a `q`, a resource or any other element ends a
    /// run). In a query this is the stretch an `ns-…` directive governs.
    pub fn namespace_run(&self, at: usize) -> Result<std::ops::Range<usize>, Error> {
        let ns = self.action_at(at)?.ns.clone();
        let elements = &self.pipeline.elements;
        let joins = |i: usize| match &elements[i].item {
            ElementKind::Action(a) => a.ns == ns,
            ElementKind::Volatile => true,
            ElementKind::Text(_)
            | ElementKind::Value(_)
            | ElementKind::Resource { .. }
            | ElementKind::KeyValue(_)
            | ElementKind::Cwd(_)
            | ElementKind::Evaluate(_)
            | ElementKind::QueryValue(_)
            | ElementKind::Quote
            | ElementKind::Pipeline(_) => false,
        };
        let mut start = at;
        while start > 0 && joins(start - 1) {
            start -= 1;
        }
        let mut end = at + 1;
        while end < elements.len() && joins(end) {
            end += 1;
        }
        // A run starts and ends with an action.
        while !matches!(elements[start].item, ElementKind::Action(_)) {
            start += 1;
        }
        while !matches!(elements[end - 1].item, ElementKind::Action(_)) {
            end -= 1;
        }
        Ok(start..end)
    }

    /// Every namespace with commands in the pipeline's realm (AC-16).
    pub fn available_namespaces(&self, at: usize) -> Result<Vec<String>, Error> {
        self.action_at(at)?;
        Ok(self.cmr.namespaces(&self.pipeline.realm))
    }

    /// The available namespaces not yet in the `ns` list of the run at `at` (AC-16).
    pub fn unused_namespaces(&self, at: usize) -> Result<Vec<String>, Error> {
        let used = self.action_at(at)?.ns.clone().map(|n| n.0).unwrap_or_default();
        Ok(self
            .available_namespaces(at)?
            .into_iter()
            .filter(|n| !used.contains(n))
            .collect())
    }

    /// What setting the `ns` list of the run at `at` to `namespaces` would do (AC-25): `Same` when
    /// every action of the run resolves to the same command, `Changed` when every one still
    /// resolves and its arguments still fit but some resolve to another command, `Invalid` when an
    /// action no longer resolves or its arguments no longer fit. An action that did not resolve
    /// before counts as unchanged when it resolves now.
    pub fn namespace_impact(&self, at: usize, namespaces: &[String]) -> Result<NamespaceImpact, Error> {
        let run = self.namespace_run(at)?;
        let realm = self.pipeline.realm.clone();
        let new_ns = if namespaces.is_empty() {
            None
        } else {
            Some(Namespaces(namespaces.to_vec()))
        };
        let mut copy = self.pipeline.clone();
        let mut changes = Vec::new();
        for i in run {
            let ElementKind::Action(before) = &self.pipeline.elements[i].item else {
                continue;
            };
            let mut after = before.clone();
            after.ns = new_ns.clone();
            let before_key = self.lookup(before).map(|md| md.key()).ok();
            let after_key = match self.lookup(&after) {
                Ok(md) => md.key(),
                Err(e) => return Ok(NamespaceImpact::Invalid(self.annotate(e, Some(i)))),
            };
            if let Err(e) = self.check_action(&after, &realm, false) {
                return Ok(NamespaceImpact::Invalid(self.annotate(e, Some(i))));
            }
            match before_key {
                Some(before_key) if before_key != after_key => changes.push(CommandChange {
                    at: i,
                    before: before_key,
                    after: after_key,
                }),
                Some(_) | None => {}
            }
            copy.elements[i].item = ElementKind::Action(after);
        }
        if self.query_mode {
            if let Err(e) = copy.to_query_lenient(&self.cmr) {
                return Ok(NamespaceImpact::Invalid(self.annotate(e, None)));
            }
        }
        Ok(if changes.is_empty() {
            NamespaceImpact::Same
        } else {
            NamespaceImpact::Changed(changes)
        })
    }

    /// Sets the `ns` list of the run at `at` (AC-25). `Same` is applied; `Changed` is applied only
    /// with `accept_changes`, and otherwise returned with nothing changed; `Invalid` is refused
    /// with its error.
    pub fn set_namespaces(
        &mut self,
        at: usize,
        namespaces: Vec<String>,
        accept_changes: bool,
    ) -> Result<NamespaceImpact, Error> {
        let impact = self.namespace_impact(at, &namespaces)?;
        match &impact {
            NamespaceImpact::Invalid(e) => return Err(e.clone()),
            NamespaceImpact::Changed(_) if !accept_changes => return Ok(impact),
            NamespaceImpact::Changed(_) | NamespaceImpact::Same => {}
        }
        let run = self.namespace_run(at)?;
        let new_ns = if namespaces.is_empty() {
            None
        } else {
            Some(Namespaces(namespaces))
        };
        self.apply(|p| {
            for i in run {
                if let ElementKind::Action(a) = &mut p.elements[i].item {
                    a.ns = new_ns.clone();
                }
            }
            Ok(())
        })?;
        Ok(impact)
    }

    /// Checks the whole pipeline: every action resolves and its arguments fit.
    pub fn validate(&self) -> Result<(), Error> {
        self.check_pipeline(&self.pipeline, true)?;
        for (i, e) in self.pipeline.elements.iter().enumerate() {
            if let ElementKind::Action(a) = &e.item {
                let md = self.lookup(a).map_err(|e| self.annotate(e, Some(i)))?;
                check_argument_names(a, md).map_err(|e| self.annotate(e, Some(i)))?;
            }
        }
        if self.query_mode {
            self.pipeline.to_query_with(&self.cmr).map_err(|e| self.annotate(e, None))?;
        }
        Ok(())
    }

    // -----------------------------------------------------------------------------------------
    // Internals
    // -----------------------------------------------------------------------------------------

    fn action_at(&self, at: usize) -> Result<&ActionElement, Error> {
        match self.pipeline.elements.get(at).map(|e| &e.item) {
            Some(ElementKind::Action(a)) => Ok(a),
            Some(_) => Err(self.annotate(
                Error::invalid_pipeline(format!("element {at} is not an action")),
                Some(at),
            )),
            None => Err(self.out_of_range(at)),
        }
    }

    fn out_of_range(&self, at: usize) -> Error {
        self.annotate(
            Error::invalid_pipeline(format!(
                "element {at} is out of range (the pipeline has {})",
                self.pipeline.elements.len()
            )),
            None,
        )
    }

    /// The command `action` names, in the pipeline's realm, by reference into the registry.
    fn lookup(&self, action: &ActionElement) -> Result<&CommandMetadata, Error> {
        let explicit = action.ns.as_ref().map(|n| n.0.clone()).unwrap_or_default();
        let namespaces = self.cmr.lookup_namespaces(&explicit);
        let realm = &self.pipeline.realm;
        namespaces
            .iter()
            .find_map(|ns| self.cmr.get(CommandKey::new(realm, ns, &action.action)))
            .ok_or_else(|| {
                Error::action_not_registered(
                    &crate::query::ActionRequest::new(action.action.clone()),
                    realm,
                    &namespaces,
                )
            })
    }

    /// Adds the element's position and the query to an error (AC-41).
    fn annotate(&self, error: Error, at: Option<usize>) -> Error {
        let error = match at
            .and_then(|i| self.pipeline.elements.get(i))
            .and_then(|e| e.pos.clone())
        {
            Some(pos) if error.position.is_unknown() => error.with_position(&pos),
            Some(_) | None => error,
        };
        match &self.origin {
            Some(q) if error.query.is_none() => error.with_query(q),
            Some(_) | None => error,
        }
    }

    /// Applies `edit` to a copy, checks it, and keeps it only if the checks pass.
    fn apply(&mut self, edit: impl FnOnce(&mut Pipeline) -> Result<(), Error>) -> Result<&mut Self, Error> {
        let mut copy = self.pipeline.clone();
        edit(&mut copy).map_err(|e| self.annotate(e, None))?;
        if self.query_mode {
            let query = copy
                .to_query_lenient(&self.cmr)
                .map_err(|e| self.annotate(e, None))?;
            // Re-parse, so positions point into the current query (AC-41).
            let reparsed = parse_query(&query.encode()).map_err(|e| self.annotate(e, None))?;
            self.pipeline = Pipeline::try_from(&reparsed)?.typed_with(&self.cmr);
            self.origin = Some(reparsed);
        } else {
            self.pipeline = copy;
        }
        Ok(self)
    }

    fn check_placeholder(&self, name: &str) -> Result<(), Error> {
        match &self.placeholders {
            None => Err(Error::invalid_pipeline(format!(
                "placeholder '{name}' outside a command definition"
            ))),
            Some(declared) if declared.contains(name) => Ok(()),
            Some(_) => Err(Error::invalid_pipeline(format!(
                "placeholder '{name}' is not an argument of the command being defined"
            ))),
        }
    }

    fn check_text_operand(&self, op: &Operand<String>) -> Result<(), Error> {
        match op {
            Operand::Placeholder(name) => self.check_placeholder(name),
            Operand::Literal(text) => {
                for segment in text.split('/').filter(|s| !s.is_empty()) {
                    if !is_resource_name(segment) {
                        return Err(Error::invalid_token(segment, "a key segment"));
                    }
                }
                parse_key(text).map(|_| ())
            }
        }
    }

    fn check_source(&self, source: &Source) -> Result<(), Error> {
        match source {
            Source::Value(_) => Ok(()),
            Source::Placeholder(name) => self.check_placeholder(name),
            Source::Link(text) => {
                let q = parse_query(text)?;
                q.check_encodable()
            }
            Source::Pipeline(p) => self.check_pipeline(p, false),
            Source::State(_) => match &self.placeholders {
                Some(_) => Ok(()),
                None => Err(Error::invalid_pipeline(
                    "the input state outside a command definition",
                )),
            },
            Source::Multiple(items) => items.iter().try_for_each(|s| self.check_source(s)),
        }
    }

    fn check_pipeline(&self, p: &Pipeline, is_top: bool) -> Result<(), Error> {
        if !is_realm_name(&p.realm) {
            return Err(Error::invalid_token(&p.realm, "a realm name"));
        }
        if let Some(f) = &p.filename {
            if !is_top {
                return Err(Error::invalid_pipeline("only the top-level pipeline has a filename"));
            }
            if !is_filename(f) {
                return Err(Error::invalid_token(f, "a filename"));
            }
        }
        for e in p.elements.iter() {
            self.check_element(e, &p.realm, true)?;
        }
        Ok(())
    }

    /// Checks one element. `lazy` accepts an action the registry cannot resolve yet.
    fn check_element(&self, element: &PipelineElement, realm: &str, lazy: bool) -> Result<(), Error> {
        let at_element = |e: Error| match &element.pos {
            Some(pos) if e.position.is_unknown() => e.with_position(pos),
            Some(_) | None => e,
        };
        let result = match &element.item {
            ElementKind::Text(text) => Pipeline::from_query_text(text).map(|_| ()),
            ElementKind::Action(a) => self.check_action(a, realm, lazy),
            ElementKind::Value(Operand::Placeholder(name)) => self.check_placeholder(name),
            ElementKind::Value(Operand::Literal(_)) | ElementKind::Quote | ElementKind::Volatile => {
                Ok(())
            }
            ElementKind::Resource { resource, .. } => self.check_text_operand(resource),
            ElementKind::KeyValue(op) | ElementKind::Cwd(op) => self.check_text_operand(op),
            ElementKind::Evaluate(op) | ElementKind::QueryValue(op) => match op {
                Operand::Placeholder(name) => self.check_placeholder(name),
                Operand::Literal(QueryOrPipeline::Query(text)) => parse_query(text).map(|_| ()),
                Operand::Literal(QueryOrPipeline::Pipeline(p)) => self.check_pipeline(p, false),
            },
            ElementKind::Pipeline(p) => self.check_pipeline(p, false),
        };
        result.map_err(|e| self.annotate(at_element(e), None))
    }

    fn check_action(&self, a: &ActionElement, realm: &str, lazy: bool) -> Result<(), Error> {
        check_action_name(&a.action)?;
        if let Some(ns) = &a.ns {
            for n in ns.0.iter() {
                if !(n.is_empty() || is_identifier(n)) {
                    return Err(Error::invalid_token(n, "a namespace"));
                }
            }
        }
        for s in a.parameters.iter().chain(a.arguments.values()) {
            self.check_source(&s.item)?;
        }
        let explicit = a.ns.as_ref().map(|n| n.0.clone()).unwrap_or_default();
        let namespaces = self.cmr.lookup_namespaces(&explicit);
        let md = namespaces
            .iter()
            .find_map(|ns| self.cmr.get(CommandKey::new(realm, ns, &a.action)));
        match md {
            Some(md) => {
                check_argument_names(a, md)?;
                let slots = positional_arguments(md);
                for (i, p) in a.parameters.iter().enumerate() {
                    if let Some(info) = slot_for_position(&slots, i) {
                        check_source_type(md, info, &p.item)?;
                    }
                }
                for (name, s) in a.arguments.iter() {
                    if let Some(info) = md.arguments.iter().find(|x| &x.name == name) {
                        check_source_type(md, info, &s.item)?;
                    }
                }
                Ok(())
            }
            None if lazy => Ok(()),
            None => Err(Error::action_not_registered(
                &crate::query::ActionRequest::new(a.action.clone()),
                realm,
                &namespaces,
            )),
        }
    }
}

fn check_action_name(name: &str) -> Result<(), Error> {
    if !is_identifier(name) {
        return Err(Error::invalid_token(name, "an action name"));
    }
    if [NS_INSTRUCTION, Q_INSTRUCTION, V_INSTRUCTION].contains(&name) {
        return Err(Error::invalid_token(
            name,
            "an action name (ns, q and v are planner instructions)",
        ));
    }
    Ok(())
}

fn slot_for_position<'a>(slots: &[&'a ArgumentInfo], i: usize) -> Option<&'a ArgumentInfo> {
    match slots.get(i) {
        Some(s) => Some(*s),
        None => slots.last().filter(|s| s.multiple).copied(),
    }
}

fn argument_info_of<'a>(md: &'a CommandMetadata, arg: &ArgRef) -> Result<&'a ArgumentInfo, Error> {
    match arg {
        ArgRef::Name(name) => md
            .arguments
            .iter()
            .find(|a| &a.name == name)
            .ok_or_else(|| Error::invalid_argument(&md.name, name, "no such argument")),
        ArgRef::Position(i) => slot_for_position(&positional_arguments(md), *i).ok_or_else(|| {
            Error::invalid_argument(&md.name, &format!("#{i}"), "the command takes fewer parameters")
        }),
    }
}

/// Checks a literal against an argument's declared type.
fn check_source_type(md: &CommandMetadata, info: &ArgumentInfo, source: &Source) -> Result<(), Error> {
    match source {
        Source::Value(Json::Array(items)) if info.multiple => {
            items.iter().try_for_each(|v| check_value_type(md, info, v))
        }
        Source::Value(v) => check_value_type(md, info, v),
        Source::Multiple(items) => {
            if !info.multiple {
                return Err(Error::invalid_argument(
                    &md.name,
                    &info.name,
                    "a multiple source is only for a variadic argument",
                ));
            }
            items.iter().try_for_each(|s| check_source_type(md, info, s))
        }
        Source::Placeholder(_) | Source::Link(_) | Source::Pipeline(_) | Source::State(_) => Ok(()),
    }
}

fn check_value_type(md: &CommandMetadata, info: &ArgumentInfo, v: &Json) -> Result<(), Error> {
    let wrong = |expected: &str| {
        Err(Error::invalid_argument(
            &md.name,
            &info.name,
            &format!("{v} is not {expected}"),
        ))
    };
    // Text, including the empty text a query uses for "the default", is checked as a query
    // parameter would be.
    let text = match v {
        Json::String(s) if s.is_empty() => return Ok(()),
        Json::String(s) => Some(s.as_str()),
        Json::Null | Json::Bool(_) | Json::Number(_) | Json::Array(_) | Json::Object(_) => None,
    };
    match &info.argument_type {
        ArgumentType::Integer | ArgumentType::IntegerOption => match (v, text) {
            (Json::Number(n), _) if n.is_i64() || n.is_u64() => Ok(()),
            (Json::Null, _) if info.argument_type.is_option() => Ok(()),
            (_, Some(t)) if t.parse::<i64>().is_ok() => Ok(()),
            _ => wrong("an integer"),
        },
        ArgumentType::Float | ArgumentType::FloatOption => match (v, text) {
            (Json::Number(_), _) => Ok(()),
            (Json::Null, _) if info.argument_type.is_option() => Ok(()),
            (_, Some(t)) if t.parse::<f64>().is_ok() => Ok(()),
            _ => wrong("a number"),
        },
        ArgumentType::Boolean | ArgumentType::BooleanOption => match (v, text) {
            (Json::Bool(_), _) => Ok(()),
            (Json::Null, _) if info.argument_type.is_option() => Ok(()),
            (_, Some(t))
                if matches!(
                    t.to_lowercase().as_str(),
                    "true" | "t" | "yes" | "y" | "1" | "false" | "f" | "no" | "n" | "0"
                ) || (t == "none" && info.argument_type.is_option()) =>
            {
                Ok(())
            }
            _ => wrong("a boolean"),
        },
        ArgumentType::String => match v {
            Json::String(_) | Json::Bool(_) | Json::Number(_) => Ok(()),
            Json::Null | Json::Array(_) | Json::Object(_) => wrong("text"),
        },
        ArgumentType::Enum(e) => match text {
            Some(t) if e.others_allowed || e.values.iter().any(|a| a.alias == t) => Ok(()),
            Some(_) | None if e.others_allowed => Ok(()),
            Some(_) | None => wrong(&format!(
                "one of {}",
                e.values.iter().map(|a| a.alias.as_str()).collect::<Vec<_>>().join(", ")
            )),
        },
        ArgumentType::Node(n) => match text {
            Some(t) if n.link(t).is_ok() => Ok(()),
            Some(_) | None => wrong("a node name"),
        },
        ArgumentType::GlobalEnum(_) | ArgumentType::Any | ArgumentType::None => Ok(()),
    }
}

/// Edits one action of a [`PipelineEditor`]; every method returns `Result<&mut Self, Error>`,
/// so calls chain:
/// `editor.action("a")?.ns("myns")?.set("x", 1)?.link("y", "-R/text.txt")?`.
#[derive(Debug)]
pub struct ActionEditor<'a> {
    editor: &'a mut PipelineEditor,
    index: usize,
}

impl ActionEditor<'_> {
    fn element(&self) -> Result<&ActionElement, Error> {
        self.editor.action_at(self.index)
    }

    /// Applies an edit to a copy of this action, checks it, and keeps it.
    fn edit(&mut self, f: impl FnOnce(&mut ActionElement, &CommandMetadata) -> Result<(), Error>) -> Result<&mut Self, Error> {
        let index = self.index;
        let mut action = self.element()?.clone();
        let md = self
            .editor
            .lookup(&action)
            .map_err(|e| self.editor.annotate(e, Some(index)))?
            .clone();
        f(&mut action, &md).map_err(|e| self.editor.annotate(e, Some(index)))?;
        let realm = self.editor.pipeline.realm.clone();
        self.editor
            .check_action(&action, &realm, false)
            .map_err(|e| self.editor.annotate(e, Some(index)))?;
        let pos = self.editor.pipeline.elements[index].pos.clone();
        self.editor.apply(|p| {
            p.elements[index] = Located {
                item: ElementKind::Action(action),
                pos,
            };
            Ok(())
        })?;
        Ok(self)
    }

    fn put(action: &mut ActionElement, md: &CommandMetadata, arg: ArgRef, source: Source) -> Result<(), Error> {
        let slots = positional_arguments(md);
        match arg {
            ArgRef::Position(i) => {
                if i < action.parameters.len() {
                    action.parameters[i] = Located::new(source);
                } else if i == action.parameters.len() {
                    action.parameters.push(Located::new(source));
                } else {
                    return Err(Error::invalid_argument(
                        &md.name,
                        &format!("#{i}"),
                        &format!(
                            "parameters are set in order; the next one is #{}",
                            action.parameters.len()
                        ),
                    ));
                }
            }
            ArgRef::Name(name) => {
                let info = argument_info_of(md, &ArgRef::Name(name.clone()))?;
                if info.injected {
                    return Err(Error::invalid_argument(
                        &md.name,
                        &name,
                        "injected arguments are supplied by the environment",
                    ));
                }
                match slots.iter().position(|s| s.name == name) {
                    // Given by position already: replace it there rather than duplicate it.
                    Some(i) if i < action.parameters.len() && !slots[i].multiple => {
                        action.parameters[i] = Located::new(source)
                    }
                    Some(_) | None => {
                        action.arguments.insert(name, Located::new(source));
                    }
                }
            }
        }
        Ok(())
    }

    /// Sets the action's namespaces and re-resolves its command.
    pub fn ns(&mut self, ns: impl Into<Namespaces>) -> Result<&mut Self, Error> {
        let index = self.index;
        let mut action = self.element()?.clone();
        action.ns = Some(ns.into());
        let realm = self.editor.pipeline.realm.clone();
        self.editor
            .check_action(&action, &realm, false)
            .map_err(|e| self.editor.annotate(e, Some(index)))?;
        let pos = self.editor.pipeline.elements[index].pos.clone();
        self.editor.apply(|p| {
            p.elements[index] = Located {
                item: ElementKind::Action(action),
                pos,
            };
            Ok(())
        })?;
        Ok(self)
    }
    /// Sets an argument to a literal value.
    pub fn set(&mut self, arg: impl Into<ArgRef>, v: impl Into<Json>) -> Result<&mut Self, Error> {
        let (arg, v) = (arg.into(), v.into());
        self.edit(|a, md| Self::put(a, md, arg, Source::Value(v)))
    }
    /// Sets an argument to a placeholder (only in a command definition).
    pub fn placeholder(&mut self, arg: impl Into<ArgRef>, name: &str) -> Result<&mut Self, Error> {
        let arg = arg.into();
        let name = name.to_owned();
        self.editor
            .check_placeholder(&name)
            .map_err(|e| self.editor.annotate(e, Some(self.index)))?;
        self.edit(|a, md| Self::put(a, md, arg, Source::Placeholder(name)))
    }
    /// Sets an argument to a link: `query` is evaluated and its result passed.
    pub fn link(&mut self, arg: impl Into<ArgRef>, query: &str) -> Result<&mut Self, Error> {
        let (arg, query) = (arg.into(), query.to_owned());
        self.edit(|a, md| Self::put(a, md, arg, Source::Link(query)))
    }
    /// Sets an argument to a nested pipeline, evaluated and its result passed.
    pub fn pipeline(&mut self, arg: impl Into<ArgRef>, p: Pipeline) -> Result<&mut Self, Error> {
        let arg = arg.into();
        self.edit(|a, md| Self::put(a, md, arg, Source::Pipeline(p)))
    }
    /// Sets an argument to the pipeline command's input state (only in a command definition).
    pub fn state(&mut self, arg: impl Into<ArgRef>) -> Result<&mut Self, Error> {
        let arg = arg.into();
        self.edit(|a, md| Self::put(a, md, arg, Source::State(true)))
    }
    /// Appends a literal as the next positional parameter.
    pub fn push(&mut self, v: impl Into<Json>) -> Result<&mut Self, Error> {
        let v = v.into();
        self.edit(|a, md| {
            let i = a.parameters.len();
            Self::put(a, md, ArgRef::Position(i), Source::Value(v))
        })
    }
    /// Removes an argument, given by position (the last ones only) or by name.
    pub fn unset(&mut self, arg: impl Into<ArgRef>) -> Result<&mut Self, Error> {
        let arg = arg.into();
        self.edit(|a, md| {
            match &arg {
                ArgRef::Position(i) if *i + 1 == a.parameters.len() => {
                    a.parameters.pop();
                }
                ArgRef::Position(i) => {
                    return Err(Error::invalid_argument(
                        &md.name,
                        &format!("#{i}"),
                        "only the last positional parameter can be removed",
                    ))
                }
                ArgRef::Name(name) => {
                    if a.arguments.remove(name).is_none() {
                        let slots = positional_arguments(md);
                        match slots.iter().position(|s| &s.name == name) {
                            Some(i) if i + 1 == a.parameters.len() => {
                                a.parameters.pop();
                            }
                            Some(i) if i < a.parameters.len() => {
                                return Err(Error::invalid_argument(
                                    &md.name,
                                    name,
                                    "is followed by another positional parameter; reset writes its default in place",
                                ))
                            }
                            Some(_) | None => {
                                return Err(Error::invalid_argument(&md.name, name, "is not set"))
                            }
                        }
                    }
                }
            }
            Ok(())
        })
    }
    /// Renames the action. Its arguments are kept and checked against the new command.
    pub fn rename(&mut self, action: &str) -> Result<&mut Self, Error> {
        check_action_name(action).map_err(|e| self.editor.annotate(e, Some(self.index)))?;
        let index = self.index;
        let mut element = self.element()?.clone();
        element.action = action.to_owned();
        let realm = self.editor.pipeline.realm.clone();
        self.editor
            .check_action(&element, &realm, true)
            .map_err(|e| self.editor.annotate(e, Some(index)))?;
        self.editor.apply(|p| {
            p.elements[index].item = ElementKind::Action(element);
            Ok(())
        })?;
        Ok(self)
    }
    /// Sets an argument to any source: a literal, a link, a nested pipeline (AC-19).
    pub fn set_source(&mut self, arg: impl Into<ArgRef>, source: Source) -> Result<&mut Self, Error> {
        let arg = arg.into();
        if let Source::Placeholder(name) = &source {
            self.editor
                .check_placeholder(name)
                .map_err(|e| self.editor.annotate(e, Some(self.index)))?;
        }
        self.edit(|a, md| Self::put(a, md, arg, source))
    }

    /// Resets an argument to its default: removes it when it is the last one written (by name, or
    /// the last positional parameter, or a whole variadic run), otherwise writes its default in its
    /// place. Resetting an argument that is not given changes nothing. An argument in the middle
    /// with no default cannot be reset.
    pub fn reset(&mut self, arg: impl Into<ArgRef>) -> Result<&mut Self, Error> {
        let arg = arg.into();
        self.edit(|a, md| {
            let info = argument_info_of(md, &arg)?.clone();
            if a.arguments.remove(&info.name).is_some() {
                return Ok(());
            }
            let slots = positional_arguments(md);
            let Some(slot) = slots.iter().position(|s| s.name == info.name) else {
                return Ok(());
            };
            if slot >= a.parameters.len() {
                return Ok(());
            }
            if info.multiple || slot + 1 == a.parameters.len() {
                a.parameters.truncate(slot);
                return Ok(());
            }
            match Source::from_parameter_value(&info.default) {
                Some(source) => {
                    a.parameters[slot] = Located::new(source);
                    Ok(())
                }
                None => Err(Error::invalid_argument(
                    &md.name,
                    &info.name,
                    "has no default to write in its place, and a later argument is given",
                )),
            }
        })
    }

    /// The items of a variadic argument, and whether they are given by position.
    fn items_of(a: &ActionElement, md: &CommandMetadata, name: &str) -> Result<(Vec<Source>, bool), Error> {
        let info = argument_info_of(md, &ArgRef::Name(name.to_owned()))?;
        if !info.multiple {
            return Err(Error::invalid_argument(&md.name, name, "is not a variadic argument"));
        }
        if let Some(given) = a.arguments.get(name) {
            let items = match &given.item {
                Source::Multiple(items) => items.clone(),
                Source::Value(Json::Array(values)) => {
                    values.iter().cloned().map(Source::Value).collect()
                }
                other => vec![other.clone()],
            };
            return Ok((items, false));
        }
        let slots = positional_arguments(md);
        let slot = slots.iter().position(|s| s.name == name).unwrap_or(slots.len());
        if a.parameters.len() >= slot {
            Ok((a.parameters.iter().skip(slot).map(|p| p.item.clone()).collect(), true))
        } else {
            Ok((Vec::new(), false))
        }
    }

    /// Writes the items of a variadic argument back, by position when it can be.
    fn put_items(a: &mut ActionElement, md: &CommandMetadata, name: &str, items: Vec<Source>, by_position: bool) {
        let slots = positional_arguments(md);
        let slot = slots.iter().position(|s| s.name == name).unwrap_or(slots.len());
        if by_position || (a.parameters.len() == slot && !a.arguments.contains_key(name)) {
            a.parameters.truncate(slot);
            a.parameters.extend(items.into_iter().map(Located::new));
        } else if items.is_empty() {
            a.arguments.remove(name);
        } else {
            a.arguments.insert(name.to_owned(), Located::new(Source::Multiple(items)));
        }
    }

    fn edit_items(
        &mut self,
        arg: ArgRef,
        f: impl FnOnce(&mut Vec<Source>, &CommandMetadata, &str) -> Result<(), Error>,
    ) -> Result<&mut Self, Error> {
        self.edit(|a, md| {
            let name = argument_info_of(md, &arg)?.name.clone();
            let (mut items, by_position) = Self::items_of(a, md, &name)?;
            f(&mut items, md, &name)?;
            Self::put_items(a, md, &name, items, by_position);
            Ok(())
        })
    }

    /// Inserts an item into a variadic argument before item `at` (`at` equal to the count appends).
    pub fn insert_item(&mut self, arg: impl Into<ArgRef>, at: usize, source: Source) -> Result<&mut Self, Error> {
        self.edit_items(arg.into(), |items, md, name| {
            if at > items.len() {
                return Err(Error::invalid_argument(
                    &md.name,
                    name,
                    &format!("item {at} is out of range (it has {})", items.len()),
                ));
            }
            items.insert(at, source);
            Ok(())
        })
    }

    /// Removes item `at` of a variadic argument.
    pub fn remove_item(&mut self, arg: impl Into<ArgRef>, at: usize) -> Result<&mut Self, Error> {
        self.edit_items(arg.into(), |items, md, name| {
            if at >= items.len() {
                return Err(Error::invalid_argument(
                    &md.name,
                    name,
                    &format!("item {at} is out of range (it has {})", items.len()),
                ));
            }
            items.remove(at);
            Ok(())
        })
    }

    /// Moves item `from` of a variadic argument to position `to`.
    pub fn move_item(&mut self, arg: impl Into<ArgRef>, from: usize, to: usize) -> Result<&mut Self, Error> {
        self.edit_items(arg.into(), |items, md, name| {
            if from >= items.len() || to >= items.len() {
                return Err(Error::invalid_argument(
                    &md.name,
                    name,
                    &format!("item {from} or {to} is out of range (it has {})", items.len()),
                ));
            }
            let item = items.remove(from);
            items.insert(to, item);
            Ok(())
        })
    }

    /// The parameters beyond what the command's arguments take (AC-23): none when the command has
    /// a variadic argument, which takes them all.
    pub fn excess(&self) -> Result<Vec<Source>, Error> {
        let action = self.element()?;
        let md = self.metadata()?;
        let slots = positional_arguments(md);
        if slots.iter().any(|s| s.multiple) {
            return Ok(Vec::new());
        }
        Ok(action.parameters.iter().skip(slots.len()).map(|p| p.item.clone()).collect())
    }

    /// The command this action resolves to.
    pub fn metadata(&self) -> Result<&CommandMetadata, Error> {
        self.editor.command_metadata(self.index)
    }
    /// An argument of this action's command.
    pub fn argument_info(&self, arg: impl Into<ArgRef>) -> Result<&ArgumentInfo, Error> {
        self.editor.argument_info(self.index, arg)
    }
    /// The index of the action in the pipeline.
    pub fn index(&self) -> usize {
        self.index
    }
}

/// A pipeline editor that keeps its pipeline representable as a query (AC-32).
#[derive(Debug, Clone)]
pub struct QueryEditor(PipelineEditor);

impl QueryEditor {
    /// An empty query.
    pub fn new(cmr: Arc<CommandMetadataRegistry>) -> Self {
        QueryEditor(PipelineEditor {
            query_mode: true,
            ..PipelineEditor::new(cmr)
        })
    }
    /// Edits `query`.
    pub fn from_query(query: &Query, cmr: Arc<CommandMetadataRegistry>) -> Result<Self, Error> {
        let mut editor = PipelineEditor::from_query(query, cmr)?;
        editor.query_mode = true;
        editor
            .pipeline
            .to_query_lenient(&editor.cmr)
            .map_err(|e| e.with_query(query))?;
        Ok(QueryEditor(editor))
    }
    /// The current query. Always succeeds: every edit was checked.
    pub fn query(&self) -> Query {
        match &self.0.origin {
            Some(q) => q.clone(),
            None => self.0.pipeline.to_query_lenient(&self.0.cmr).unwrap_or_default(),
        }
    }
    /// The underlying pipeline editor's read-only view.
    pub fn editor(&self) -> &PipelineEditor {
        &self.0
    }

    /// Appends an element.
    pub fn push(&mut self, element: PipelineElement) -> Result<&mut Self, Error> {
        self.0.push(element)?;
        Ok(self)
    }
    /// Inserts an element before `at`.
    pub fn insert(&mut self, at: usize, element: PipelineElement) -> Result<&mut Self, Error> {
        self.0.insert(at, element)?;
        Ok(self)
    }
    /// Replaces the element at `at`.
    pub fn replace(&mut self, at: usize, element: PipelineElement) -> Result<&mut Self, Error> {
        self.0.replace(at, element)?;
        Ok(self)
    }
    /// Removes the element at `at`.
    pub fn remove(&mut self, at: usize) -> Result<&mut Self, Error> {
        self.0.remove(at)?;
        Ok(self)
    }
    /// Moves the element at `from` to `to`.
    pub fn move_to(&mut self, from: usize, to: usize) -> Result<&mut Self, Error> {
        self.0.move_to(from, to)?;
        Ok(self)
    }
    /// Sets the realm.
    pub fn set_realm(&mut self, realm: &str) -> Result<&mut Self, Error> {
        self.0.set_realm(realm)?;
        Ok(self)
    }
    /// Sets or clears the filename.
    pub fn set_filename(&mut self, name: Option<&str>) -> Result<&mut Self, Error> {
        self.0.set_filename(name)?;
        Ok(self)
    }
    /// Appends a resource read.
    pub fn resource(&mut self, key: &str, read: Read) -> Result<&mut Self, Error> {
        self.0.resource(key, read)?;
        Ok(self)
    }
    /// Appends an action and returns its editor.
    pub fn action(&mut self, name: &str) -> Result<ActionEditor<'_>, Error> {
        self.0.action(name)
    }
    /// The editor of the action at `at`.
    pub fn edit_action(&mut self, at: usize) -> Result<ActionEditor<'_>, Error> {
        self.0.edit_action(at)
    }
    /// The namespaces later actions get.
    pub fn default_ns(&mut self, ns: Option<Namespaces>) -> &mut Self {
        self.0.default_ns(ns);
        self
    }
    /// See [`PipelineEditor::command_metadata`].
    pub fn command_metadata(&self, at: usize) -> Result<&CommandMetadata, Error> {
        self.0.command_metadata(at)
    }
    /// See [`PipelineEditor::argument_info`].
    pub fn argument_info(&self, at: usize, arg: impl Into<ArgRef>) -> Result<&ArgumentInfo, Error> {
        self.0.argument_info(at, arg)
    }
    /// See [`PipelineEditor::argument_source`].
    pub fn argument_source(&self, at: usize, arg: impl Into<ArgRef>) -> Result<Option<Source>, Error> {
        self.0.argument_source(at, arg)
    }
    /// See [`PipelineEditor::unset_arguments`].
    pub fn unset_arguments(&self, at: usize) -> Result<Vec<&ArgumentInfo>, Error> {
        self.0.unset_arguments(at)
    }
    /// See [`PipelineEditor::namespace_run`].
    pub fn namespace_run(&self, at: usize) -> Result<std::ops::Range<usize>, Error> {
        self.0.namespace_run(at)
    }
    /// See [`PipelineEditor::available_namespaces`].
    pub fn available_namespaces(&self, at: usize) -> Result<Vec<String>, Error> {
        self.0.available_namespaces(at)
    }
    /// See [`PipelineEditor::unused_namespaces`].
    pub fn unused_namespaces(&self, at: usize) -> Result<Vec<String>, Error> {
        self.0.unused_namespaces(at)
    }
    /// See [`PipelineEditor::namespace_impact`].
    pub fn namespace_impact(&self, at: usize, namespaces: &[String]) -> Result<NamespaceImpact, Error> {
        self.0.namespace_impact(at, namespaces)
    }
    /// See [`PipelineEditor::set_namespaces`].
    pub fn set_namespaces(
        &mut self,
        at: usize,
        namespaces: Vec<String>,
        accept_changes: bool,
    ) -> Result<NamespaceImpact, Error> {
        self.0.set_namespaces(at, namespaces, accept_changes)
    }
}

impl From<&QueryEditor> for Query {
    fn from(editor: &QueryEditor) -> Self {
        editor.query()
    }
}

impl TryFrom<&PipelineEditor> for Query {
    type Error = Error;
    fn try_from(editor: &PipelineEditor) -> Result<Self, Self::Error> {
        editor
            .pipeline
            .to_query_with(&editor.cmr)
            .map_err(|e| editor.annotate(e, None))
    }
}

#[cfg(test)]
mod tests;
