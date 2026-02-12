use std::sync::Arc;

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_core::value::ValueInterface;

use crate::value::Value;

use super::app_state::AppState;
use super::handle::UIHandle;
use super::payload::UIPayload;
use super::resolve::{resolve_navigation, resolve_position};

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

    let reference = resolve_navigation(&*app_state, &reference_word, current)?;
    let insertion = resolve_position(&position_word, reference)?;

    // Delegate to AppState's insert_state which handles UIElement detection,
    // StateViewElement wrapping, and source preservation from metadata
    let handle = app_state.insert_state(&insertion, &state)?;

    Ok(Value::from(format!("{}", handle.0)))
}

/// Remove an element from the UI tree.
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
    let handles_str: Vec<String> = child_handles.iter().map(|h| h.0.to_string()).collect();
    Ok(Value::from(handles_str.join(",")))
}

/// Navigate to the first child of the target element.
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
    let handles_str: Vec<String> = root_handles.iter().map(|h| h.0.to_string()).collect();
    Ok(Value::from(handles_str.join(",")))
}

/// Set the active (focused) element.
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
        )?;
        register_command!($cr,
            async fn remove(state, target_word: String, context) -> result
            namespace: "lui"
            label: "Remove element"
            doc: "Remove an element from the UI tree"
        )?;
        register_command!($cr,
            async fn children(state, target_word: String, context) -> result
            namespace: "lui"
            label: "Children"
            doc: "Get children handles of target element"
        )?;
        register_command!($cr,
            async fn first(state, target_word: String, context) -> result
            namespace: "lui"
            label: "First child"
            doc: "Navigate to first child of target"
        )?;
        register_command!($cr,
            async fn last(state, target_word: String, context) -> result
            namespace: "lui"
            label: "Last child"
            doc: "Navigate to last child of target"
        )?;
        register_command!($cr,
            async fn parent(state, target_word: String, context) -> result
            namespace: "lui"
            label: "Parent"
            doc: "Navigate to parent of target"
        )?;
        register_command!($cr,
            async fn next(state, target_word: String, context) -> result
            namespace: "lui"
            label: "Next sibling"
            doc: "Navigate to next sibling of target"
        )?;
        register_command!($cr,
            async fn prev(state, target_word: String, context) -> result
            namespace: "lui"
            label: "Previous sibling"
            doc: "Navigate to previous sibling of target"
        )?;
        register_command!($cr,
            async fn roots(state, context) -> result
            namespace: "lui"
            label: "Roots"
            doc: "Get all root element handles"
        )?;
        register_command!($cr,
            async fn activate(state, target_word: String, context) -> result
            namespace: "lui"
            label: "Activate"
            doc: "Set the active element"
        )?;
        Ok::<(), liquers_core::error::Error>(())
    }};
}
