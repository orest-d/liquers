//! [`RecipesElement`] and [`RecipeElement`]: the `UIElement`s of the recipes panel and of the
//! single-recipe form.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use liquers_core::accessor::{Fingerprint, ValueAccessor};
use liquers_core::error::Error;
use liquers_core::recipes::Recipe;
use liquers_core::value::ValueInterface;

use super::form::{FormChange, FormCommand, RecipeForm};
use super::rows::{RecipeLabel, RecipesView};
use super::state::{RecipesCommand, RecipesEffect, RecipesState};
use crate::ui::editor::{EditorBinding, EditorCommand};
use crate::ui::element::{UIElement, UpdateMessage, UpdateResponse};
use crate::ui::event::{UIEvent, UIEventKind};
use crate::ui::handle::UIHandle;
use crate::ui::message::{AccessorOp, AccessorUpdate, AppMessage};
use crate::ui::ui_context::UIContext;
use crate::value::Value;

fn yes() -> bool {
    true
}

/// The folder of the `recipes.yaml` an accessor address reads
/// (`-R-key/scenes/car/recipes.yaml/-/ns-acc/accessor` → `scenes/car`; the root → `""`).
pub fn recipes_folder(address: &str) -> Option<String> {
    let end = address.find("recipes.yaml")?;
    let head = address[..end].trim_end_matches('/');
    for prefix in ["-R-key", "-R-stored", "-R"] {
        if head == prefix {
            return Some(String::new());
        }
        if let Some(folder) = head.strip_prefix(prefix).and_then(|h| h.strip_prefix('/')) {
            if folder.contains("/-/") || folder.starts_with('-') {
                return None;
            }
            return Some(folder.to_owned());
        }
    }
    None
}

/// Transient state of the egui renderer of the panel (dialogs and inline edits).
#[derive(Debug, Clone, Default)]
pub struct RecipesUi {
    /// The row being renamed inline, and the text typed.
    pub renaming: Option<(String, String)>,
    /// The New dialog: title and extension; `Some` while open. With a parent, Add child.
    pub new_dialog: Option<NewDialog>,
    /// The filename whose Save as / Export dialog is pending, and its format.
    pub save_as: Option<(String, Option<String>, String)>,
    /// The settings popup is open.
    pub settings_open: bool,
    /// The egui time of the last status refresh.
    pub last_refresh: f64,
}

/// The New / Add child dialog.
#[derive(Debug, Clone, Default)]
pub struct NewDialog {
    pub parent: Option<String>,
    pub title: String,
    pub extension: String,
    /// Index into the parent's child slots.
    pub slot: usize,
}

/// The recipes panel: a narrow side panel over a folder's `recipes.yaml` (see the module docs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipesElement {
    handle: Option<UIHandle>,
    title: String,
    binding: EditorBinding,
    #[serde(default)]
    label: RecipeLabel,
    #[serde(default)]
    view: RecipesView,
    #[serde(default = "yes")]
    continuous: bool,
    #[serde(skip)]
    state: Option<RecipesState>,
    #[serde(skip)]
    bound: bool,
    #[serde(skip)]
    live: Option<Arc<dyn ValueAccessor<Value>>>,
    /// What the egui renderer is in the middle of.
    #[serde(skip)]
    pub ui: RecipesUi,
}

impl RecipesElement {
    /// A panel over `binding` (normally the accessor of a `recipes.yaml`).
    pub fn new(title: impl Into<String>, binding: EditorBinding) -> Self {
        RecipesElement {
            handle: None,
            title: title.into(),
            binding,
            label: RecipeLabel::Name,
            view: RecipesView::Outline,
            continuous: true,
            state: None,
            bound: false,
            live: None,
            ui: RecipesUi::default(),
        }
    }

    /// A panel over an accessor value: its address, or the accessor itself when made in code.
    pub fn from_accessor(title: impl Into<String>, accessor: Arc<dyn ValueAccessor<Value>>) -> Self {
        let address = accessor.address().encode();
        if address.is_empty() {
            let mut element = RecipesElement::new(title, EditorBinding::Memory(String::new()));
            element.live = Some(accessor);
            element
        } else {
            RecipesElement::new(title, EditorBinding::Accessor(address))
        }
    }

    /// Rows show `label` (builder style).
    pub fn with_label(mut self, label: RecipeLabel) -> Self {
        self.label = label;
        self
    }

    /// The binding.
    pub fn binding(&self) -> &EditorBinding {
        &self.binding
    }

    /// The panel's core, once it has a registry.
    pub fn state(&self) -> Option<&RecipesState> {
        self.state.as_ref()
    }

    /// The panel's core, mutable (renderers keep typing state in its forms).
    pub fn state_mut(&mut self) -> Option<&mut RecipesState> {
        self.state.as_mut()
    }

    /// The folder of the file, from the binding.
    pub fn folder(&self) -> Option<String> {
        match &self.binding {
            EditorBinding::Accessor(address) => recipes_folder(address),
            EditorBinding::Memory(_) => None,
        }
    }

    fn ensure_state(&mut self, ctx: &UIContext) {
        if self.state.is_none() {
            if let Some(registry) = ctx.command_registry() {
                self.state = Some(
                    RecipesState::new(registry.clone())
                        .with_folder(self.folder())
                        .with_display(self.label, self.view)
                        .with_continuous(self.continuous),
                );
            }
        }
        if !self.bound && self.state.is_some() {
            if let Some(handle) = self.handle {
                self.bound = true;
                let op = match self.live.take() {
                    Some(accessor) => AccessorOp::Adopt(accessor),
                    None => AccessorOp::Watch,
                };
                ctx.send_message(AppMessage::Accessor { handle, binding: self.binding.clone(), op });
                self.refresh_status(ctx);
            }
        }
    }

    /// Asks the runner for the folder's listing and asset information again.
    pub fn refresh_status(&self, ctx: &UIContext) {
        if let (Some(handle), Some(folder)) = (self.handle, self.folder()) {
            ctx.send_message(AppMessage::NodeContext { handle, folder });
        }
    }

    /// Applies commands from a renderer and carries out their effects. Returns whether any
    /// was applied.
    pub fn apply_commands(&mut self, commands: Vec<RecipesCommand>, ctx: &UIContext) -> bool {
        if commands.is_empty() {
            return false;
        }
        self.ensure_state(ctx);
        let mut effects = Vec::new();
        if let Some(state) = &mut self.state {
            for command in commands {
                // A refused command is the state's message; the others still apply.
                if let Ok(effect) = state.apply(command) {
                    effects.push(effect);
                }
            }
            self.label = state.label();
            self.view = state.view();
            self.continuous = state.continuous();
        }
        for effect in effects {
            self.run_effect(effect, ctx);
        }
        true
    }

    fn run_effect(&mut self, effect: RecipesEffect, ctx: &UIContext) {
        let Some(handle) = self.handle else {
            return;
        };
        match effect {
            RecipesEffect::None => {}
            RecipesEffect::Write { value, expected } => ctx.send_message(AppMessage::Accessor {
                handle,
                binding: self.binding.clone(),
                op: AccessorOp::SetIf { value, expected },
            }),
            RecipesEffect::SaveTree { folder, editor } => {
                ctx.send_message(AppMessage::RenameData { handle, folder, editor })
            }
            RecipesEffect::Reload => ctx.send_message(AppMessage::Accessor {
                handle,
                binding: self.binding.clone(),
                op: AccessorOp::Get,
            }),
            RecipesEffect::Submit(query) => ctx.submit_query(handle, query),
            RecipesEffect::Evaluate(query) => {
                ctx.send_message(AppMessage::Evaluate { query });
                self.refresh_status(ctx);
            }
            RecipesEffect::SaveAs { query, format, path } => {
                ctx.send_message(AppMessage::SaveAs { handle, query, format, path })
            }
        }
    }

    fn on_accessor(&mut self, update: &AccessorUpdate, ctx: &UIContext) {
        let mut effect = RecipesEffect::None;
        let mut written = false;
        if let Some(state) = &mut self.state {
            match update {
                AccessorUpdate::Bound { writable, .. } => {
                    if !writable {
                        state.set_read_only(Some("The recipes cannot be written.".to_owned()));
                    }
                }
                AccessorUpdate::Snapshot { value, fingerprint } => {
                    let _ = state.on_snapshot(value, *fingerprint);
                }
                AccessorUpdate::Written { fingerprint } => {
                    effect = state.on_written(*fingerprint);
                    written = true;
                }
                AccessorUpdate::Conflict { value, fingerprint } => state.on_conflict(value, *fingerprint),
                AccessorUpdate::Failed(error) => {
                    if state.list().is_none() {
                        state.set_read_only(Some(error.to_string()));
                    } else {
                        state.on_write_failed(error);
                    }
                }
            }
        }
        self.run_effect(effect, ctx);
        if written {
            self.refresh_status(ctx);
        }
    }
}

/// The text of a field of a browser event.
fn field_text(event: &UIEvent, name: &str) -> Option<String> {
    event.field(name).map(|v| v.try_into_string().unwrap_or_default())
}

/// The commands of a browser event of the panel (see `ui::web::recipes` for the names).
pub fn panel_commands(event: &UIEvent, state: &RecipesState) -> Vec<RecipesCommand> {
    let name = match &event.kind {
        UIEventKind::Named(name) => name.clone(),
        UIEventKind::Submit => return vec![],
        UIEventKind::Change(field) => match field.as_str() {
            "label" => {
                return field_text(event, "label")
                    .and_then(|l| RecipeLabel::parse(&l))
                    .map(RecipesCommand::SetLabel)
                    .into_iter()
                    .collect()
            }
            _ => return vec![],
        },
    };
    let Some(rest) = name.strip_prefix("rc:").or_else(|| name.strip_prefix("rf:")) else {
        return vec![];
    };
    let form_event = name.starts_with("rf:");
    let (verb, arg) = rest.split_once(':').unwrap_or((rest, ""));
    if form_event {
        return form_commands(event, verb, arg, state);
    }
    let arg = arg.to_owned();
    match verb {
        "open" => vec![RecipesCommand::Open(arg)],
        "close" => vec![RecipesCommand::Close(arg)],
        "view" => vec![RecipesCommand::View(arg)],
        "metadata" => vec![RecipesCommand::Metadata(arg)],
        "evaluate" => vec![RecipesCommand::Evaluate(arg)],
        "duplicate" => vec![RecipesCommand::Duplicate(arg)],
        "delete" => vec![RecipesCommand::Delete(arg)],
        "result" => vec![RecipesCommand::SetResult(Some(arg))],
        "toggle" => vec![RecipesCommand::Toggle(arg)],
        "rename" => match field_text(event, &format!("rename:{arg}")) {
            Some(to) if !to.trim().is_empty() => vec![RecipesCommand::Rename { from: arg, to: to.trim().to_owned() }],
            Some(_) | None => vec![],
        },
        "label" => RecipeLabel::parse(&arg).map(RecipesCommand::SetLabel).into_iter().collect(),
        "mode" => match arg.as_str() {
            "outline" => vec![RecipesCommand::SetView(RecipesView::Outline)],
            "dependencies" => vec![RecipesCommand::SetView(RecipesView::Dependencies)],
            _ => vec![],
        },
        "new" => vec![RecipesCommand::New {
            title: field_text(event, "new_title").unwrap_or_default(),
            extension: field_text(event, "new_ext").unwrap_or_default(),
        }],
        "addchild" => {
            let slots = state.child_slots(&arg);
            let slot = field_text(event, &format!("child_slot:{arg}"))
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0);
            match slots.get(slot) {
                Some(slot) => vec![RecipesCommand::AddChild {
                    parent: arg.clone(),
                    title: field_text(event, &format!("child_title:{arg}")).unwrap_or_default(),
                    extension: slot.extension.clone(),
                    action: slot.action,
                    arg: slot.arg.clone(),
                }],
                None => vec![],
            }
        }
        "yaml" => vec![RecipesCommand::OpenYaml],
        "yaml-close" => vec![RecipesCommand::CloseYaml],
        "yaml-apply" => match field_text(event, "yaml") {
            Some(text) => vec![RecipesCommand::SetYaml(text), RecipesCommand::ApplyYaml],
            None => vec![RecipesCommand::ApplyYaml],
        },
        "update" => vec![RecipesCommand::Update],
        "revert" => vec![RecipesCommand::Revert],
        "continuous" => vec![RecipesCommand::SetContinuous(arg == "on")],
        _ => vec![],
    }
}

/// The form commands of a browser `rf:save:<id>` event: one per changed field.
fn form_commands(event: &UIEvent, verb: &str, id: &str, state: &RecipesState) -> Vec<RecipesCommand> {
    let Some(form) = state.form(id) else {
        return vec![];
    };
    let mut commands = Vec::new();
    let mut push = |command: FormCommand| commands.push(RecipesCommand::Form { id: id.to_owned(), command });
    match verb {
        "save" => {
            for change in form_field_changes(event, form) {
                push(change);
            }
        }
        "options" => push(FormCommand::ToggleOptions),
        _ => {}
    }
    commands
}

/// The edits a browser form's fields make to `form`'s recipe.
pub fn form_field_changes(event: &UIEvent, form: &RecipeForm) -> Vec<FormCommand> {
    let recipe = form.recipe();
    let mut out = Vec::new();
    if let Some(title) = field_text(event, "title").filter(|t| *t != recipe.title) {
        out.push(FormCommand::SetTitle(title));
    }
    if let Some(description) = field_text(event, "description").filter(|t| *t != recipe.description) {
        out.push(FormCommand::SetDescription(description));
    }
    if let Some(query) = field_text(event, "query") {
        let current = form.query_editor().tree().text.clone();
        if query.trim() != current {
            out.push(FormCommand::Editor(EditorCommand::SetText(query.trim().to_owned())));
        }
    }
    for (field, current) in [
        ("cwd", recipe.cwd.clone().unwrap_or_default()),
        ("volatile", if recipe.volatile { "true".to_owned() } else { String::new() }),
        ("expires", recipe.expires.to_string()),
        ("stored", recipe.stored.map(|s| s.to_string()).unwrap_or_default()),
    ] {
        if let Some(text) = field_text(event, field) {
            if text.trim() != current && !(field == "volatile" && text.trim() == "false" && current.is_empty()) {
                out.push(FormCommand::SetOption { field: field.to_owned(), text });
            }
        }
    }
    if let Some(filename) = field_text(event, "filename") {
        if Some(filename.trim().to_owned()) != form.filename() && !filename.trim().is_empty() {
            out.push(FormCommand::SetFilename(filename));
        }
    }
    out
}

#[typetag::serde]
impl UIElement for RecipesElement {
    fn type_name(&self) -> &'static str {
        "RecipesElement"
    }
    fn handle(&self) -> Option<UIHandle> {
        self.handle
    }
    fn set_handle(&mut self, handle: UIHandle) {
        self.handle = Some(handle);
    }
    fn title(&self) -> String {
        self.title.clone()
    }
    fn set_title(&mut self, title: String) {
        self.title = title;
    }
    fn clone_boxed(&self) -> Box<dyn UIElement> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn init(&mut self, handle: UIHandle, ctx: &UIContext) -> Result<(), Error> {
        self.set_handle(handle);
        self.bound = false;
        self.ensure_state(ctx);
        Ok(())
    }

    fn update(&mut self, message: &UpdateMessage, ctx: &UIContext) -> UpdateResponse {
        self.ensure_state(ctx);
        match message {
            UpdateMessage::Accessor(update) => {
                self.on_accessor(update, ctx);
                UpdateResponse::NeedsRepaint
            }
            UpdateMessage::NodeContext(context) => {
                if let Some(state) = &mut self.state {
                    state.on_context(context.clone());
                }
                UpdateResponse::NeedsRepaint
            }
            UpdateMessage::UI(event) => {
                let commands = match &self.state {
                    Some(state) => panel_commands(event, state),
                    None => vec![],
                };
                if self.apply_commands(commands, ctx) {
                    UpdateResponse::NeedsRepaint
                } else {
                    UpdateResponse::Unchanged
                }
            }
            UpdateMessage::Directory(_)
            | UpdateMessage::AssetNotification(_)
            | UpdateMessage::Timer { .. }
            | UpdateMessage::Custom(_)
            | UpdateMessage::AssetUpdate(_) => UpdateResponse::Unchanged,
        }
    }

    fn get_value(&self) -> Option<Arc<Value>> {
        self.state
            .as_ref()
            .map(|s| Arc::new(Value::from(s.document_text())))
    }

    #[cfg(feature = "egui")]
    fn show_in_egui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &UIContext,
        app_state: &mut dyn crate::ui::app_state::AppState,
    ) -> egui::Response {
        self.ensure_state(ctx);
        crate::egui::recipes::show_recipes(self, ui, ctx, app_state)
    }

    #[cfg(feature = "webui")]
    fn render_web(&self, app_state: &dyn crate::ui::app_state::AppState) -> String {
        let dom_id = crate::ui::web::element_dom_id(self.handle);
        let body = match &self.state {
            Some(state) => crate::ui::web::recipes::recipes_html(state, &self.title),
            None => crate::ui::web::html::escape_html(&self.title),
        };
        // View and Metadata windows are children of the panel.
        let mut children = String::new();
        if let Some(handle) = self.handle {
            for child in app_state.children(handle).unwrap_or_default() {
                children.push_str(&crate::ui::web::render_element_web(child, app_state));
            }
        }
        format!("<div id=\"{dom_id}\" class=\"lq-element lq-RecipesElement\">{body}{children}</div>")
    }
}

// ---------------------------------------------------------------------------------------------
// One recipe
// ---------------------------------------------------------------------------------------------

/// A form editing one recipe through its accessor (`acc/recipe_file-…` or `acc/recipe-…`):
/// every committed edit is written as `Value::Recipe`. A rename is refused here: it must rewrite
/// the folder's references, which the recipes panel does.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeElement {
    handle: Option<UIHandle>,
    title: String,
    binding: EditorBinding,
    #[serde(skip)]
    form: Option<RecipeForm>,
    #[serde(skip)]
    registry: Option<Arc<liquers_core::command_metadata::CommandMetadataRegistry>>,
    #[serde(skip)]
    fingerprint: Option<Fingerprint>,
    #[serde(skip)]
    write_in_flight: bool,
    #[serde(skip)]
    pending: bool,
    #[serde(skip)]
    bound: bool,
    #[serde(skip)]
    live: Option<Arc<dyn ValueAccessor<Value>>>,
    #[serde(skip)]
    message: Option<String>,
}

impl RecipeElement {
    /// A form over `binding` (the accessor of a recipe).
    pub fn new(title: impl Into<String>, binding: EditorBinding) -> Self {
        RecipeElement {
            handle: None,
            title: title.into(),
            binding,
            form: None,
            registry: None,
            fingerprint: None,
            write_in_flight: false,
            pending: false,
            bound: false,
            live: None,
            message: None,
        }
    }

    /// A form over an accessor value.
    pub fn from_accessor(title: impl Into<String>, accessor: Arc<dyn ValueAccessor<Value>>) -> Self {
        let address = accessor.address().encode();
        if address.is_empty() {
            let mut element = RecipeElement::new(title, EditorBinding::Memory(String::new()));
            element.live = Some(accessor);
            element
        } else {
            RecipeElement::new(title, EditorBinding::Accessor(address))
        }
    }

    /// The form, once the recipe has been read.
    pub fn form(&self) -> Option<&RecipeForm> {
        self.form.as_ref()
    }

    /// The form, mutable (the renderer's typing state).
    pub fn form_mut(&mut self) -> Option<&mut RecipeForm> {
        self.form.as_mut()
    }

    /// The last refused edit or failed write.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref().or_else(|| self.form.as_ref().and_then(|f| f.message.as_deref()))
    }

    fn ensure_bound(&mut self, ctx: &UIContext) {
        if self.registry.is_none() {
            self.registry = ctx.command_registry().cloned();
        }
        if !self.bound {
            if let Some(handle) = self.handle {
                self.bound = true;
                let op = match self.live.take() {
                    Some(accessor) => AccessorOp::Adopt(accessor),
                    None => AccessorOp::Watch,
                };
                ctx.send_message(AppMessage::Accessor { handle, binding: self.binding.clone(), op });
            }
        }
    }

    fn write(&mut self, ctx: &UIContext) {
        let (Some(handle), Some(form)) = (self.handle, &self.form) else {
            return;
        };
        if self.write_in_flight {
            self.pending = true;
            return;
        }
        self.pending = false;
        self.write_in_flight = true;
        ctx.send_message(AppMessage::Accessor {
            handle,
            binding: self.binding.clone(),
            op: AccessorOp::SetIf { value: Value::from_recipe(form.recipe().clone()), expected: self.fingerprint },
        });
    }

    /// Applies form commands and writes the recipe when it changed.
    pub fn apply_commands(&mut self, commands: Vec<FormCommand>, ctx: &UIContext) -> bool {
        if commands.is_empty() {
            return false;
        }
        self.ensure_bound(ctx);
        let mut changed = false;
        if let Some(form) = &mut self.form {
            for command in commands {
                match form.apply(command) {
                    Ok(FormChange::Recipe(_)) => changed = true,
                    Ok(FormChange::Rename(_)) | Ok(FormChange::RenameNode { .. }) => {
                        form.message = Some(
                            "Rename the recipe in the recipes editor, which rewrites the folder's references.".to_owned(),
                        );
                    }
                    Ok(FormChange::None) | Err(_) => {}
                }
            }
        }
        if changed {
            self.message = None;
            self.write(ctx);
        }
        true
    }

    fn on_snapshot(&mut self, value: &Value, fingerprint: Option<Fingerprint>) {
        if self.write_in_flight || self.pending {
            return;
        }
        self.fingerprint = fingerprint;
        let recipe: Result<Recipe, Error> = value
            .try_into_json_value()
            .and_then(|json| serde_json::from_value(json).map_err(|e| Error::conversion_error_with_message("value", "Recipe", &e.to_string())));
        match recipe {
            Ok(recipe) => match &mut self.form {
                Some(form) => form.reload(recipe),
                None => {
                    if let Some(registry) = &self.registry {
                        let id = recipe.filename().ok().flatten().map(|f| f.name).unwrap_or_default();
                        self.form = Some(RecipeForm::new(id, recipe, registry.clone()));
                    }
                }
            },
            Err(e) => self.message = Some(e.to_string()),
        }
    }
}

#[typetag::serde]
impl UIElement for RecipeElement {
    fn type_name(&self) -> &'static str {
        "RecipeElement"
    }
    fn handle(&self) -> Option<UIHandle> {
        self.handle
    }
    fn set_handle(&mut self, handle: UIHandle) {
        self.handle = Some(handle);
    }
    fn title(&self) -> String {
        self.title.clone()
    }
    fn set_title(&mut self, title: String) {
        self.title = title;
    }
    fn clone_boxed(&self) -> Box<dyn UIElement> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn init(&mut self, handle: UIHandle, ctx: &UIContext) -> Result<(), Error> {
        self.set_handle(handle);
        self.bound = false;
        self.ensure_bound(ctx);
        Ok(())
    }

    fn update(&mut self, message: &UpdateMessage, ctx: &UIContext) -> UpdateResponse {
        self.ensure_bound(ctx);
        match message {
            UpdateMessage::Accessor(update) => {
                match update {
                    AccessorUpdate::Bound { writable, .. } => {
                        if !writable {
                            self.message = Some("The recipe cannot be written.".to_owned());
                        }
                    }
                    AccessorUpdate::Snapshot { value, fingerprint } => self.on_snapshot(value, *fingerprint),
                    AccessorUpdate::Written { fingerprint } => {
                        self.write_in_flight = false;
                        self.fingerprint = Some(*fingerprint);
                        if self.pending {
                            self.write(ctx);
                        }
                    }
                    AccessorUpdate::Conflict { value, fingerprint } => {
                        self.write_in_flight = false;
                        self.pending = false;
                        self.on_snapshot(value, *fingerprint);
                        self.message = Some(
                            "Conflict: the recipe was changed elsewhere and has been reloaded.".to_owned(),
                        );
                    }
                    AccessorUpdate::Failed(error) => {
                        self.write_in_flight = false;
                        self.message = Some(error.to_string());
                    }
                }
                UpdateResponse::NeedsRepaint
            }
            UpdateMessage::UI(event) => {
                let commands = match (&event.kind, &self.form) {
                    (UIEventKind::Named(name), Some(form)) if name.starts_with("rf:save") => {
                        form_field_changes(event, form)
                    }
                    (UIEventKind::Named(name), Some(_)) if name.starts_with("rf:options") => {
                        vec![FormCommand::ToggleOptions]
                    }
                    (UIEventKind::Named(_) | UIEventKind::Submit | UIEventKind::Change(_), _) => vec![],
                };
                if self.apply_commands(commands, ctx) {
                    UpdateResponse::NeedsRepaint
                } else {
                    UpdateResponse::Unchanged
                }
            }
            UpdateMessage::NodeContext(_)
            | UpdateMessage::Directory(_)
            | UpdateMessage::AssetNotification(_)
            | UpdateMessage::Timer { .. }
            | UpdateMessage::Custom(_)
            | UpdateMessage::AssetUpdate(_) => UpdateResponse::Unchanged,
        }
    }

    fn get_value(&self) -> Option<Arc<Value>> {
        self.form.as_ref().map(|f| Arc::new(Value::from_recipe(f.recipe().clone())))
    }

    #[cfg(feature = "egui")]
    fn show_in_egui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &UIContext,
        _app_state: &mut dyn crate::ui::app_state::AppState,
    ) -> egui::Response {
        self.ensure_bound(ctx);
        let message = self.message().map(str::to_owned);
        let Some(form) = &mut self.form else {
            return ui.label(message.unwrap_or_else(|| "Reading the recipe…".to_owned()));
        };
        let theme = ctx.theme();
        let output = crate::egui::recipes::show_form(ui, form, &theme, Some(ctx), message.as_deref(), false);
        let commands = output.commands;
        let response = output.response;
        self.apply_commands(commands, ctx);
        response
    }

    #[cfg(feature = "webui")]
    fn render_web(&self, _app_state: &dyn crate::ui::app_state::AppState) -> String {
        let dom_id = crate::ui::web::element_dom_id(self.handle);
        let body = match &self.form {
            Some(form) => crate::ui::web::recipes::form_html(form, self.message(), false),
            None => crate::ui::web::html::escape_html(self.message().unwrap_or("Reading the recipe…")),
        };
        format!("<div id=\"{dom_id}\" class=\"lq-element lq-RecipeElement\">{body}</div>")
    }
}
