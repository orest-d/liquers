//! The `lui` commands: build and arrange UI elements from queries.
//!
//! Each command is an ordinary Liquers command in namespace `lui`, so a query such as
//! `-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query/ns-lui/query_editor` creates an
//! element, and `…/ns-lui/add-instead` places one in the app state.
//!
//! # Main components
//!
//! - Tree commands: `add`, `remove`, `children`, `first`, `last`, `parent`, … (positions and
//!   navigation words of `ui::resolve`).
//! - Element constructors: [`ui_spec`], [`markdown`], [`query_console`], and the editors
//!   [`query_editor`], [`action_editor`], [`resource_editor`], [`key_editor`] (an accessor value is
//!   bound by its address, any other value is kept in memory).
//! - [`submit`]: submit the state as the current element's query.
//! - [`register_lui_commands!`](crate::register_lui_commands): registers them all.
//!
//! # Where it is used
//!
//! - Every UI host registers them (`ui::native`, the browser mount, the examples); `UISpec` menus
//!   and shortcuts run them through `ui::action`.
//!
//! # Related concepts
//!
//! - `ui::app_state` (the element tree), `ui::payload` (how a command reaches it).
//! - `specs/guides/COMMAND_REGISTRATION_GUIDE.md`: how the commands are declared.
//!
//! # Examples
//!
//! ```
//! use liquers_core::state::State;
//! use liquers_lib::ui::UIElement;
//! use liquers_lib::value::{ExtValueInterface, Value};
//!
//! let value = liquers_lib::ui::commands::query_editor(&State::<Value>::new(), "on_demand".into())?;
//! assert_eq!(value.as_ui_element()?.type_name(), "EditorElement");
//! # Ok::<(), liquers_core::error::Error>(())
//! ```

use std::sync::Arc;

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_core::value::ValueInterface;

use crate::value::Value;

use super::app_state::AppState;
use super::editor::{EditorBinding, EditorElement, EditorKind, UpdateMode};
use super::handle::UIHandle;
use super::message::AppMessage;
use super::payload::UIPayload;
use super::resolve::{resolve_navigation, resolve_position, InsertionPoint};
use super::widgets::markdown_element::MarkdownElement;
use super::widgets::query_console_element::QueryConsoleElement;
use super::widgets::ui_spec_element::{UISpec, UISpecElement};

// ─── Helper ─────────────────────────────────────────────────────────────────

fn get_app_state_and_current<E: Environment>(
    context: &Context<E>,
) -> Result<(Arc<tokio::sync::Mutex<dyn AppState>>, Option<UIHandle>), Error>
where
    E::Payload: UIPayload,
{
    let payload = context
        .get_payload_clone()
        .ok_or_else(|| Error::general_error("No UI payload available".to_string()))?;
    Ok((payload.app_state(), payload.handle()))
}

fn handles_to_value(handles: &[UIHandle]) -> Result<Value, Error> {
    let mut ids = Vec::with_capacity(handles.len());
    for h in handles {
        let id = i64::try_from(h.0).map_err(|_| {
            Error::conversion_error_with_message(h.0, "i64", "UI handle does not fit into i64")
        })?;
        ids.push(id);
    }
    Ok(Value::from(ids))
}

// ─── Commands ───────────────────────────────────────────────────────────────

/// Add a new element to the UI tree.
///
/// 2-arg model: add-<position_word>-<reference_word>
/// - position_word: before, after, instead, first, last, child
/// - reference_word: navigation word for the reference element (default: "current")
///
/// The input state is inserted via `AppState::insert_state`, which handles:
/// - UIElement extraction (if the value is `ExtValue::UIElement`)
/// - StateViewElement wrapping (for plain values)
/// - Source preservation from metadata
#[liquers_macro::command_version]
pub async fn add<E: Environment<Value = Value>>(
    state: State<Value>,
    position_word: String,
    reference_word: String,
    context: Context<E>,
) -> Result<Value, Error>
where
    E::Payload: UIPayload,
{
    let (app_state_arc, current) = get_app_state_and_current(&context)?;
    let mut app_state = app_state_arc.lock().await;

    // When reference_word is "current" and there is no current element,
    // allow root creation for child/last/first position words.
    let insertion = if reference_word == "current" && current.is_none() {
        match position_word.as_str() {
            "child" | "last" | "first" => InsertionPoint::Root,
            other => {
                return Err(Error::general_error(format!(
                    "Cannot use position '{}' without a current element (no root exists)",
                    other
                )));
            }
        }
    } else {
        let reference = resolve_navigation(&*app_state, &reference_word, current)?;
        resolve_position(&position_word, reference)?
    };

    // Delegate to AppState's insert_state which handles UIElement detection,
    // StateViewElement wrapping, and source preservation from metadata
    let handle = app_state.insert_state(&insertion, &state)?;

    // Call init() on the newly inserted element (if present).
    // insert_state may create pending nodes (element=None) for Query sources,
    // so only init if an element was actually set.
    if let Ok(mut elem) = app_state.take_element(handle) {
        let payload = context
            .get_payload_clone()
            .ok_or_else(|| Error::general_error("No UI payload available".to_string()))?;
        let ui_ctx = payload.ui_context().clone().with_handle(Some(handle));
        let _ = elem.init(handle, &ui_ctx);
        let _ = app_state.put_element(handle, elem);
    }

    Ok(Value::from(format!("{}", handle.0)))
}

/// Remove an element from the UI tree.
#[liquers_macro::command_version]
pub async fn remove<E: Environment<Value = Value>>(
    _state: State<Value>,
    target_word: String,
    context: Context<E>,
) -> Result<Value, Error>
where
    E::Payload: UIPayload,
{
    let (app_state_arc, current) = get_app_state_and_current(&context)?;
    let mut app_state = app_state_arc.lock().await;

    let target = resolve_navigation(&*app_state, &target_word, current)?;
    app_state.remove(target)?;
    Ok(Value::none())
}

/// Get children handles of the target element.
#[liquers_macro::command_version]
pub async fn children<E: Environment<Value = Value>>(
    _state: State<Value>,
    target_word: String,
    context: Context<E>,
) -> Result<Value, Error>
where
    E::Payload: UIPayload,
{
    let (app_state_arc, current) = get_app_state_and_current(&context)?;
    let app_state = app_state_arc.lock().await;

    let target = resolve_navigation(&*app_state, &target_word, current)?;
    let child_handles = app_state.children(target)?;
    handles_to_value(&child_handles)
}

/// Navigate to the first child of the target element.
#[liquers_macro::command_version]
pub async fn first<E: Environment<Value = Value>>(
    _state: State<Value>,
    target_word: String,
    context: Context<E>,
) -> Result<Value, Error>
where
    E::Payload: UIPayload,
{
    let (app_state_arc, current) = get_app_state_and_current(&context)?;
    let app_state = app_state_arc.lock().await;

    let target = resolve_navigation(&*app_state, &target_word, current)?;
    let child = app_state
        .first_child(target)?
        .ok_or_else(|| Error::general_error("No children".to_string()))?;
    Ok(Value::from(format!("{}", child.0)))
}

/// Navigate to the last child of the target element.
#[liquers_macro::command_version]
pub async fn last<E: Environment<Value = Value>>(
    _state: State<Value>,
    target_word: String,
    context: Context<E>,
) -> Result<Value, Error>
where
    E::Payload: UIPayload,
{
    let (app_state_arc, current) = get_app_state_and_current(&context)?;
    let app_state = app_state_arc.lock().await;

    let target = resolve_navigation(&*app_state, &target_word, current)?;
    let child = app_state
        .last_child(target)?
        .ok_or_else(|| Error::general_error("No children".to_string()))?;
    Ok(Value::from(format!("{}", child.0)))
}

/// Navigate to the parent of the target element.
#[liquers_macro::command_version]
pub async fn parent<E: Environment<Value = Value>>(
    _state: State<Value>,
    target_word: String,
    context: Context<E>,
) -> Result<Value, Error>
where
    E::Payload: UIPayload,
{
    let (app_state_arc, current) = get_app_state_and_current(&context)?;
    let app_state = app_state_arc.lock().await;

    let target = resolve_navigation(&*app_state, &target_word, current)?;
    let p = app_state
        .parent(target)?
        .ok_or_else(|| Error::general_error("No parent (root element)".to_string()))?;
    Ok(Value::from(format!("{}", p.0)))
}

/// Navigate to the next sibling of the target element.
#[liquers_macro::command_version]
pub async fn next<E: Environment<Value = Value>>(
    _state: State<Value>,
    target_word: String,
    context: Context<E>,
) -> Result<Value, Error>
where
    E::Payload: UIPayload,
{
    let (app_state_arc, current) = get_app_state_and_current(&context)?;
    let app_state = app_state_arc.lock().await;

    let target = resolve_navigation(&*app_state, &target_word, current)?;
    let sibling = app_state
        .next_sibling(target)?
        .ok_or_else(|| Error::general_error("No next sibling".to_string()))?;
    Ok(Value::from(format!("{}", sibling.0)))
}

/// Navigate to the previous sibling of the target element.
#[liquers_macro::command_version]
pub async fn prev<E: Environment<Value = Value>>(
    _state: State<Value>,
    target_word: String,
    context: Context<E>,
) -> Result<Value, Error>
where
    E::Payload: UIPayload,
{
    let (app_state_arc, current) = get_app_state_and_current(&context)?;
    let app_state = app_state_arc.lock().await;

    let target = resolve_navigation(&*app_state, &target_word, current)?;
    let sibling = app_state
        .previous_sibling(target)?
        .ok_or_else(|| Error::general_error("No previous sibling".to_string()))?;
    Ok(Value::from(format!("{}", sibling.0)))
}

/// Get all root element handles.
#[liquers_macro::command_version]
pub async fn roots<E: Environment<Value = Value>>(
    _state: State<Value>,
    context: Context<E>,
) -> Result<Value, Error>
where
    E::Payload: UIPayload,
{
    let (app_state_arc, _) = get_app_state_and_current(&context)?;
    let app_state = app_state_arc.lock().await;

    let root_handles = app_state.roots();
    handles_to_value(&root_handles)
}

/// Set the active (focused) element.
#[liquers_macro::command_version]
pub async fn activate<E: Environment<Value = Value>>(
    _state: State<Value>,
    target_word: String,
    context: Context<E>,
) -> Result<Value, Error>
where
    E::Payload: UIPayload,
{
    let (app_state_arc, current) = get_app_state_and_current(&context)?;
    let mut app_state = app_state_arc.lock().await;

    let target = resolve_navigation(&*app_state, &target_word, current)?;
    app_state.set_active_handle(Some(target));
    Ok(Value::from(format!("{}", target.0)))
}

/// Command: lui/ui_spec
/// Creates UISpecElement from YAML spec in state
#[liquers_macro::command_version]
pub fn ui_spec<E: Environment<Value = Value>>(
    state: &State<Value>,
    context: Context<E>,
) -> Result<Value, Error> {
    // 1. Extract YAML from state (text or bytes)
    let yaml_str = state.try_into_string().or_else(|_| {
        // Try bytes → UTF-8
        state.as_bytes().and_then(|bytes| {
            String::from_utf8(bytes.to_vec())
                .map_err(|e| Error::general_error(format!("Invalid UTF-8: {}", e)))
        })
    })?;

    // 2. Parse YAML → UISpec
    let spec = UISpec::from_yaml(&yaml_str)?;

    // 3. Validate shortcuts, warn via context if conflicts
    if let Some(menu_spec) = &spec.menu {
        let conflicts = menu_spec.validate_shortcuts();
        for (shortcut, count) in conflicts {
            context.warning(&format!(
                "Keyboard shortcut '{}' defined {} times (will use first occurrence)",
                shortcut, count
            ))?;
        }
    }

    // 4. Create element
    let element = UISpecElement::from_spec("UI Spec".to_string(), spec);

    // 5. Wrap in ExtValue::UIElement
    Ok(Value::from(crate::value::ExtValue::UIElement {
        value: Arc::new(element),
    }))
}

/// Command: lui/markdown
/// Creates a MarkdownElement from the input state (markdown text string).
#[liquers_macro::command_version]
pub fn markdown(state: &State<Value>) -> Result<Value, Error> {
    let text = if state.is_none() {
        String::new()
    } else {
        state.try_into_string()?
    };
    let element = MarkdownElement::new("Markdown".to_string(), text);
    Ok(Value::from(crate::value::ExtValue::UIElement {
        value: Arc::new(element),
    }))
}

/// Command: lui/query_console
/// Creates a QueryConsoleElement from the input state (query text string).
#[liquers_macro::command_version]
pub fn query_console(state: &State<Value>, mode: String) -> Result<Value, Error> {
    let mode = UpdateMode::from_name(&mode)?;
    let query_string = if state.is_none() {
        String::new()
    } else if let Ok(q) = state.data_unchecked().try_into_query() {
        q.encode()
    } else {
        state.try_into_string()?
    };
    let mut element = QueryConsoleElement::new("Query Console".to_string(), query_string);
    element.set_update_mode(mode);
    Ok(Value::from(crate::value::ExtValue::UIElement {
        value: Arc::new(element),
    }))
}

/// The editor element of `kind` over the state: an accessor value is bound by its address, any
/// other value (a query, a pipeline, a key or text) is kept in memory for the session.
fn editor_element(state: &State<Value>, kind: EditorKind, title: &str, mode: &str) -> Result<Value, Error> {
    let mode = UpdateMode::from_name(mode)?;
    let element = if state.is_none() {
        EditorElement::new(title, kind, EditorBinding::Memory(String::new()))
    } else {
        let value = state.data_unchecked();
        match value.try_into_accessor() {
            Ok(accessor) => EditorElement::from_accessor(title, kind, accessor),
            Err(_) => {
                let text = match kind {
                    EditorKind::Key => match value.try_into_key() {
                        Ok(key) => key.encode(),
                        Err(_) => value.try_into_string()?,
                    },
                    EditorKind::Query | EditorKind::Action | EditorKind::Resource => {
                        match value.try_into_query() {
                            Ok(q) => q.encode(),
                            Err(_) => match value.try_into_pipeline() {
                                Ok(p) => p.to_json().to_string(),
                                Err(_) => value.try_into_string()?,
                            },
                        }
                    }
                };
                EditorElement::new(title, kind, EditorBinding::Memory(text))
            }
        }
    };
    let element = element.with_update_mode(mode);
    Ok(Value::from(crate::value::ExtValue::UIElement {
        value: Arc::new(element),
    }))
}

/// Command: lui/query_editor
/// An editor of a query or pipeline: a tree of its steps beside the highlighted text.
#[liquers_macro::command_version]
pub fn query_editor(state: &State<Value>, mode: String) -> Result<Value, Error> {
    editor_element(state, EditorKind::Query, "Query editor", &mode)
}

/// Command: lui/action_editor
/// An editor of one action's arguments (the one-element pipeline of an action accessor).
#[liquers_macro::command_version]
pub fn action_editor(state: &State<Value>, mode: String) -> Result<Value, Error> {
    editor_element(state, EditorKind::Action, "Action editor", &mode)
}

/// Command: lui/resource_editor
/// An editor of a query's resource: its header and its key, with a store file selector.
#[liquers_macro::command_version]
pub fn resource_editor(state: &State<Value>, mode: String) -> Result<Value, Error> {
    editor_element(state, EditorKind::Resource, "Resource editor", &mode)
}

/// Command: lui/key_editor
/// An editor of a key, with a store file selector.
#[liquers_macro::command_version]
pub fn key_editor(state: &State<Value>, mode: String) -> Result<Value, Error> {
    editor_element(state, EditorKind::Key, "Key editor", &mode)
}

/// Command: lui/submit
/// Interpret the input state as a query string and submit it bound to the current handle.
/// This is what makes `UiAction::Apply { query: "ns-lui/submit" }` behave like the old
/// SubmitInput: the entered text becomes the query the console evaluates.
#[liquers_macro::command_version]
pub fn submit<E: Environment<Value = Value>>(
    state: &State<Value>,
    context: Context<E>,
) -> Result<Value, Error>
where
    E::Payload: UIPayload,
{
    let query = state.try_into_string()?;
    let payload = context
        .get_payload_clone()
        .ok_or_else(|| Error::general_error("No UI payload available".to_string()))?;
    let handle = payload
        .handle()
        .ok_or_else(|| Error::general_error("No current element for submit".to_string()))?;
    payload
        .ui_context()
        .send_message(AppMessage::RequestAssetUpdates { handle, query });
    Ok(Value::none())
}

// ─── Registration ───────────────────────────────────────────────────────────

/// Register lui namespace commands.
///
/// The caller must define `type CommandEnvironment = ...` with a concrete
/// environment type whose Payload implements UIPayload before calling this.
///
/// Example:
/// ```ignore
/// use liquers_core::context::SimpleEnvironmentWithPayload;
/// use liquers_lib::value::Value;
/// use liquers_lib::ui::payload::SimpleUIPayload;
///
/// type CommandEnvironment = SimpleEnvironmentWithPayload<Value, SimpleUIPayload>;
/// let cr = env.get_mut_command_registry();
/// liquers_lib::ui::commands::register_lui_commands!(cr);
/// ```
#[macro_export]
macro_rules! register_lui_commands {
    ($cr:expr) => {{
        use liquers_macro::register_command;
        use $crate::ui::commands::*;

        register_command!($cr,
            async fn add(state, position_word: String, reference_word: String = "current", context) -> result
            namespace: "lui"
            label: "Add element"
            doc: "Add a new element to the UI tree"

            payload: required
        version: auto
        )?;
        register_command!($cr,
            async fn remove(state, target_word: String, context) -> result
            namespace: "lui"
            label: "Remove element"
            doc: "Remove an element from the UI tree"

            payload: required
        version: auto
        )?;
        register_command!($cr,
            async fn children(state, target_word: String, context) -> result
            namespace: "lui"
            label: "Children"
            doc: "Get children handles of target element"

            payload: required
        version: auto
        )?;
        register_command!($cr,
            async fn first(state, target_word: String, context) -> result
            namespace: "lui"
            label: "First child"
            doc: "Navigate to first child of target"

            payload: required
        version: auto
        )?;
        register_command!($cr,
            async fn last(state, target_word: String, context) -> result
            namespace: "lui"
            label: "Last child"
            doc: "Navigate to last child of target"

            payload: required
        version: auto
        )?;
        register_command!($cr,
            async fn parent(state, target_word: String, context) -> result
            namespace: "lui"
            label: "Parent"
            doc: "Navigate to parent of target"

            payload: required
        version: auto
        )?;
        register_command!($cr,
            async fn next(state, target_word: String, context) -> result
            namespace: "lui"
            label: "Next sibling"
            doc: "Navigate to next sibling of target"

            payload: required
        version: auto
        )?;
        register_command!($cr,
            async fn prev(state, target_word: String, context) -> result
            namespace: "lui"
            label: "Previous sibling"
            doc: "Navigate to previous sibling of target"

            payload: required
        version: auto
        )?;
        register_command!($cr,
            async fn roots(state, context) -> result
            namespace: "lui"
            label: "Roots"
            doc: "Get all root element handles"

            payload: required
        version: auto
        )?;
        register_command!($cr,
            async fn activate(state, target_word: String, context) -> result
            namespace: "lui"
            label: "Activate"
            doc: "Set the active element"

            payload: required
        version: auto
        )?;
        register_command!($cr,
            fn ui_spec(state, context) -> result
            namespace: "lui"
            label: "UI Spec"
            doc: "Create UISpecElement from YAML specification"

        version: auto
        )?;
        register_command!($cr,
            fn markdown(state) -> result
            namespace: "lui"
            label: "Markdown"
            doc: "Create a markdown viewer element"

        version: auto
        )?;
        register_command!($cr,
            fn query_console(state, mode: String = "immediate" (
                label: "Update mode",
                description: "When an edit is evaluated: immediate (after each edit) or on_demand (on ✓ or Enter)",
                enum: ["immediate", "on_demand"]
            )) -> result
            namespace: "lui"
            label: "Query Console"
            doc: "Create an interactive query console element"

        version: auto
        )?;
        register_command!($cr,
            fn query_editor(state, mode: String = "preview" (
                label: "Update mode",
                description: "When an edit takes effect: preview (shown in place until ✓), immediate (written at once) or on_demand (written on ✓)",
                enum: ["preview", "immediate", "on_demand"]
            )) -> result
            namespace: "lui"
            label: "Query editor"
            doc: "An editor of a query or pipeline: a tree of its steps beside the highlighted text"
        version: auto
        )?;
        register_command!($cr,
            fn action_editor(state, mode: String = "immediate" (
                label: "Update mode",
                description: "When an edit is written: immediate (at once) or on_demand (on ✓)",
                enum: ["immediate", "on_demand"]
            )) -> result
            namespace: "lui"
            label: "Action editor"
            doc: "An editor of one action's arguments, from the command's argument metadata"
        version: auto
        )?;
        register_command!($cr,
            fn resource_editor(state, mode: String = "immediate" (
                label: "Update mode",
                description: "When an edit is written: immediate (at once) or on_demand (on ✓)",
                enum: ["immediate", "on_demand"]
            )) -> result
            namespace: "lui"
            label: "Resource editor"
            doc: "An editor of a resource query: its header and its key, with a store file selector"
        version: auto
        )?;
        register_command!($cr,
            fn key_editor(state, mode: String = "immediate" (
                label: "Update mode",
                description: "When an edit is written: immediate (at once) or on_demand (on ✓)",
                enum: ["immediate", "on_demand"]
            )) -> result
            namespace: "lui"
            label: "Key editor"
            doc: "An editor of a key, with a store file selector"
        version: auto
        )?;
        register_command!($cr,
            fn submit(state, context) -> result
            namespace: "lui"
            label: "Submit"
            doc: "Interpret the input state as a query string and submit it for the current element"

            payload: required
        version: auto
        )?;
        Ok::<(), liquers_core::error::Error>(())
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::value::ValueInterface;

    #[test]
    fn handles_to_value_returns_json_array_of_i64() -> Result<(), Box<dyn std::error::Error>> {
        let handles = vec![UIHandle(1), UIHandle(2), UIHandle(42)];
        let value = handles_to_value(&handles)?;
        let json = value.try_into_json_value()?;
        assert_eq!(json, serde_json::json!([1, 2, 42]));
        Ok(())
    }

    /// query-update-modes AC-20: each element starts in the mode its `mode` argument names; an
    /// unknown name is refused.
    #[test]
    fn mode_argument_sets_the_element_mode() -> Result<(), Box<dyn std::error::Error>> {
        use crate::value::ExtValueInterface;
        let state = State::<Value>::new();
        let console = query_console(&State::new().with_data(Value::from("-R/a.csv")), "on_demand".into())?;
        let console = console.as_ui_element()?;
        let console = console.as_any().and_then(|a| a.downcast_ref::<QueryConsoleElement>()).ok_or("console")?;
        assert_eq!(console.update_mode(), UpdateMode::OnDemand);
        assert_eq!(console.query_text, "-R/a.csv");
        let editor_mode = |value: Value| -> Result<UpdateMode, Box<dyn std::error::Error>> {
            let element = value.as_ui_element()?;
            let editor = element.as_any().and_then(|a| a.downcast_ref::<EditorElement>()).ok_or("editor")?;
            Ok(editor.update_mode())
        };
        assert_eq!(editor_mode(query_editor(&state, "preview".into())?)?, UpdateMode::LivePreview);
        assert_eq!(editor_mode(query_editor(&state, "immediate".into())?)?, UpdateMode::ApplyImmediately);
        assert_eq!(editor_mode(key_editor(&state, "on_demand".into())?)?, UpdateMode::OnDemand);
        assert!(query_console(&state, "sometimes".into()).is_err());
        Ok(())
    }

    #[test]
    fn handles_to_value_fails_on_i64_overflow() {
        let handles = vec![UIHandle(u64::MAX)];
        let res = handles_to_value(&handles);
        assert!(res.is_err());
    }
}
