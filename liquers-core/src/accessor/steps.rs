//! Projection steps: lenses from a value to a part of it, for documents, recipes and queries.
//!
//! A step sees only the parent value, never its metadata, so it chooses by the value's
//! `identifier()` and content. Document steps parse text or bytes (JSON when it starts with `{` or
//! `[`, otherwise YAML) or take the value's JSON form, and write back in the parent's own
//! representation. Query steps edit through the command registry, so a refused edit is the
//! editor's error and nothing is written.
//!
//! # Main components
//!
//! - [`FieldStep`] and [`ItemStep`]: a field of a mapping, an item of a list.
//! - [`RecipeStep`]: a recipe of a `recipes.yaml` (a `recipes:` list, or a bare list), as
//!   `Value::Recipe`; [`RecipeFileStep`] the same by filename, following it through reordering.
//! - [`QueryFieldStep`] and [`PipelineFieldStep`]: a recipe's query or pipeline.
//! - [`ActionStep`]: one action of a query or pipeline, as a one-element pipeline (element 0 is
//!   the resource of a query that has one).
//! - [`ArgumentStep`]: one argument of an action, as its JSON value; `none` unsets it.
//!
//! # Where it is used
//!
//! - [`super::ValueAccessor::project`] takes any of these; the `acc` commands of `liquers-lib`
//!   (`acc/recipe`, `acc/recipe_file`, `acc/query`, `acc/action`, `acc/argument`, `acc/field`, `acc/item`) build them.
//!
//! # Related concepts
//!
//! - [`crate::pipeline::QueryEditor`] and [`crate::pipeline::PipelineEditor`]: the validated edits
//!   the query steps apply.
//! - [`crate::recipes::RecipeList`]: the document [`RecipeStep`] reads.
//!
//! # Examples
//!
//! ```
//! use std::sync::Arc;
//! use liquers_core::accessor::{steps::FieldStep, ProjectionStep};
//! use liquers_core::value::{Value, ValueInterface};
//! let doc = Value::from("title: Sales\nrows: 3\n");
//! let step: Arc<dyn ProjectionStep<Value>> = Arc::new(FieldStep::new("rows"));
//! assert_eq!(step.get(&doc).unwrap(), Some(Value::from(3i64)));
//! let edited = step.put(&doc, Value::from(4i64)).unwrap();
//! assert_eq!(edited.try_into_string().unwrap(), "title: Sales\nrows: 4\n");
//! ```
//!
//! - [Tests](https://github.com/orest-d/liquers/blob/main/liquers-core/tests/value_accessor.rs):
//!   the recipe, query, action and argument chain over a stored `recipes.yaml`.

use std::borrow::Cow;
use std::fmt::Debug;
use std::sync::Arc;

use serde_json::Value as Json;
use serde_yaml::Value as Yaml;

use super::{stable_hash, PartDescription, ProjectionStep};
use crate::command_metadata::CommandMetadataRegistry;
use crate::error::Error;
use crate::pipeline::{
    ArgRef, ElementKind, Pipeline, PipelineEditor, PipelineElement, QueryEditor, Source,
};
use crate::recipes::Recipe;
use crate::value::ValueInterface;

// ---------------------------------------------------------------------------------------------
// Documents
// ---------------------------------------------------------------------------------------------

/// How a document value is held, so a part can be written back the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Repr {
    /// Text, JSON when `json`, else YAML.
    Text { json: bool },
    /// UTF-8 bytes, JSON when `json`, else YAML.
    Bytes { json: bool },
    /// A value with a JSON form (an object, an array).
    Tree,
}

fn looks_like_json(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with('{') || trimmed.starts_with('[')
}

fn yaml_error(e: impl std::fmt::Display) -> Error {
    Error::conversion_error_with_message("document", "YAML", &e.to_string())
}

fn parse_text(text: &str) -> Result<Yaml, Error> {
    if text.trim().is_empty() {
        return Ok(Yaml::Mapping(Default::default()));
    }
    serde_yaml::from_str(text).map_err(yaml_error)
}

fn read_doc<V: ValueInterface>(value: &V) -> Result<(Yaml, Repr), Error> {
    match value.identifier().as_ref() {
        "Text" => {
            let text = value.try_into_string()?;
            Ok((parse_text(&text)?, Repr::Text { json: looks_like_json(&text) }))
        }
        "Bytes" => {
            let bytes = value.try_into_bytes()?;
            let text = String::from_utf8(bytes).map_err(|e| {
                Error::conversion_error_with_message("bytes", "UTF-8 text", &e.to_string())
            })?;
            Ok((parse_text(&text)?, Repr::Bytes { json: looks_like_json(&text) }))
        }
        _ => {
            let json = value.try_into_json_value()?;
            Ok((serde_yaml::to_value(json).map_err(yaml_error)?, Repr::Tree))
        }
    }
}

fn to_json(tree: &Yaml) -> Result<Json, Error> {
    serde_json::to_value(tree)
        .map_err(|e| Error::conversion_error_with_message("YAML", "JSON", &e.to_string()))
}

fn render(tree: &Yaml, json: bool) -> Result<String, Error> {
    if json {
        serde_json::to_string_pretty(&to_json(tree)?)
            .map_err(|e| Error::conversion_error_with_message("document", "JSON", &e.to_string()))
    } else {
        serde_yaml::to_string(tree).map_err(yaml_error)
    }
}

fn write_doc<V: ValueInterface>(tree: &Yaml, repr: Repr) -> Result<V, Error> {
    match repr {
        Repr::Text { json } => Ok(V::from_string(render(tree, json)?)),
        Repr::Bytes { json } => Ok(V::from_bytes(render(tree, json)?.into_bytes())),
        Repr::Tree => V::try_from_json_value(&to_json(tree)?),
    }
}

fn part_value<V: ValueInterface>(tree: &Yaml) -> Result<V, Error> {
    V::try_from_json_value(&to_json(tree)?)
}

fn part_tree<V: ValueInterface>(part: &V) -> Result<Yaml, Error> {
    serde_yaml::to_value(part.try_into_json_value()?).map_err(yaml_error)
}

fn describe(step: impl Into<String>) -> PartDescription {
    PartDescription {
        step: step.into(),
        ..PartDescription::default()
    }
}

/// A field of a mapping.
#[derive(Debug, Clone)]
pub struct FieldStep(pub String);

impl FieldStep {
    pub fn new(name: impl Into<String>) -> Self {
        FieldStep(name.into())
    }
}

impl<V: ValueInterface> ProjectionStep<V> for FieldStep {
    fn name(&self) -> Cow<'_, str> {
        Cow::Owned(format!("field-{}", self.0))
    }
    fn get(&self, parent: &V) -> Result<Option<V>, Error> {
        let (tree, _) = read_doc(parent)?;
        match tree.get(&self.0) {
            Some(part) => Ok(Some(part_value(part)?)),
            None => Ok(None),
        }
    }
    fn put(&self, parent: &V, part: V) -> Result<V, Error> {
        let (mut tree, repr) = read_doc(parent)?;
        let Some(mapping) = tree.as_mapping_mut() else {
            return Err(Error::general_error(format!(
                "cannot set field '{}' of a value that is not a mapping",
                self.0
            )));
        };
        mapping.insert(Yaml::String(self.0.clone()), part_tree(&part)?);
        write_doc(&tree, repr)
    }
    fn anchor(&self, parent: &V) -> Result<Option<u64>, Error> {
        let (tree, _) = read_doc(parent)?;
        Ok(tree
            .get(&self.0)
            .map(|_| stable_hash(&[b"field", self.0.as_bytes()])))
    }
    fn describe(&self, _parent: &V) -> Result<PartDescription, Error> {
        Ok(describe(ProjectionStep::<V>::name(self)))
    }
}

/// An item of a list. Its anchor includes the list length, so an insertion or a removal
/// invalidates the item projections of that list.
#[derive(Debug, Clone)]
pub struct ItemStep(pub usize);

impl<V: ValueInterface> ProjectionStep<V> for ItemStep {
    fn name(&self) -> Cow<'_, str> {
        Cow::Owned(format!("item-{}", self.0))
    }
    fn get(&self, parent: &V) -> Result<Option<V>, Error> {
        let (tree, _) = read_doc(parent)?;
        match tree.as_sequence().and_then(|s| s.get(self.0)) {
            Some(part) => Ok(Some(part_value(part)?)),
            None => Ok(None),
        }
    }
    fn put(&self, parent: &V, part: V) -> Result<V, Error> {
        let (mut tree, repr) = read_doc(parent)?;
        let index = self.0;
        match tree.as_sequence_mut().and_then(|s| s.get_mut(index)) {
            Some(slot) => *slot = part_tree(&part)?,
            None => {
                return Err(Error::general_error(format!(
                    "there is no item {index} to set"
                )))
            }
        }
        write_doc(&tree, repr)
    }
    fn anchor(&self, parent: &V) -> Result<Option<u64>, Error> {
        let (tree, _) = read_doc(parent)?;
        Ok(tree.as_sequence().and_then(|s| {
            (self.0 < s.len()).then(|| {
                stable_hash(&[b"item", &(s.len() as u64).to_le_bytes(), &(self.0 as u64).to_le_bytes()])
            })
        }))
    }
    fn describe(&self, _parent: &V) -> Result<PartDescription, Error> {
        Ok(describe(ProjectionStep::<V>::name(self)))
    }
}

// ---------------------------------------------------------------------------------------------
// Recipes
// ---------------------------------------------------------------------------------------------

fn recipe_of<V: ValueInterface>(value: &V) -> Result<Recipe, Error> {
    serde_json::from_value(value.try_into_json_value()?)
        .map_err(|e| Error::conversion_error_with_message(value.identifier(), "Recipe", &e.to_string()))
}

/// The recipes of a `recipes.yaml` tree: its `recipes:` list, or the tree when it is a list.
fn recipes(tree: &Yaml) -> Option<&Vec<Yaml>> {
    match tree.get("recipes") {
        Some(list) => list.as_sequence(),
        None => tree.as_sequence(),
    }
}

fn recipes_mut(tree: &mut Yaml) -> Option<&mut Vec<Yaml>> {
    if tree.get("recipes").is_some() {
        tree.get_mut("recipes").and_then(|l| l.as_sequence_mut())
    } else {
        tree.as_sequence_mut()
    }
}

/// A recipe of a recipe list, as `Value::Recipe`. Its anchor is the list length and the
/// recipe's filename (its position when it has none).
#[derive(Debug, Clone)]
pub struct RecipeStep(pub usize);

impl RecipeStep {
    fn item(&self, tree: &Yaml) -> Result<Option<Recipe>, Error> {
        match recipes(tree).and_then(|l| l.get(self.0)) {
            Some(item) => Ok(Some(serde_yaml::from_value(item.clone()).map_err(|e| {
                Error::conversion_error_with_message("recipe list item", "Recipe", &e.to_string())
            })?)),
            None => Ok(None),
        }
    }
}

impl<V: ValueInterface> ProjectionStep<V> for RecipeStep {
    fn name(&self) -> Cow<'_, str> {
        Cow::Owned(format!("recipe-{}", self.0))
    }
    fn get(&self, parent: &V) -> Result<Option<V>, Error> {
        let (tree, _) = read_doc(parent)?;
        Ok(self.item(&tree)?.map(V::from_recipe))
    }
    fn put(&self, parent: &V, part: V) -> Result<V, Error> {
        let recipe = recipe_of(&part)?;
        let (mut tree, repr) = read_doc(parent)?;
        let index = self.0;
        match recipes_mut(&mut tree).and_then(|l| l.get_mut(index)) {
            Some(slot) => *slot = serde_yaml::to_value(&recipe).map_err(yaml_error)?,
            None => {
                return Err(Error::general_error(format!(
                    "there is no recipe {index} to set"
                )))
            }
        }
        write_doc(&tree, repr)
    }
    fn anchor(&self, parent: &V) -> Result<Option<u64>, Error> {
        let (tree, _) = read_doc(parent)?;
        let len = recipes(&tree).map(|l| l.len()).unwrap_or(0) as u64;
        Ok(match self.item(&tree)? {
            Some(recipe) => {
                let place = match recipe.filename().ok().flatten() {
                    Some(name) => name.encode().to_string(),
                    None => format!("#{}", self.0),
                };
                Some(stable_hash(&[b"recipe", &len.to_le_bytes(), place.as_bytes()]))
            }
            None => None,
        })
    }
    fn describe(&self, _parent: &V) -> Result<PartDescription, Error> {
        Ok(describe(ProjectionStep::<V>::name(self)))
    }
    fn overridden_arguments(&self, part: &V) -> Vec<String> {
        recipe_of(part).map(|r| recipe_overrides(&r)).unwrap_or_default()
    }
}

/// A recipe of a recipe list by its filename (`wheel.glb`), as `Value::Recipe`, wherever it
/// sits in the list: it follows the recipe when others are added, removed or reordered. Setting
/// a recipe with another filename is refused, because a rename must rewrite the folder's
/// references (`crate::recipe_tree::TreeEditor::rename`).
#[derive(Debug, Clone)]
pub struct RecipeFileStep(pub String);

impl RecipeFileStep {
    /// The position and the recipe with this filename, if any.
    fn find(&self, tree: &Yaml) -> Result<Option<(usize, Recipe)>, Error> {
        let Some(list) = recipes(tree) else {
            return Ok(None);
        };
        for (index, item) in list.iter().enumerate() {
            // An item that is not a recipe, or has no filename, is not this one.
            let Ok(recipe) = serde_yaml::from_value::<Recipe>(item.clone()) else {
                continue;
            };
            if let Ok(Some(name)) = recipe.filename() {
                if name.name == self.0 {
                    return Ok(Some((index, recipe)));
                }
            }
        }
        Ok(None)
    }
}

impl<V: ValueInterface> ProjectionStep<V> for RecipeFileStep {
    fn name(&self) -> Cow<'_, str> {
        Cow::Owned(format!("recipe_file-{}", self.0))
    }
    fn get(&self, parent: &V) -> Result<Option<V>, Error> {
        let (tree, _) = read_doc(parent)?;
        Ok(self.find(&tree)?.map(|(_, recipe)| V::from_recipe(recipe)))
    }
    fn put(&self, parent: &V, part: V) -> Result<V, Error> {
        let recipe = recipe_of(&part)?;
        let filename = recipe.filename()?.map(|n| n.name);
        if filename.as_deref() != Some(self.0.as_str()) {
            return Err(Error::general_error(format!(
                "the recipe '{}' cannot be given the filename {}; rename it in the recipes editor, which rewrites the references",
                self.0,
                filename.map(|f| format!("'{f}'")).unwrap_or_else(|| "none".to_owned())
            )));
        }
        let (mut tree, repr) = read_doc(parent)?;
        let Some((index, _)) = self.find(&tree)? else {
            return Err(Error::general_error(format!("there is no recipe '{}' to set", self.0)));
        };
        match recipes_mut(&mut tree).and_then(|l| l.get_mut(index)) {
            Some(slot) => *slot = serde_yaml::to_value(&recipe).map_err(yaml_error)?,
            None => {
                return Err(Error::general_error(format!("there is no recipe '{}' to set", self.0)))
            }
        }
        write_doc(&tree, repr)
    }
    fn anchor(&self, parent: &V) -> Result<Option<u64>, Error> {
        let (tree, _) = read_doc(parent)?;
        Ok(self
            .find(&tree)?
            .map(|_| stable_hash(&[b"recipe_file", self.0.as_bytes()])))
    }
    fn describe(&self, _parent: &V) -> Result<PartDescription, Error> {
        Ok(describe(ProjectionStep::<V>::name(self)))
    }
    fn overridden_arguments(&self, part: &V) -> Vec<String> {
        recipe_of(part).map(|r| recipe_overrides(&r)).unwrap_or_default()
    }
}

/// A recipe's query, as `Value::Query`; absent for a recipe defined by a pipeline.
#[derive(Debug, Clone, Default)]
pub struct QueryFieldStep;

impl<V: ValueInterface> ProjectionStep<V> for QueryFieldStep {
    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("query")
    }
    fn get(&self, parent: &V) -> Result<Option<V>, Error> {
        let recipe = recipe_of(parent)?;
        if recipe.query.is_empty() {
            return Ok(None);
        }
        Ok(Some(V::from_query(&recipe.get_query()?)))
    }
    fn put(&self, parent: &V, part: V) -> Result<V, Error> {
        let mut recipe = recipe_of(parent)?;
        recipe.query = part.try_into_query()?.encode();
        Ok(V::from_recipe(recipe))
    }
    fn describe(&self, _parent: &V) -> Result<PartDescription, Error> {
        Ok(describe("query"))
    }
}

/// A recipe's pipeline, as `Value::Pipeline`; absent for a recipe defined by a query.
#[derive(Debug, Clone, Default)]
pub struct PipelineFieldStep;

impl<V: ValueInterface> ProjectionStep<V> for PipelineFieldStep {
    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("pipeline")
    }
    fn get(&self, parent: &V) -> Result<Option<V>, Error> {
        Ok(recipe_of(parent)?.pipeline.map(V::from_pipeline))
    }
    fn put(&self, parent: &V, part: V) -> Result<V, Error> {
        let mut recipe = recipe_of(parent)?;
        recipe.pipeline = Some(part.try_into_pipeline()?);
        Ok(V::from_recipe(recipe))
    }
    fn describe(&self, _parent: &V) -> Result<PartDescription, Error> {
        Ok(describe("pipeline"))
    }
}

// ---------------------------------------------------------------------------------------------
// Queries, actions and arguments
// ---------------------------------------------------------------------------------------------

/// The editor of a query or pipeline value, which writes back in the same form.
enum Edited {
    Query(QueryEditor),
    Pipeline(PipelineEditor),
}

impl Edited {
    fn of<V: ValueInterface>(value: &V, cmr: &Arc<CommandMetadataRegistry>) -> Result<Self, Error> {
        match value.try_into_query() {
            Ok(query) => Ok(Edited::Query(QueryEditor::from_query(&query, cmr.clone())?)),
            Err(_) => Ok(Edited::Pipeline(PipelineEditor::from_pipeline(
                value.try_into_pipeline()?,
                cmr.clone(),
            )?)),
        }
    }

    fn editor(&self) -> &PipelineEditor {
        match self {
            Edited::Query(q) => q.editor(),
            Edited::Pipeline(p) => p,
        }
    }

    fn element(&self, at: usize) -> Option<&PipelineElement> {
        self.editor().pipeline().elements.get(at)
    }

    fn replace(&mut self, at: usize, element: PipelineElement) -> Result<(), Error> {
        match self {
            Edited::Query(q) => q.replace(at, element).map(|_| ()),
            Edited::Pipeline(p) => p.replace(at, element).map(|_| ()),
        }
    }

    fn into_value<V: ValueInterface>(self) -> V {
        match self {
            Edited::Query(q) => V::from_query(&q.query()),
            Edited::Pipeline(p) => V::from_pipeline(p.into_pipeline()),
        }
    }
}

/// The command name an action element shows, for anchors when the registry does not know it.
fn command_name(editor: &PipelineEditor, at: usize) -> Option<String> {
    match editor.command_key(at) {
        Ok(key) => Some(key.to_string()),
        Err(_) => match editor.pipeline().elements.get(at).map(|e| &e.item) {
            Some(ElementKind::Action(action)) => Some(action.action.clone()),
            Some(_) | None => None,
        },
    }
}

/// One action of a query or pipeline, as a one-element pipeline in the parent's realm. Element
/// indices are the editor's: 0 is a query's resource. Its anchor is the element count and the
/// command, so an inserted step or a renamed command invalidates it.
#[derive(Debug, Clone)]
pub struct ActionStep {
    pub index: usize,
    pub registry: Arc<CommandMetadataRegistry>,
}

impl ActionStep {
    pub fn new(index: usize, registry: Arc<CommandMetadataRegistry>) -> Self {
        ActionStep { index, registry }
    }
}

impl<V: ValueInterface> ProjectionStep<V> for ActionStep {
    fn name(&self) -> Cow<'_, str> {
        Cow::Owned(format!("action-{}", self.index))
    }
    fn get(&self, parent: &V) -> Result<Option<V>, Error> {
        let edited = Edited::of(parent, &self.registry)?;
        match edited.element(self.index) {
            Some(element) if matches!(element.item, ElementKind::Action(_)) => {
                let mut pipeline = Pipeline::new(vec![element.clone()]);
                pipeline.realm = edited.editor().pipeline().realm.clone();
                Ok(Some(V::from_pipeline(pipeline)))
            }
            Some(_) | None => Ok(None),
        }
    }
    fn put(&self, parent: &V, part: V) -> Result<V, Error> {
        let mut edited = Edited::of(parent, &self.registry)?;
        let mut elements = part.try_into_pipeline()?.elements;
        if elements.len() != 1 {
            return Err(Error::general_error(format!(
                "an action part is a pipeline of one element, not {}",
                elements.len()
            )));
        }
        let element = elements.remove(0);
        if !matches!(element.item, ElementKind::Action(_)) {
            return Err(Error::general_error(
                "an action part must be an action".to_string(),
            ));
        }
        edited.replace(self.index, element)?;
        Ok(edited.into_value())
    }
    fn anchor(&self, parent: &V) -> Result<Option<u64>, Error> {
        let edited = Edited::of(parent, &self.registry)?;
        let editor = edited.editor();
        let count = editor.pipeline().elements.len() as u64;
        Ok(match edited.element(self.index).map(|e| &e.item) {
            Some(ElementKind::Action(_)) => command_name(editor, self.index).map(|command| {
                stable_hash(&[b"action", &count.to_le_bytes(), command.as_bytes()])
            }),
            Some(_) | None => None,
        })
    }
    fn describe(&self, parent: &V) -> Result<PartDescription, Error> {
        let edited = Edited::of(parent, &self.registry)?;
        let mut description = describe(format!("action-{}", self.index));
        description.command = edited.editor().command_key(self.index).ok();
        Ok(description)
    }
}

/// An argument's value as plain JSON: a literal as itself, a variadic argument as the array of
/// its items, and anything else (a link, a placeholder) in its tagged form.
fn source_json(source: &Source) -> Json {
    match source {
        Source::Value(value) => value.clone(),
        Source::Multiple(items) => Json::Array(items.iter().map(source_json).collect()),
        Source::Placeholder(_) | Source::Link(_) | Source::Pipeline(_) | Source::State(_) => {
            source.to_json()
        }
    }
}

/// One argument of the (first) action of a query or pipeline, as its JSON value as a `V`; an
/// argument the action does not give reads as `none`, and setting `none` unsets it. A set goes
/// through `ActionEditor::set`, so a value of the wrong type or an argument the command does not
/// have is refused.
#[derive(Debug, Clone)]
pub struct ArgumentStep {
    pub argument: ArgRef,
    pub registry: Arc<CommandMetadataRegistry>,
}

impl ArgumentStep {
    pub fn new(argument: impl Into<ArgRef>, registry: Arc<CommandMetadataRegistry>) -> Self {
        ArgumentStep {
            argument: argument.into(),
            registry,
        }
    }

    fn label(&self) -> String {
        match &self.argument {
            ArgRef::Name(name) => name.clone(),
            ArgRef::Position(i) => format!("{i}"),
        }
    }

    /// The editor of the parent and the index of its first action.
    fn edited<V: ValueInterface>(&self, parent: &V) -> Result<(Edited, usize), Error> {
        let edited = Edited::of(parent, &self.registry)?;
        let at = edited
            .editor()
            .pipeline()
            .elements
            .iter()
            .position(|e| matches!(e.item, ElementKind::Action(_)))
            .ok_or_else(|| Error::general_error("there is no action to take an argument of".to_string()))?;
        Ok((edited, at))
    }
}

impl<V: ValueInterface> ProjectionStep<V> for ArgumentStep {
    fn name(&self) -> Cow<'_, str> {
        Cow::Owned(format!("argument-{}", self.label()))
    }
    fn get(&self, parent: &V) -> Result<Option<V>, Error> {
        let (edited, at) = self.edited(parent)?;
        match edited.editor().argument_source(at, self.argument.clone())? {
            Some(source) => Ok(Some(V::try_from_json_value(&source_json(&source))?)),
            None => Ok(Some(V::none())),
        }
    }
    fn put(&self, parent: &V, part: V) -> Result<V, Error> {
        let (mut edited, at) = self.edited(parent)?;
        let unset = part.is_none();
        let json = if unset { Json::Null } else { part.try_into_json_value()? };
        match &mut edited {
            Edited::Query(q) => {
                let mut action = q.edit_action(at)?;
                if unset {
                    action.unset(self.argument.clone())?;
                } else {
                    action.set(self.argument.clone(), json)?;
                }
            }
            Edited::Pipeline(p) => {
                let mut action = p.edit_action(at)?;
                if unset {
                    action.unset(self.argument.clone())?;
                } else {
                    action.set(self.argument.clone(), json)?;
                }
            }
        }
        Ok(edited.into_value())
    }
    fn anchor(&self, parent: &V) -> Result<Option<u64>, Error> {
        let (edited, at) = self.edited(parent)?;
        let editor = edited.editor();
        let Ok(info) = editor.argument_info(at, self.argument.clone()) else {
            return Ok(None);
        };
        let command = command_name(editor, at).unwrap_or_default();
        Ok(Some(stable_hash(&[b"argument", command.as_bytes(), info.name.as_bytes()])))
    }
    fn describe(&self, parent: &V) -> Result<PartDescription, Error> {
        let (edited, at) = self.edited(parent)?;
        let editor = edited.editor();
        let mut description = describe(ProjectionStep::<V>::name(self));
        description.command = editor.command_key(at).ok();
        description.argument = Some(editor.argument_info(at, self.argument.clone())?.clone());
        Ok(description)
    }
}

/// The names a recipe's `arguments` and `links` override.
fn recipe_overrides(recipe: &Recipe) -> Vec<String> {
    recipe
        .arguments
        .keys()
        .chain(recipe.links.keys())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    #[test]
    fn document_steps_keep_the_representation() {
        let yaml = Value::from("a: 1\nlist:\n- x\n- y\n");
        let field: Arc<dyn ProjectionStep<Value>> = Arc::new(FieldStep::new("a"));
        let edited = field.put(&yaml, Value::from(2i64)).unwrap();
        assert_eq!(edited.try_into_string().unwrap(), "a: 2\nlist:\n- x\n- y\n");
        let json = Value::from("{\"a\": [1, 2]}");
        let item: Arc<dyn ProjectionStep<Value>> = Arc::new(ItemStep(1));
        let list = field.get(&json).unwrap().unwrap();
        assert_eq!(item.get(&list).unwrap(), Some(Value::from(2i64)));
        let edited = field
            .put(&json, item.put(&list, Value::from(3i64)).unwrap())
            .unwrap();
        assert!(looks_like_json(&edited.try_into_string().unwrap()));
        assert_eq!(field.get(&json).unwrap().unwrap().try_into_json_value().unwrap(), serde_json::json!([1, 2]));
        assert!(field.get(&Value::from("b: 1\n")).unwrap().is_none());
    }

    fn recipe_file(file: &str) -> Arc<dyn ProjectionStep<Value>> {
        Arc::new(RecipeFileStep(file.to_owned()))
    }

    fn title_of(value: &Value) -> String {
        recipe_of(value).unwrap().title
    }

    #[test]
    fn recipe_file_step_reads_and_writes_by_filename() {
        let doc = Value::from("recipes:\n- {query: a.txt, title: A}\n- {query: \"-R/./a.txt/-/b.txt\", title: B}\n");
        let step = recipe_file("b.txt");
        let b = step.get(&doc).unwrap().unwrap();
        assert_eq!(title_of(&b), "B");
        let mut recipe = recipe_of(&b).unwrap();
        recipe.title = "Bee".to_owned();
        let edited = step.put(&doc, Value::from_recipe(recipe)).unwrap();
        assert_eq!(title_of(&step.get(&edited).unwrap().unwrap()), "Bee");
        assert_eq!(title_of(&recipe_file("a.txt").get(&edited).unwrap().unwrap()), "A");
        assert!(recipe_file("c.txt").get(&doc).unwrap().is_none());
        assert_eq!(step.name(), "recipe_file-b.txt");
    }

    #[test]
    fn recipe_file_step_follows_reordering() {
        let doc = Value::from("recipes:\n- {query: a.txt, title: A}\n- {query: b.txt, title: B}\n");
        let reordered = Value::from("recipes:\n- {query: c.txt, title: C}\n- {query: b.txt, title: B}\n- {query: a.txt, title: A}\n");
        let step = recipe_file("a.txt");
        assert_eq!(title_of(&step.get(&doc).unwrap().unwrap()), "A");
        assert_eq!(title_of(&step.get(&reordered).unwrap().unwrap()), "A");
        // The anchor names the file, not its position, so a binding survives the move.
        assert_eq!(step.anchor(&doc).unwrap(), step.anchor(&reordered).unwrap());
    }

    #[test]
    fn recipe_file_step_refuses_a_filename_change() {
        let doc = Value::from("recipes:\n- {query: a.txt, title: A}\n");
        let step = recipe_file("a.txt");
        let mut recipe = recipe_of(&step.get(&doc).unwrap().unwrap()).unwrap();
        recipe.query = "z.txt".to_owned();
        let error = step.put(&doc, Value::from_recipe(recipe)).unwrap_err();
        assert!(error.to_string().contains("rename"), "{error}");
        let missing = recipe_file("q.txt").put(&doc, Value::from_recipe(Recipe::new("q.txt".into(), String::new(), String::new()).unwrap()));
        assert!(missing.is_err());
    }
}
