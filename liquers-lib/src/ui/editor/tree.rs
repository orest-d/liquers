//! What the editor renderers draw: the [`EditorTree`] of a query, pipeline, action, resource or
//! key, built by [`super::EditorState`] from its working copy and the command registry.

use liquers_core::command_metadata::{
    ArgumentGUIInfo, ArgumentInfo, ArgumentType, CommandKey, CommandParameterValue, EnumArgument,
    VariadicShape, DEFAULT_GUI,
};
use liquers_core::error::Error;
use serde_json::Value as Json;

/// One step of a [`NodePath`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PathStep {
    /// An element of the pipeline at this level (a query's resource is element 0).
    Element(usize),
    /// An argument of the action the previous step names.
    Argument(String),
    /// An item of the variadic argument the previous step names.
    Item(usize),
}

/// Where a node lives: elements and arguments from the top of the edited query or pipeline down
/// into link arguments, which are subtrees.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct NodePath(pub Vec<PathStep>);

impl NodePath {
    /// The path of element `at` at the top level.
    pub fn element(at: usize) -> Self {
        NodePath(vec![PathStep::Element(at)])
    }
    /// This path extended by one step.
    pub fn then(&self, step: PathStep) -> Self {
        let mut steps = self.0.clone();
        steps.push(step);
        NodePath(steps)
    }
    /// The path as text, for widget ids and markup field names: `3/values/2/0`.
    pub fn id(&self) -> String {
        self.0
            .iter()
            .map(|s| match s {
                PathStep::Element(i) => i.to_string(),
                PathStep::Argument(name) => format!("a.{name}"),
                PathStep::Item(i) => format!("i.{i}"),
            })
            .collect::<Vec<_>>()
            .join("/")
    }
    /// Reads [`Self::id`] back.
    pub fn from_id(id: &str) -> Result<Self, Error> {
        if id.is_empty() {
            return Ok(NodePath::default());
        }
        id.split('/')
            .map(|part| {
                if let Some(name) = part.strip_prefix("a.") {
                    Ok(PathStep::Argument(name.to_string()))
                } else if let Some(i) = part.strip_prefix("i.") {
                    i.parse()
                        .map(PathStep::Item)
                        .map_err(|_| Error::general_error(format!("'{id}' is not a node path")))
                } else {
                    part.parse()
                        .map(PathStep::Element)
                        .map_err(|_| Error::general_error(format!("'{id}' is not a node path")))
                }
            })
            .collect::<Result<Vec<_>, _>>()
            .map(NodePath)
    }
    /// The path of the element after this one at the same level: where "Insert action after"
    /// inserts (at the end of the level, it appends). Unchanged when the path does not end with
    /// an element.
    pub fn next_sibling(&self) -> Self {
        let mut steps = self.0.clone();
        if let Some(PathStep::Element(i)) = steps.last_mut() {
            *i += 1;
        }
        NodePath(steps)
    }
    /// The last element index of the path, if it ends with one.
    pub fn last_element(&self) -> Option<usize> {
        match self.0.last() {
            Some(PathStep::Element(i)) => Some(*i),
            Some(PathStep::Argument(_)) | Some(PathStep::Item(_)) | None => None,
        }
    }
}

/// A preset entry of a presets or next menu.
#[derive(Debug, Clone, PartialEq)]
pub struct PresetEntry {
    pub label: String,
    pub description: String,
    /// The action already is this preset (AC-7): its text equals the preset's.
    pub active: bool,
}

/// The value a field shows.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    /// Not given: the field shows the default.
    Unset,
    /// A literal, as text.
    Text(String),
    /// A link: its query text and its own nodes (AC-1, AC-19). `is_key` is true for a `-R-key/…`
    /// link, which acts as a key (AC-15).
    Link { query: String, nodes: Vec<EditorNode>, is_key: bool },
    /// The items of a variadic argument (AC-2).
    Items(Vec<FieldValue>),
    /// A source the editor shows but does not edit (a nested pipeline, a placeholder).
    Other(String),
}

/// One component of an element of a shaped variadic argument: a column of a list of tuples, or
/// the key or the value of a dictionary.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentField {
    /// The column header: the declared column name, else the type's name (`key` and `value` for
    /// a dictionary).
    pub label: String,
    pub type_name: String,
    /// The widget of one cell: [`effective_gui`] of the component's type.
    pub widget: ArgumentGUIInfo,
}

/// The form of a JSON value argument ([`VariadicShape::Json`]), which its parameters decide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonForm {
    /// No parameters: `null`.
    Null,
    /// One parameter: a number or a string.
    Scalar,
    /// A leading empty parameter, then the entries.
    List,
    /// Key and value pairs.
    Dictionary,
}

impl JsonForm {
    /// The forms, in the order a chooser offers them.
    pub const ALL: [JsonForm; 4] =
        [JsonForm::Null, JsonForm::Scalar, JsonForm::List, JsonForm::Dictionary];

    /// The word a chooser shows.
    pub fn label(&self) -> &'static str {
        match self {
            JsonForm::Null => "none",
            JsonForm::Scalar => "value",
            JsonForm::List => "list",
            JsonForm::Dictionary => "dictionary",
        }
    }

    /// The chooser word back to the form.
    pub fn from_label(label: &str) -> Option<JsonForm> {
        JsonForm::ALL.into_iter().find(|form| form.label() == label)
    }

    /// The form the item texts of a JSON value argument stand for.
    pub fn of<S: AsRef<str>>(items: &[S]) -> JsonForm {
        match items {
            [] => JsonForm::Null,
            [first, ..] if first.as_ref().is_empty() => JsonForm::List,
            [_] => JsonForm::Scalar,
            _ => JsonForm::Dictionary,
        }
    }

    /// The item texts of the same data in this form, for [`super::EditorCommand::SetItems`]:
    /// the values are kept, a list gets its leading empty item, a dictionary made from values
    /// gets the keys `key1`, `key2`, ….
    pub fn convert<S: AsRef<str>>(&self, items: &[S]) -> Vec<String> {
        let from = JsonForm::of(items);
        let values: Vec<String> = match from {
            JsonForm::Null => Vec::new(),
            JsonForm::Scalar => items.iter().map(|s| s.as_ref().to_string()).collect(),
            JsonForm::List => items.iter().skip(1).map(|s| s.as_ref().to_string()).collect(),
            JsonForm::Dictionary => {
                items.iter().skip(1).step_by(2).map(|s| s.as_ref().to_string()).collect()
            }
        };
        match self {
            JsonForm::Null => Vec::new(),
            // One empty item alone would be the empty list, so a value is never empty.
            JsonForm::Scalar => vec![values
                .into_iter()
                .find(|v| !v.is_empty())
                .unwrap_or_else(|| "0".to_string())],
            JsonForm::List => std::iter::once(String::new()).chain(values).collect(),
            JsonForm::Dictionary if from == JsonForm::Dictionary => {
                items.iter().map(|s| s.as_ref().to_string()).collect()
            }
            JsonForm::Dictionary => {
                let mut values = values;
                if values.is_empty() {
                    values.push(String::new());
                }
                values
                    .into_iter()
                    .enumerate()
                    .flat_map(|(k, value)| [format!("key{}", k + 1), value])
                    .collect()
            }
        }
    }
}

/// The type of item `at` of a variadic argument: its component's for tuples, text for a
/// dictionary key, the argument's own otherwise.
pub fn item_type(argument: &ArgumentInfo, at: usize) -> ArgumentType {
    match &argument.shape {
        VariadicShape::Tuples(components) if !components.is_empty() => {
            components[at % components.len()].clone()
        }
        VariadicShape::Map if at % 2 == 0 => ArgumentType::String,
        VariadicShape::Json => ArgumentType::Any,
        VariadicShape::List | VariadicShape::Tuples(_) | VariadicShape::Map => {
            argument.argument_type.clone()
        }
    }
}

/// The components of one element of a shaped variadic argument; empty for a plain list and for
/// a JSON value, whose form decides them.
pub fn shape_components(argument: &ArgumentInfo) -> Vec<ComponentField> {
    let component = |label: String, argument_type: &ArgumentType| {
        let mut one = argument.clone();
        one.argument_type = argument_type.clone();
        ComponentField {
            label,
            type_name: crate::utils::argument_type_name(argument_type),
            widget: effective_gui(&one),
        }
    };
    match &argument.shape {
        VariadicShape::Tuples(components) => components
            .iter()
            .enumerate()
            .map(|(c, argument_type)| {
                let label = match argument.columns.get(c) {
                    Some(name) if !name.is_empty() => name.clone(),
                    Some(_) | None => crate::utils::argument_type_name(argument_type),
                };
                component(label, argument_type)
            })
            .collect(),
        VariadicShape::Map => vec![
            component("key".to_string(), &ArgumentType::String),
            component("value".to_string(), &argument.argument_type),
        ],
        VariadicShape::List | VariadicShape::Json => Vec::new(),
    }
}

/// One argument of an action, with everything its widget and its hover text need.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorField {
    pub name: String,
    pub label: String,
    pub description: String,
    pub type_name: String,
    pub default_text: Option<String>,
    /// The widget to draw: [`effective_gui`].
    pub widget: ArgumentGUIInfo,
    /// The alternatives of an enum argument, by alias, with their descriptive labels.
    pub alternatives: Vec<String>,
    pub others_allowed: bool,
    pub multiple: bool,
    /// How the items of a variadic argument group into its value: a plain list, rows of tuples,
    /// the pairs of a dictionary, or a JSON value. The items stay one flat list
    /// ([`FieldValue::Items`]); row `r`, component `c` is item `r * components.len() + c`.
    pub shape: VariadicShape,
    /// One entry per component of an element: the columns of the tuples, or key and value.
    pub components: Vec<ComponentField>,
    pub value: FieldValue,
    pub presets: Vec<PresetEntry>,
    /// The field's index among the action's numeric arguments (AC-13).
    pub numeric_index: Option<usize>,
    /// The extension of a Node argument (`glb`); `None` for any other argument.
    pub node_extension: Option<String>,
    /// The nodes a Node argument can name (filename and title), when the editor knows the
    /// folder's recipes ([`NodeContext`]); empty otherwise, and the field is a text box
    /// (design `recipe-tree`, AC-18).
    pub node_choices: Vec<liquers_core::recipe_tree::NodeChoice>,
}

/// The folder an editor's query belongs to, for its Node arguments (design `recipe-tree`,
/// AC-18, AC-25): the folder's recipes and, when known, its listing (the asset manager's:
/// recipes and stored files together). The runner delivers it (`UpdateMessage::NodeContext`)
/// to an editor bound to a recipe of a `recipes.yaml`.
#[derive(Debug, Clone)]
pub struct NodeContext {
    /// The folder, as a key.
    pub folder: String,
    /// The folder's `recipes.yaml`.
    pub recipes: liquers_core::recipes::RecipeList,
    /// The folder's listing, when known.
    pub listing: Option<Vec<String>>,
    /// The asset manager's information of each file of the folder, by filename: status,
    /// progress, icon and type (design `recipes-editor`, AC-2, AC-17). Empty when unknown.
    pub assets: std::collections::BTreeMap<String, liquers_core::metadata::AssetInfo>,
    /// The data formats each file's type can be written in, by filename (AC-19 Export).
    pub formats: std::collections::BTreeMap<String, Vec<String>>,
    /// The outcome of the last Save as, for the recipes editor to show.
    pub notice: Option<String>,
}

impl NodeContext {
    /// A context with the recipes and listing only.
    pub fn new(
        folder: impl Into<String>,
        recipes: liquers_core::recipes::RecipeList,
        listing: Option<Vec<String>>,
    ) -> Self {
        NodeContext {
            folder: folder.into(),
            recipes,
            listing,
            assets: Default::default(),
            formats: Default::default(),
            notice: None,
        }
    }
}

impl EditorField {
    /// The items of a variadic argument as text; a link or another source that is not a literal
    /// is `None`. Empty when the argument is not given.
    pub fn item_texts(&self) -> Vec<Option<String>> {
        match &self.value {
            FieldValue::Items(items) => items
                .iter()
                .map(|item| match item {
                    FieldValue::Text(text) => Some(text.clone()),
                    FieldValue::Unset
                    | FieldValue::Link { .. }
                    | FieldValue::Items(_)
                    | FieldValue::Other(_) => None,
                })
                .collect(),
            FieldValue::Unset | FieldValue::Text(_) | FieldValue::Link { .. } | FieldValue::Other(_) => {
                Vec::new()
            }
        }
    }

    /// The form of a JSON value argument, read from its items.
    pub fn json_form(&self) -> JsonForm {
        let texts: Vec<String> =
            self.item_texts().into_iter().map(|t| t.unwrap_or_else(|| "?".to_string())).collect();
        JsonForm::of(&texts)
    }

    /// The hover text (AC-3): label, description, type and default.
    pub fn hover(&self) -> String {
        let mut out = format!("{} ({})", self.label, self.type_name);
        if !self.description.is_empty() {
            out.push_str("\n");
            out.push_str(&self.description);
        }
        if let Some(d) = &self.default_text {
            out.push_str(&format!("\nDefault: {d}"));
        }
        out
    }
}

/// The resource headers a resource node offers, in the order of the dropdown (AC-7): the empty
/// header is the asset itself.
pub const RESOURCE_HEADERS: &[&str] = &[
    "",
    "key",
    "dir",
    "recipe",
    "cwd",
    "binary",
    "metadata",
    "sdir",
    "stored_binary",
    "stored_metadata",
];

/// One node of the tree.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorNode {
    /// A resource: its header (`None` for a key editor, which has no header) and its key.
    Resource { path: NodePath, header: Option<String>, key: String, note: Option<String> },
    /// The `ns` list of a run of actions (AC-16, AC-25).
    Namespaces { path: NodePath, list: Vec<String>, unused: Vec<String> },
    /// An action resolved by the registry.
    Action {
        path: NodePath,
        command: CommandKey,
        label: String,
        doc: String,
        presets: Vec<PresetEntry>,
        next: Vec<PresetEntry>,
        filename: Option<String>,
        fields: Vec<EditorField>,
        excess: Vec<String>,
        /// The command's `hint icon` (AC-8), `None` when absent or malformed.
        hint_icon: Option<crate::ui::theme::Icon>,
    },
    /// A `q` (quote) or `v` (volatile) directive.
    Directive { path: NodePath, name: String },
    /// A literal value element.
    Value { path: NodePath, json: String },
    /// A subtree the editor shows read-only (a nested pipeline).
    Nested { path: NodePath, label: String, nodes: Vec<EditorNode> },
    /// An element the editor shows as text, with the reason it is not edited (AC-23).
    Opaque { path: NodePath, text: String, reason: String },
}

impl EditorNode {
    /// The node's path.
    pub fn path(&self) -> &NodePath {
        match self {
            EditorNode::Resource { path, .. }
            | EditorNode::Namespaces { path, .. }
            | EditorNode::Action { path, .. }
            | EditorNode::Directive { path, .. }
            | EditorNode::Value { path, .. }
            | EditorNode::Nested { path, .. }
            | EditorNode::Opaque { path, .. } => path,
        }
    }
    /// The node's fields, when it is an action.
    pub fn fields(&self) -> &[EditorField] {
        match self {
            EditorNode::Action { fields, .. } => fields,
            EditorNode::Resource { .. }
            | EditorNode::Namespaces { .. }
            | EditorNode::Directive { .. }
            | EditorNode::Value { .. }
            | EditorNode::Nested { .. }
            | EditorNode::Opaque { .. } => &[],
        }
    }
}

/// A command of the searchable command tree (AC-17).
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogCommand {
    pub key: CommandKey,
    pub label: String,
    pub doc: String,
}

/// The commands of one namespace.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogNamespace {
    pub namespace: String,
    pub commands: Vec<CatalogCommand>,
}

/// The command picker, while it is open.
#[derive(Debug, Clone, PartialEq)]
pub struct Picker {
    /// Where the picked command goes: before this element, or appended when `None`.
    pub insert_at: Option<NodePath>,
    pub filter: String,
    pub catalog: Vec<CatalogNamespace>,
}

/// One entry of a directory listing.
#[derive(Debug, Clone, PartialEq)]
pub struct DirectoryEntry {
    pub name: String,
    pub is_dir: bool,
}

/// A directory listing for the file selector (AC-14), as the runner delivers it.
#[derive(Debug, Clone, PartialEq)]
pub struct DirectoryListing {
    /// The folder listed, as a key.
    pub folder: String,
    pub entries: Vec<DirectoryEntry>,
    pub error: Option<String>,
}

/// The file selector, while it is open.
#[derive(Debug, Clone, PartialEq)]
pub struct Browser {
    /// The resource node whose key it sets.
    pub path: NodePath,
    pub listing: Option<DirectoryListing>,
}

/// A namespace edit waiting for the user's confirmation (AC-25).
#[derive(Debug, Clone, PartialEq)]
pub struct Confirmation {
    pub path: NodePath,
    pub namespaces: Vec<String>,
    /// One line per action that would resolve to another command.
    pub changes: Vec<String>,
}

/// What the renderers draw.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EditorTree {
    /// The query text (or, when `text_editable` is false, the reason it cannot be edited is in
    /// `text_note`).
    pub text: String,
    pub text_editable: bool,
    /// The host draws the query text itself (the query console), so the editor does not.
    /// `Ctrl+J` from a field then reports the range in [`EditorTree::text`] to select instead.
    pub hide_text: bool,
    /// A parse error of the typed text, or why the text is read-only (AC-4, AC-10).
    pub text_note: Option<String>,
    /// The tree no longer matches the typed text (AC-4).
    pub stale: bool,
    pub nodes: Vec<EditorNode>,
    /// Whether the tree offers the filename field (a query or pipeline editor).
    pub has_filename: bool,
    pub filename: String,
    /// The last command's declared filename; `None` hides the Default button (AC-22).
    pub filename_default: Option<String>,
    /// Edits not yet set into the accessor (AC-26).
    pub pending: bool,
    /// What a committed edit does (design `query-update-modes`).
    pub update_mode: super::UpdateMode,
    /// The modes the mode control offers (all three in an editor, two in the console).
    pub update_modes: Vec<super::UpdateMode>,
    /// An edit can be undone, or redone (AC-10).
    pub can_undo: bool,
    pub can_redo: bool,
    /// The host (the query console) draws the update mode, ✓ and undo controls, so the editor
    /// draws none of its own (AC-13).
    pub host_controls: bool,
    /// The editor cannot write (no accessor, a read-only binding, or the binding failed).
    pub read_only: bool,
    /// A refusal, a conflict or another message for the user, with the path it concerns.
    pub message: Option<(NodePath, String)>,
    pub picker: Option<Picker>,
    pub browser: Option<Browser>,
    pub confirmation: Option<Confirmation>,
    /// Source ranges of the text's elements and arguments (AC-10); empty while `stale`.
    pub spans: Vec<TextSpan>,
    /// The registry has more than one realm, so card headers show the realm (AC-4).
    pub show_realm: bool,
}

/// What a [`TextSpan`] covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpanTarget {
    /// The whole element at the span's path.
    Element,
    /// The `ns` instruction before the action at the span's path.
    Namespaces,
    /// One argument of the action at the span's path; `item` for an item of a variadic argument.
    Argument { arg: String, item: Option<usize> },
}

/// A byte range of [`EditorTree::text`] and the tree node (or argument) it is the text of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSpan {
    pub path: NodePath,
    pub target: SpanTarget,
    pub range: std::ops::Range<usize>,
}

impl EditorTree {
    /// The innermost span holding byte `offset` (a cursor at the end of a span is still in it).
    pub fn locate(&self, offset: usize) -> Option<&TextSpan> {
        self.spans
            .iter()
            .filter(|s| s.range.start <= offset && offset <= s.range.end)
            .min_by_key(|s| (s.range.end - s.range.start, std::cmp::Reverse(s.path.0.len())))
    }

    /// The range of `target` at `path`, when the text has one.
    pub fn span_of(&self, path: &NodePath, target: &SpanTarget) -> Option<std::ops::Range<usize>> {
        self.spans.iter().find(|s| &s.path == path && &s.target == target).map(|s| s.range.clone())
    }

    /// The index of the last action node of the top level.
    pub fn last_action(&self) -> Option<usize> {
        self.nodes.iter().rposition(|n| matches!(n, EditorNode::Action { .. }))
    }
}

/// The widget an argument is edited with (AC-2): its declared `gui_info`, unless that is the
/// default text field or `None`, in which case the argument type picks it.
pub fn effective_gui(argument: &ArgumentInfo) -> ArgumentGUIInfo {
    let declared = match &argument.gui_info {
        ArgumentGUIInfo::None => None,
        g if *g == DEFAULT_GUI => None,
        g => Some(g.clone()),
    };
    if let Some(g) = declared {
        return g;
    }
    match &argument.argument_type {
        ArgumentType::Boolean | ArgumentType::BooleanOption => ArgumentGUIInfo::Checkbox,
        ArgumentType::Integer | ArgumentType::IntegerOption => ArgumentGUIInfo::IntegerField,
        ArgumentType::Float | ArgumentType::FloatOption => ArgumentGUIInfo::FloatField,
        ArgumentType::Enum(_) | ArgumentType::GlobalEnum(_) => ArgumentGUIInfo::EnumSelector,
        ArgumentType::Node(_) => ArgumentGUIInfo::NodeSelector,
        ArgumentType::String | ArgumentType::Any | ArgumentType::None => DEFAULT_GUI.clone(),
    }
}

/// Whether an argument takes a number (AC-13).
pub fn is_numeric(argument: &ArgumentInfo) -> bool {
    match argument.argument_type {
        ArgumentType::Integer
        | ArgumentType::IntegerOption
        | ArgumentType::Float
        | ArgumentType::FloatOption => true,
        ArgumentType::String
        | ArgumentType::Boolean
        | ArgumentType::BooleanOption
        | ArgumentType::Enum(_)
        | ArgumentType::GlobalEnum(_)
        | ArgumentType::Any
        | ArgumentType::None
        | ArgumentType::Node(_) => false,
    }
}

/// Scales a controller value `value` in `0..=max` into an argument (AC-13): an integer argument
/// with a range or slider takes the same fraction of its range, one without takes the value as
/// is; a float slider takes the fraction of its range, a float without one the fraction itself.
pub fn scale_numeric(argument: &ArgumentInfo, value: f64, max: f64) -> Result<Json, Error> {
    if !is_numeric(argument) {
        return Err(Error::invalid_argument(
            "controller",
            &argument.name,
            "is not a numeric argument",
        ));
    }
    let fraction = if max > 0.0 { (value / max).clamp(0.0, 1.0) } else { 0.0 };
    let integer = matches!(
        argument.argument_type,
        ArgumentType::Integer | ArgumentType::IntegerOption
    );
    let json = match (effective_gui(argument), integer) {
        (ArgumentGUIInfo::IntegerRange { min, max }, _)
        | (ArgumentGUIInfo::IntegerSlider { min, max, .. }, _) => {
            Json::from(min + ((max - min) as f64 * fraction).round() as i64)
        }
        (ArgumentGUIInfo::FloatSlider { min, max, .. }, true) => {
            Json::from((min + (max - min) * fraction).round() as i64)
        }
        (ArgumentGUIInfo::FloatSlider { min, max, .. }, false) => {
            float_json(min + (max - min) * fraction)?
        }
        (_, true) => Json::from(value.round() as i64),
        (_, false) => float_json(fraction)?,
    };
    Ok(json)
}

fn float_json(x: f64) -> Result<Json, Error> {
    serde_json::Number::from_f64(x)
        .map(Json::Number)
        .ok_or_else(|| Error::general_error(format!("{x} is not a finite number")))
}

/// The aliases of an enum argument's alternatives.
pub fn enum_alternatives(e: &EnumArgument) -> Vec<String> {
    e.values.iter().map(|a| a.alias.clone()).collect()
}

/// The text of a default value, as the hover shows it.
pub fn default_text(default: &CommandParameterValue) -> Option<String> {
    match default {
        CommandParameterValue::Value(v) => Some(json_text(v)),
        CommandParameterValue::Query(q) => Some(format!("link {}", q.encode())),
        CommandParameterValue::Pipeline(_) => Some("a pipeline".to_string()),
        CommandParameterValue::None => None,
    }
}

/// A literal as the user types it: a string as is, anything else as JSON.
pub fn json_text(v: &Json) -> String {
    match v {
        Json::String(s) => s.clone(),
        Json::Null | Json::Bool(_) | Json::Number(_) | Json::Array(_) | Json::Object(_) => {
            v.to_string()
        }
    }
}
