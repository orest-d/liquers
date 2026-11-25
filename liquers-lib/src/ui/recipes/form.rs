//! [`RecipeForm`]: one recipe as a form, with the query editor inside (AC-5).

use std::sync::Arc;

use liquers_core::accessor::Fingerprint;
use liquers_core::command_metadata::CommandMetadataRegistry;
use liquers_core::error::Error;
use liquers_core::metadata::Version;
use liquers_core::parse::parse_query;
use liquers_core::recipes::Recipe;
use liquers_core::value::ValueInterface;

use crate::ui::editor::{EditorCommand, EditorEffect, EditorKind, EditorMode, EditorState, EditorView, NodeContext};
use crate::value::Value;

/// An edit of a recipe form.
#[derive(Debug, Clone, PartialEq)]
pub enum FormCommand {
    /// Set the title.
    SetTitle(String),
    /// Set the description.
    SetDescription(String),
    /// Rename the recipe's file (a filename, or a name keeping the extension).
    SetFilename(String),
    /// Set an option from its text: `cwd` (empty: none), `volatile` (`true`/`false`),
    /// `expires` (an expiration such as `never`, `in 5 min`), `stored` (`true`/`false`/empty).
    SetOption { field: String, text: String },
    /// Set an argument override (`name` → a JSON or plain text value); an empty name is refused.
    SetOverride { name: String, text: String },
    /// Remove an argument or link override.
    RemoveOverride(String),
    /// An edit of the query editor.
    Editor(EditorCommand),
    /// Show or hide the options section.
    ToggleOptions,
    /// Switch the query editor between Simple and Advanced.
    SetMode(EditorMode),
}

/// What a form edit asks of its owner.
#[derive(Debug, Clone, PartialEq)]
pub enum FormChange {
    /// Nothing to write.
    None,
    /// The recipe changed; its filename may have changed with the query.
    Recipe(Recipe),
    /// Rename the recipe's node or file to the given name or filename.
    Rename(String),
    /// Rename another node of the folder (a Node argument's Rename).
    RenameNode { from: String, to: String },
    /// The query editor's file selector asks for a listing of the store folder; the answer goes
    /// back with [`RecipeForm::on_directory`].
    ListDirectory(String),
}

/// One recipe being edited: its fields, and an embedded query editor whose committed edits are
/// written into the recipe at once (the owner decides when the recipe is written).
#[derive(Debug, Clone)]
pub struct RecipeForm {
    /// The row id: the filename, or `#<index>` for a recipe without one.
    pub id: String,
    recipe: Recipe,
    query: EditorState,
    /// What is being typed in the query editor (egui).
    pub view: EditorView,
    /// The query editor's mode.
    pub mode: EditorMode,
    /// Whether the options section is open.
    pub options_open: bool,
    /// The last refused edit.
    pub message: Option<String>,
}

fn zero() -> Fingerprint {
    Fingerprint { anchor: 0, content: Version::new(0) }
}

/// The query editor's value of a recipe: its query, or its pipeline.
fn query_value(recipe: &Recipe) -> Result<Value, Error> {
    match (&recipe.pipeline, recipe.query.is_empty()) {
        (Some(pipeline), true) => Ok(Value::from_pipeline(pipeline.clone())),
        (Some(_), false) | (None, true) | (None, false) => Ok(Value::from_query(&parse_query(&recipe.query)?)),
    }
}

impl RecipeForm {
    /// A form over `recipe`, known by `id`.
    pub fn new(id: impl Into<String>, recipe: Recipe, registry: Arc<CommandMetadataRegistry>) -> Self {
        let mut form = RecipeForm {
            id: id.into(),
            recipe: Recipe::default(),
            query: EditorState::new(EditorKind::Query, registry),
            view: EditorView::default(),
            mode: EditorMode::Simple,
            options_open: false,
            message: None,
        };
        form.reload(recipe);
        form
    }

    /// The recipe as edited.
    pub fn recipe(&self) -> &Recipe {
        &self.recipe
    }

    /// The embedded query editor.
    pub fn query_editor(&self) -> &EditorState {
        &self.query
    }

    /// The recipe's filename, if it has one.
    pub fn filename(&self) -> Option<String> {
        self.recipe.filename().ok().flatten().map(|f| f.name)
    }

    /// Loads `recipe` (after a change elsewhere, or a write).
    pub fn reload(&mut self, recipe: Recipe) {
        match query_value(&recipe) {
            Ok(value) => {
                let _ = self.query.on_snapshot(&value, None);
            }
            Err(e) => self.message = Some(e.to_string()),
        }
        self.recipe = recipe;
    }

    /// The folder's nodes, for the query's Node arguments.
    pub fn set_node_context(&mut self, context: Option<NodeContext>) {
        self.query.set_node_context(context);
    }

    /// Applies one edit. A refused edit changes nothing and is kept in [`Self::message`].
    pub fn apply(&mut self, command: FormCommand) -> Result<FormChange, Error> {
        let result = self.apply_inner(command);
        match &result {
            Ok(_) => self.message = None,
            Err(e) => self.message = Some(e.to_string()),
        }
        result
    }

    fn apply_inner(&mut self, command: FormCommand) -> Result<FormChange, Error> {
        let mut recipe = self.recipe.clone();
        match command {
            FormCommand::SetTitle(title) => recipe.title = title,
            FormCommand::SetDescription(description) => recipe.description = description,
            FormCommand::SetFilename(to) => {
                let to = to.trim().to_owned();
                if to.is_empty() {
                    return Err(Error::general_error("a filename cannot be empty".to_owned()));
                }
                return Ok(FormChange::Rename(to));
            }
            FormCommand::SetOption { field, text } => set_option(&mut recipe, &field, &text)?,
            FormCommand::SetOverride { name, text } => {
                let name = name.trim().to_owned();
                if name.is_empty() {
                    return Err(Error::general_error("an override needs an argument name".to_owned()));
                }
                let value = serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text));
                recipe.arguments.insert(name, value);
            }
            FormCommand::RemoveOverride(name) => {
                recipe.arguments.remove(&name);
                recipe.links.remove(&name);
            }
            FormCommand::ToggleOptions => {
                self.options_open = !self.options_open;
                return Ok(FormChange::None);
            }
            FormCommand::SetMode(mode) => {
                self.mode = mode;
                return Ok(FormChange::None);
            }
            FormCommand::Editor(EditorCommand::RenameNode { from, to }) => {
                return Ok(FormChange::RenameNode { from, to });
            }
            FormCommand::Editor(command) => match self.query.apply(command)? {
                EditorEffect::Write { value, .. } => {
                    self.query.on_written(zero());
                    match value.try_into_query() {
                        Ok(q) => {
                            recipe.query = q.encode();
                            recipe.pipeline = None;
                        }
                        Err(_) => {
                            recipe.pipeline = Some(value.try_into_pipeline()?);
                            recipe.query = String::new();
                        }
                    }
                }
                EditorEffect::ListDirectory(folder) => return Ok(FormChange::ListDirectory(folder)),
                EditorEffect::None
                | EditorEffect::Reload
                | EditorEffect::RenameData { .. }
                // A form writes its recipe; it has no live preview of the query.
                | EditorEffect::Preview(_)
                | EditorEffect::ClearPreview => return Ok(FormChange::None),
            },
        }
        self.recipe = recipe.clone();
        Ok(FormChange::Recipe(recipe))
    }

    /// A query editor command of the embedded file selector needs a directory listing.
    pub fn on_directory(&mut self, listing: crate::ui::editor::DirectoryListing) {
        self.query.on_directory(listing);
    }
}

fn set_option(recipe: &mut Recipe, field: &str, text: &str) -> Result<(), Error> {
    let text = text.trim();
    match field {
        "cwd" => recipe.cwd = (!text.is_empty()).then(|| text.to_owned()),
        "volatile" => recipe.volatile = parse_bool(text)?.unwrap_or(false),
        "stored" => recipe.stored = parse_bool(text)?,
        "expires" => {
            recipe.expires = if text.is_empty() {
                Default::default()
            } else {
                text.parse()?
            }
        }
        other => return Err(Error::general_error(format!("a recipe has no option '{other}'"))),
    }
    Ok(())
}

fn parse_bool(text: &str) -> Result<Option<bool>, Error> {
    match text {
        "" => Ok(None),
        "true" | "yes" | "on" => Ok(Some(true)),
        "false" | "no" | "off" => Ok(Some(false)),
        other => Err(Error::general_error(format!("'{other}' is not true or false"))),
    }
}
