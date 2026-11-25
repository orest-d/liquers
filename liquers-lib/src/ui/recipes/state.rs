//! [`RecipesState`]: the backend-free core of the recipes panel.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use liquers_core::accessor::Fingerprint;
use liquers_core::command_metadata::CommandMetadataRegistry;
use liquers_core::error::Error;
use liquers_core::recipe_tree::{node_name, ChildSlot, TreeEditor, TreeReport};
use liquers_core::recipes::{Recipe, RecipeList};
use liquers_core::value::ValueInterface;

use super::form::{FormChange, FormCommand, RecipeForm};
use super::rows::{RecipeLabel, RecipeRow, RecipesView, RowSource};
use super::yaml::parse_recipes;
use crate::ui::editor::NodeContext;
use crate::value::Value;

/// An edit or action of the recipes panel, from a renderer.
#[derive(Debug, Clone, PartialEq)]
pub enum RecipesCommand {
    /// Choose what rows show (AC-16).
    SetLabel(RecipeLabel),
    /// Choose the outline or the dependencies view (AC-11).
    SetView(RecipesView),
    /// Collapse or expand a row of the dependencies view, by row id.
    Toggle(String),
    /// Open (or focus) the recipe window of a row id (AC-4).
    Open(String),
    /// Close a recipe window.
    Close(String),
    /// Rename a node or file (AC-8): `to` is a name keeping the extension, or a filename.
    Rename { from: String, to: String },
    /// Append a new recipe titled `title` producing a file of `extension`, and open it (AC-7).
    New { title: String, extension: String },
    /// Add a child of `parent`, named in argument `arg` of its action `action` (AC-7).
    AddChild { parent: String, title: String, extension: String, action: usize, arg: String },
    /// Append a copy of a recipe under a new name (AC-9).
    Duplicate(String),
    /// Delete a recipe; refused while referenced (AC-9).
    Delete(String),
    /// Declare (or clear) the tree's result.
    SetResult(Option<String>),
    /// Show the value of a file in a window (AC-19).
    View(String),
    /// Show the metadata of a file in a window.
    Metadata(String),
    /// Start evaluating a file.
    Evaluate(String),
    /// Write a file's value to `path` on this computer, in `format` (Export) or its own.
    SaveAs { filename: String, format: Option<String>, path: String },
    /// An edit in a recipe window.
    Form { id: String, command: FormCommand },
    /// Open the YAML window (AC-12).
    OpenYaml,
    /// Close the YAML window, dropping what was typed.
    CloseYaml,
    /// What is typed in the YAML window.
    SetYaml(String),
    /// Apply the YAML window's text, if it parses.
    ApplyYaml,
    /// Write the pending edits (On demand, AC-13).
    Update,
    /// Drop the pending edits and read the file again.
    Revert,
    /// Apply immediately (`true`) or on demand (AC-13).
    SetContinuous(bool),
}

/// What the element must do after a command.
#[derive(Debug, Clone)]
pub enum RecipesEffect {
    /// Nothing.
    None,
    /// Write the document through the accessor if it still has the `expected` fingerprint.
    Write { value: Value, expected: Option<Fingerprint> },
    /// Save the recipes with their stored-file renames (`AppMessage::RenameData`).
    SaveTree { folder: String, editor: Box<TreeEditor> },
    /// Read the accessor again.
    Reload,
    /// Evaluate a query with the panel as the current element (`lui/add-child` opens a window).
    Submit(String),
    /// Start evaluating a query.
    Evaluate(String),
    /// Write a query's value to a file.
    SaveAs { query: String, format: Option<String>, path: String },
    /// List a store folder for the file selector of a recipe window; the answer goes to
    /// [`RecipesState::on_directory`].
    ListDirectory(String),
}

/// The YAML window's buffer (AC-12).
#[derive(Debug, Clone, PartialEq)]
pub struct YamlBuffer {
    /// The text as typed.
    pub text: String,
    /// The parse error of the text: its line (1-based) when known, and the message.
    pub error: Option<(Option<usize>, String)>,
    /// Typed since the last apply or load.
    pub dirty: bool,
}

/// The recipes panel: the working copy of a folder's `recipes.yaml`, the rows to draw, the open
/// recipe windows and the YAML window. See the module docs.
#[derive(Debug, Clone)]
pub struct RecipesState {
    registry: Arc<CommandMetadataRegistry>,
    folder: Option<String>,
    editor: Option<TreeEditor>,
    bytes: bool,
    fingerprint: Option<Fingerprint>,
    pending: bool,
    write_in_flight: bool,
    remote_changed: bool,
    continuous: bool,
    loaded: bool,
    read_only: Option<String>,
    raw_text: String,
    context: Option<NodeContext>,
    label: RecipeLabel,
    view: RecipesView,
    collapsed: BTreeSet<String>,
    forms: Vec<RecipeForm>,
    yaml: Option<YamlBuffer>,
    /// The recipe window whose file selector waits for a listing.
    browsing: Option<String>,
    rows: Vec<RecipeRow>,
    report: TreeReport,
    message: Option<String>,
}

/// The `folder/filename` key text of `filename` in `folder` (the root when empty).
fn key_in(folder: &str, filename: &str) -> String {
    if folder.is_empty() {
        filename.to_owned()
    } else {
        format!("{}/{filename}", folder.trim_end_matches('/'))
    }
}

/// The extension of a filename, without the dot.
fn extension(filename: &str) -> &str {
    match filename.rfind('.') {
        Some(i) if i > 0 => &filename[i + 1..],
        Some(_) | None => "",
    }
}

impl RecipesState {
    /// An empty panel; the document arrives with [`Self::on_snapshot`].
    pub fn new(registry: Arc<CommandMetadataRegistry>) -> Self {
        RecipesState {
            registry,
            folder: None,
            editor: None,
            bytes: false,
            fingerprint: None,
            pending: false,
            write_in_flight: false,
            remote_changed: false,
            continuous: true,
            loaded: false,
            read_only: None,
            raw_text: String::new(),
            context: None,
            label: RecipeLabel::Name,
            view: RecipesView::Outline,
            collapsed: BTreeSet::new(),
            forms: Vec::new(),
            yaml: None,
            browsing: None,
            rows: Vec::new(),
            report: TreeReport::default(),
            message: None,
        }
    }

    /// Sets the folder of the file, for the keys of View, Evaluate and Save as.
    pub fn with_folder(mut self, folder: Option<String>) -> Self {
        self.folder = folder;
        self
    }

    /// Sets what rows show and the view (builder style).
    pub fn with_display(mut self, label: RecipeLabel, view: RecipesView) -> Self {
        self.label = label;
        self.view = view;
        self.rebuild();
        self
    }

    /// Apply immediately (`true`, the default) or on demand.
    pub fn with_continuous(mut self, continuous: bool) -> Self {
        self.continuous = continuous;
        self
    }

    /// The folder, when known.
    pub fn folder(&self) -> Option<&str> {
        self.folder.as_deref()
    }
    /// What rows show.
    pub fn label(&self) -> RecipeLabel {
        self.label
    }
    /// The view.
    pub fn view(&self) -> RecipesView {
        self.view
    }
    /// The rows to draw.
    pub fn rows(&self) -> &[RecipeRow] {
        &self.rows
    }
    /// The open recipe windows, the last focused last.
    pub fn forms(&self) -> &[RecipeForm] {
        &self.forms
    }
    /// The open recipe windows, mutable (for the renderer's typing state).
    pub fn forms_mut(&mut self) -> &mut [RecipeForm] {
        &mut self.forms
    }
    /// The recipe window of a row id.
    pub fn form(&self, id: &str) -> Option<&RecipeForm> {
        self.forms.iter().find(|f| f.id == id)
    }
    /// The YAML window, when open.
    pub fn yaml(&self) -> Option<&YamlBuffer> {
        self.yaml.as_ref()
    }
    /// The last message: a refused edit, a conflict, a notice.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
    /// Why the panel cannot write, if it cannot.
    pub fn read_only(&self) -> Option<&str> {
        self.read_only.as_deref()
    }
    /// Whether edits wait for Update.
    pub fn is_pending(&self) -> bool {
        self.pending
    }
    /// Apply immediately.
    pub fn continuous(&self) -> bool {
        self.continuous
    }
    /// Whether the document has a `tree:` section.
    pub fn is_tree(&self) -> bool {
        self.list().is_some_and(|l| l.tree.is_some())
    }
    /// The check of the working copy.
    pub fn report(&self) -> &TreeReport {
        &self.report
    }
    /// The working copy, once loaded and parsed.
    pub fn list(&self) -> Option<&RecipeList> {
        self.editor.as_ref().map(TreeEditor::recipes)
    }
    /// Whether a row's asset is being made, so its status should be refreshed.
    pub fn busy(&self) -> bool {
        self.rows.iter().any(|r| r.status.is_busy())
    }
    /// The document as YAML text.
    pub fn document_text(&self) -> String {
        match self.list() {
            Some(list) => serde_yaml::to_string(list).unwrap_or_default(),
            None => self.raw_text.clone(),
        }
    }
    /// The Node arguments of `parent` a new child can be named in.
    pub fn child_slots(&self, parent: &str) -> Vec<ChildSlot> {
        self.editor
            .as_ref()
            .and_then(|e| e.child_slots(parent).ok())
            .unwrap_or_default()
    }

    fn document_value(&self) -> Value {
        let text = self.document_text();
        if self.bytes {
            Value::from_bytes(text.into_bytes())
        } else {
            Value::from_string(text)
        }
    }

    fn listing(&self) -> Option<Vec<String>> {
        self.context.as_ref().and_then(|c| c.listing.clone())
    }

    /// The document as the accessor read it (AC-15): a missing file is an empty list.
    pub fn on_snapshot(&mut self, value: &Value, fingerprint: Option<Fingerprint>) -> Result<(), Error> {
        if self.loaded && fingerprint.is_some() && fingerprint == self.fingerprint {
            return Ok(());
        }
        if self.pending || self.write_in_flight {
            self.remote_changed = true;
            self.message = Some(
                "The recipes were changed elsewhere; Update reports a conflict, Revert loads them.".to_owned(),
            );
            return Ok(());
        }
        self.loaded = true;
        self.fingerprint = fingerprint;
        self.remote_changed = false;
        let text = match value.identifier().as_ref() {
            "None" => String::new(),
            "Bytes" => {
                self.bytes = true;
                String::from_utf8_lossy(&value.try_into_bytes()?).into_owned()
            }
            _ => {
                self.bytes = false;
                value.try_into_string()?
            }
        };
        self.raw_text = text.clone();
        match parse_recipes(&text) {
            Ok(list) => {
                self.editor = Some(TreeEditor::new(list, self.listing(), self.registry.clone())?);
                if self.read_only.as_deref().is_some_and(|r| r.starts_with("recipes.yaml does not parse")) {
                    self.read_only = None;
                }
                let text = self.document_text();
                if let Some(yaml) = &mut self.yaml {
                    if !yaml.dirty {
                        yaml.text = text;
                        yaml.error = None;
                    }
                }
                self.sync_forms();
                self.rebuild();
                Ok(())
            }
            Err((line, message)) => {
                self.editor = None;
                self.forms.clear();
                self.read_only = Some(format!("recipes.yaml does not parse: {message}"));
                // Open the text so it can be repaired.
                self.yaml = Some(YamlBuffer { text, error: Some((line, message.clone())), dirty: false });
                self.rebuild();
                Err(Error::conversion_error_with_message("YAML", "RecipeList", &message))
            }
        }
    }

    /// A write succeeded; edits made meanwhile are written next.
    pub fn on_written(&mut self, fingerprint: Fingerprint) -> RecipesEffect {
        self.write_in_flight = false;
        self.fingerprint = Some(fingerprint);
        if self.pending && self.continuous {
            self.write()
        } else {
            RecipesEffect::None
        }
    }

    /// A write was refused because the file changed meanwhile (AC-15): the live file is loaded,
    /// and the message says what was not written.
    pub fn on_conflict(&mut self, live: &Value, fingerprint: Option<Fingerprint>) {
        self.write_in_flight = false;
        self.pending = false;
        self.remote_changed = false;
        self.loaded = false;
        let _ = self.on_snapshot(live, fingerprint);
        self.message = Some(
            "Conflict: the recipes were changed elsewhere and have been reloaded; your last edits were not written."
                .to_owned(),
        );
    }

    /// A write failed for another reason: the edits stay pending.
    pub fn on_write_failed(&mut self, error: &Error) {
        self.write_in_flight = false;
        self.pending = true;
        self.message = Some(error.to_string());
    }

    /// A store listing for the recipe window that asked for one.
    pub fn on_directory(&mut self, listing: crate::ui::editor::DirectoryListing) {
        if let Some(id) = self.browsing.take() {
            if let Some(form) = self.forms.iter_mut().find(|f| f.id == id) {
                form.on_directory(listing);
            }
        }
    }

    /// The accessor cannot be written.
    pub fn set_read_only(&mut self, reason: Option<String>) {
        self.read_only = reason;
        self.rebuild();
    }

    /// The folder's listing, asset information and formats from the runner (AC-2, AC-17).
    pub fn on_context(&mut self, context: NodeContext) {
        if let Some(notice) = &context.notice {
            self.message = Some(notice.clone());
        }
        let listing = context.listing.clone();
        self.context = Some(context);
        // The listing feeds the check (sources, dangling references); keep queued renames.
        if let Some(editor) = &self.editor {
            if editor.pending_data_renames().is_empty() {
                if let Ok(fresh) = TreeEditor::new(editor.recipes().clone(), listing, self.registry.clone()) {
                    self.editor = Some(fresh);
                }
            }
        }
        self.sync_forms();
        self.rebuild();
    }

    fn rebuild(&mut self) {
        let empty_assets = BTreeMap::new();
        let empty_formats = BTreeMap::new();
        let Some(editor) = &self.editor else {
            self.rows.clear();
            self.report = TreeReport::default();
            return;
        };
        self.report = editor.report();
        let source = RowSource {
            list: editor.recipes(),
            report: &self.report,
            assets: self.context.as_ref().map(|c| &c.assets).unwrap_or(&empty_assets),
            formats: self.context.as_ref().map(|c| &c.formats).unwrap_or(&empty_formats),
        };
        self.rows = match self.view {
            RecipesView::Outline => source.outline(),
            RecipesView::Dependencies => source.dependencies(&self.collapsed),
        };
    }

    /// The recipe of a row id: a filename, or `#<index>`.
    fn recipe_of(&self, id: &str) -> Option<Recipe> {
        let list = self.list()?;
        match id.strip_prefix('#').and_then(|i| i.parse::<usize>().ok()) {
            Some(index) => list.recipes.get(index).cloned(),
            None => list.get(id).cloned(),
        }
    }

    /// Reloads every open form from the working copy, closing those whose recipe is gone.
    fn sync_forms(&mut self) {
        let folder = self.folder.clone().unwrap_or_default();
        let context = self
            .list()
            .map(|list| NodeContext::new(folder, list.clone(), self.listing()));
        let mut forms = std::mem::take(&mut self.forms);
        forms.retain_mut(|form| match self.recipe_of(&form.id) {
            Some(recipe) => {
                if recipe != *form.recipe() {
                    form.reload(recipe);
                }
                form.set_node_context(context.clone());
                true
            }
            None => false,
        });
        self.forms = forms;
    }

    /// After a successful edit of the working copy.
    fn edited(&mut self) -> RecipesEffect {
        self.message = None;
        self.sync_forms();
        self.rebuild();
        self.pending = true;
        if self.continuous {
            self.write()
        } else {
            RecipesEffect::None
        }
    }

    fn write(&mut self) -> RecipesEffect {
        if self.write_in_flight || !self.pending {
            return RecipesEffect::None;
        }
        let Some(editor) = &self.editor else {
            return RecipesEffect::None;
        };
        if !editor.pending_data_renames().is_empty() {
            if self.remote_changed {
                // `TreeEditor::save` writes the whole list with no fingerprint check: refuse
                // rather than overwrite the edits made elsewhere.
                self.pending = false;
                self.remote_changed = false;
                self.loaded = false;
                self.message = Some(
                    "Conflict: the recipes were changed elsewhere and have been reloaded; the rename was not saved."
                        .to_owned(),
                );
                return RecipesEffect::Reload;
            }
            let Some(folder) = self.folder.clone() else {
                self.message = Some("The folder of the recipes is not known; stored files cannot be renamed.".to_owned());
                return RecipesEffect::None;
            };
            let saved = Box::new(editor.clone());
            // The renames are done by the save; the working copy starts afresh.
            if let Ok(fresh) = TreeEditor::new(editor.recipes().clone(), self.listing(), self.registry.clone()) {
                self.editor = Some(fresh);
            }
            self.pending = false;
            self.loaded = false;
            return RecipesEffect::SaveTree { folder, editor: saved };
        }
        self.pending = false;
        // The fingerprint check of the write reports a change made elsewhere as a conflict.
        self.remote_changed = false;
        self.write_in_flight = true;
        RecipesEffect::Write { value: self.document_value(), expected: self.fingerprint }
    }

    fn writable(&self) -> Result<&TreeEditor, Error> {
        if let Some(reason) = &self.read_only {
            return Err(Error::general_error(reason.clone()));
        }
        self.editor
            .as_ref()
            .ok_or_else(|| Error::general_error("The recipes have not been read yet.".to_owned()))
    }

    /// Runs `edit` on a copy of the working copy and keeps it only when it succeeds (AC-14).
    fn edit(&mut self, edit: impl FnOnce(&mut TreeEditor) -> Result<(), Error>) -> Result<RecipesEffect, Error> {
        let mut copy = self.writable()?.clone();
        if let Err(e) = edit(&mut copy) {
            self.message = Some(e.to_string());
            return Err(e);
        }
        self.editor = Some(copy);
        Ok(self.edited())
    }

    fn key(&self, filename: &str) -> Result<String, Error> {
        let folder = self
            .folder
            .as_deref()
            .ok_or_else(|| Error::general_error("The folder of the recipes is not known.".to_owned()))?;
        Ok(key_in(folder, filename))
    }

    /// The filename a rename of `from` to `to` gives.
    fn renamed_file(from: &str, to: &str) -> String {
        if to.contains('.') {
            to.to_owned()
        } else {
            match extension(from) {
                "" => to.to_owned(),
                ext => format!("{to}.{ext}"),
            }
        }
    }

    fn resolve_file(&self, node: &str) -> String {
        if self.list().is_some_and(|l| l.get(node).is_some()) {
            return node.to_owned();
        }
        self.rows
            .iter()
            .chain(std::iter::empty())
            .find(|r| r.name == node)
            .and_then(|r| r.filename.clone())
            .or_else(|| {
                self.list().and_then(|l| {
                    l.recipes
                        .iter()
                        .filter_map(|r| r.filename().ok().flatten().map(|f| f.name))
                        .find(|f| node_name(f) == node)
                })
            })
            .unwrap_or_else(|| node.to_owned())
    }

    fn open(&mut self, id: &str) -> Result<(), Error> {
        if let Some(i) = self.forms.iter().position(|f| f.id == id) {
            let form = self.forms.remove(i);
            self.forms.push(form);
            return Ok(());
        }
        let recipe = self
            .recipe_of(id)
            .ok_or_else(|| Error::general_error(format!("There is no recipe '{id}'.")))?;
        let mut form = RecipeForm::new(id, recipe, self.registry.clone());
        let folder = self.folder.clone().unwrap_or_default();
        form.set_node_context(self.list().map(|l| NodeContext::new(folder, l.clone(), self.listing())));
        self.forms.push(form);
        Ok(())
    }

    fn rekey(&mut self, from: &str, to: &str) {
        for form in self.forms.iter_mut() {
            if form.id == from {
                form.id = to.to_owned();
            }
        }
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<RecipesEffect, Error> {
        let from_file = self.resolve_file(from);
        let to_file = Self::renamed_file(&from_file, to);
        // Re-key the window first: the edit reloads the forms by their ids.
        self.rekey(&from_file, &to_file);
        match self.edit(|e| e.rename(&from_file, to).map(|_| ())) {
            Ok(effect) => Ok(effect),
            Err(e) => {
                self.rekey(&to_file, &from_file);
                Err(e)
            }
        }
    }

    fn form_change(&mut self, id: &str, change: FormChange) -> Result<RecipesEffect, Error> {
        match change {
            FormChange::None => Ok(RecipesEffect::None),
            FormChange::ListDirectory(folder) => {
                self.browsing = Some(id.to_owned());
                Ok(RecipesEffect::ListDirectory(folder))
            }
            FormChange::Rename(to) => self.rename(id, &to),
            FormChange::RenameNode { from, to } => self.rename(&from, &to),
            FormChange::Recipe(recipe) => {
                let new_file = recipe.filename().ok().flatten().map(|f| f.name);
                // Re-key the window first: the edit reloads the forms by their ids.
                let rekeyed = new_file.clone().filter(|to| !id.starts_with('#') && to != id);
                if let Some(to) = &rekeyed {
                    self.rekey(id, to);
                }
                let result = match id.strip_prefix('#').and_then(|i| i.parse::<usize>().ok()) {
                    Some(index) => self.edit(|e| {
                        let mut list = e.recipes().clone();
                        match list.recipes.get_mut(index) {
                            Some(slot) => *slot = recipe.clone(),
                            None => return Err(Error::general_error(format!("There is no recipe {index}."))),
                        }
                        e.replace(list).map(|_| ())
                    }),
                    None => {
                        let id_owned = id.to_owned();
                        let target = new_file.clone();
                        self.edit(|e| {
                            match target.as_deref() {
                                Some(to) if to != id_owned => {
                                    // The query's filename changed: rename first, rewriting the
                                    // folder's references, then set the recipe.
                                    e.rename(&id_owned, to)?;
                                    e.set_recipe(to, recipe.clone())?;
                                }
                                Some(_) => {
                                    e.set_recipe(&id_owned, recipe.clone())?;
                                }
                                None => {
                                    return Err(Error::general_error(
                                        "A recipe of the folder must keep a filename.".to_owned(),
                                    ))
                                }
                            }
                            Ok(())
                        })
                    }
                };
                match result {
                    Ok(effect) => Ok(effect),
                    Err(e) => {
                        if let Some(to) = &rekeyed {
                            self.rekey(to, id);
                        }
                        // Undo the form's own copy.
                        if let Some(original) = self.recipe_of(id) {
                            if let Some(form) = self.forms.iter_mut().find(|f| f.id == id) {
                                form.reload(original);
                                form.message = Some(e.to_string());
                            }
                        }
                        Err(e)
                    }
                }
            }
        }
    }

    /// Applies one command. A refused edit changes nothing; its error is also kept as the
    /// message (AC-14).
    pub fn apply(&mut self, command: RecipesCommand) -> Result<RecipesEffect, Error> {
        match command {
            RecipesCommand::SetLabel(label) => {
                self.label = label;
                Ok(RecipesEffect::None)
            }
            RecipesCommand::SetView(view) => {
                self.view = view;
                self.rebuild();
                Ok(RecipesEffect::None)
            }
            RecipesCommand::Toggle(id) => {
                if !self.collapsed.remove(&id) {
                    self.collapsed.insert(id);
                }
                self.rebuild();
                Ok(RecipesEffect::None)
            }
            RecipesCommand::Open(id) => {
                self.open(&id)?;
                Ok(RecipesEffect::None)
            }
            RecipesCommand::Close(id) => {
                self.forms.retain(|f| f.id != id);
                Ok(RecipesEffect::None)
            }
            RecipesCommand::Rename { from, to } => self.rename(&from, &to),
            RecipesCommand::New { title, extension } => {
                let editor = self.writable()?;
                let filename = editor.suggest_filename(&title, extension.trim_start_matches('.'));
                let recipe = Recipe::new(filename.clone(), title, String::new())?;
                let effect = self.edit(|e| e.add(recipe).map(|_| ()))?;
                self.open(&filename)?;
                Ok(effect)
            }
            RecipesCommand::AddChild { parent, title, extension, action, arg } => {
                let editor = self.writable()?;
                let filename = editor.suggest_filename(&title, extension.trim_start_matches('.'));
                let recipe = Recipe::new(filename.clone(), title, String::new())?;
                let parent = self.resolve_file(&parent);
                let effect = self.edit(|e| e.add_child(&parent, recipe, action, &arg).map(|_| ()))?;
                self.open(&filename)?;
                Ok(effect)
            }
            RecipesCommand::Duplicate(node) => {
                let file = self.resolve_file(&node);
                let editor = self.writable()?;
                let to = editor.suggest_filename(&format!("{} copy", node_name(&file)), extension(&file));
                self.edit(|e| e.duplicate(&file, &to).map(|_| ()))
            }
            RecipesCommand::Delete(node) => {
                let file = self.resolve_file(&node);
                let effect = self.edit(|e| e.delete(&file, false).map(|_| ()))?;
                self.forms.retain(|f| f.id != file);
                Ok(effect)
            }
            RecipesCommand::SetResult(node) => {
                let file = node.map(|n| self.resolve_file(&n));
                self.edit(|e| e.set_result(file.as_deref()).map(|_| ()))
            }
            RecipesCommand::View(filename) => {
                Ok(RecipesEffect::Submit(format!("-R/{}/-/ns-lui/add-child", self.key(&filename)?)))
            }
            RecipesCommand::Metadata(filename) => {
                Ok(RecipesEffect::Submit(format!("-R-meta/{}/-/ns-lui/add-child", self.key(&filename)?)))
            }
            RecipesCommand::Evaluate(filename) => Ok(RecipesEffect::Evaluate(format!("-R/{}", self.key(&filename)?))),
            RecipesCommand::SaveAs { filename, format, path } => {
                if path.trim().is_empty() {
                    return Err(Error::general_error("Save as needs a file path.".to_owned()));
                }
                Ok(RecipesEffect::SaveAs { query: format!("-R/{}", self.key(&filename)?), format, path })
            }
            RecipesCommand::Form { id, command } => {
                let read_only = self.read_only.clone();
                let Some(form) = self.forms.iter_mut().find(|f| f.id == id) else {
                    return Err(Error::general_error(format!("The recipe window '{id}' is not open.")));
                };
                let change = form.apply(command)?;
                if let (Some(reason), FormChange::Recipe(_) | FormChange::Rename(_) | FormChange::RenameNode { .. }) =
                    (&read_only, &change)
                {
                    let reason = reason.clone();
                    if let Some(original) = self.recipe_of(&id) {
                        if let Some(form) = self.forms.iter_mut().find(|f| f.id == id) {
                            form.reload(original);
                        }
                    }
                    return Err(Error::general_error(reason));
                }
                self.form_change(&id, change)
            }
            RecipesCommand::OpenYaml => {
                if self.yaml.is_none() {
                    self.yaml = Some(YamlBuffer { text: self.document_text(), error: None, dirty: false });
                }
                Ok(RecipesEffect::None)
            }
            RecipesCommand::CloseYaml => {
                self.yaml = None;
                Ok(RecipesEffect::None)
            }
            RecipesCommand::SetYaml(text) => {
                let error = parse_recipes(&text).err();
                self.yaml = Some(YamlBuffer { text, error, dirty: true });
                Ok(RecipesEffect::None)
            }
            RecipesCommand::ApplyYaml => {
                let Some(buffer) = self.yaml.clone() else {
                    return Ok(RecipesEffect::None);
                };
                let list = match parse_recipes(&buffer.text) {
                    Ok(list) => list,
                    Err((line, message)) => {
                        self.yaml = Some(YamlBuffer { error: Some((line, message.clone())), ..buffer });
                        return Err(Error::conversion_error_with_message("YAML", "RecipeList", &message));
                    }
                };
                if self.editor.is_none() {
                    if self.read_only.as_deref().is_some_and(|r| !r.starts_with("recipes.yaml does not parse")) {
                        return Err(Error::general_error(self.read_only.clone().unwrap_or_default()));
                    }
                    self.read_only = None;
                    self.editor = Some(TreeEditor::new(RecipeList::new(), self.listing(), self.registry.clone())?);
                }
                let effect = self.edit(|e| e.replace(list).map(|_| ()))?;
                self.yaml = Some(YamlBuffer { text: self.document_text(), error: None, dirty: false });
                Ok(effect)
            }
            RecipesCommand::Update => Ok(self.write()),
            RecipesCommand::Revert => {
                self.pending = false;
                self.remote_changed = false;
                self.loaded = false;
                Ok(RecipesEffect::Reload)
            }
            RecipesCommand::SetContinuous(continuous) => {
                self.continuous = continuous;
                if continuous && self.pending {
                    Ok(self.write())
                } else {
                    Ok(RecipesEffect::None)
                }
            }
        }
    }
}
