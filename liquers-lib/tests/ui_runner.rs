//! Integration tests for AppRunner: widget interaction, pending evaluation, and error handling.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::metadata::Status;
use liquers_core::state::State;
use liquers_macro::register_command;

use liquers_core::value::ValueInterface;
use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::ui::{
    app_message_channel, AppRunner, AppState, DirectAppState, ElementSource, ElementStatusInfo,
    UIContext, UIElement, UIHandle, UpdateMessage, UpdateResponse,
};
use liquers_lib::value::Value;

// Required by register_command! and register_lui_commands! macros.
type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

// ─── TestWidget ──────────────────────────────────────────────────────────────

/// Widget that submits a query on any Custom update message.
/// Used to test headless widget interaction without egui.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestWidget {
    handle: Option<UIHandle>,
    title_text: String,
    query: String,
}

impl TestWidget {
    fn new(query: impl Into<String>) -> Self {
        Self {
            handle: None,
            title_text: "TestWidget".to_string(),
            query: query.into(),
        }
    }
}

#[typetag::serde]
impl UIElement for TestWidget {
    fn type_name(&self) -> &'static str {
        "TestWidget"
    }

    fn handle(&self) -> Option<UIHandle> {
        self.handle
    }

    fn set_handle(&mut self, handle: UIHandle) {
        self.handle = Some(handle);
    }

    fn title(&self) -> String {
        self.title_text.clone()
    }

    fn set_title(&mut self, title: String) {
        self.title_text = title;
    }

    fn clone_boxed(&self) -> Box<dyn UIElement> {
        Box::new(self.clone())
    }

    fn update(&mut self, message: &UpdateMessage, ctx: &UIContext) -> UpdateResponse {
        match message {
            UpdateMessage::Custom(_) => {
                ctx.submit_query_current(&self.query);
                UpdateResponse::NeedsRepaint
            }
            UpdateMessage::AssetNotification(_) => UpdateResponse::Unchanged,
            UpdateMessage::Timer { .. } => UpdateResponse::Unchanged,
            UpdateMessage::AssetUpdate(_) => UpdateResponse::Unchanged,
            UpdateMessage::UI(_) => UpdateResponse::Unchanged,
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// A command that returns a greeting string.
fn hello(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("Hello from test!"))
}

/// Create a test environment with `hello` and `lui` commands registered.
fn setup_env() -> DefaultEnvironment<Value, SimpleUIPayload> {
    let mut env = DefaultEnvironment::<Value, SimpleUIPayload>::new();
    env.with_trivial_recipe_provider();
    register_commands(&mut env).expect("register commands");
    env
}

fn register_commands(env: &mut DefaultEnvironment<Value, SimpleUIPayload>) -> Result<(), Error> {
    let cr = env.get_mut_command_registry();
    register_command!(cr, fn hello(state) -> result)?;
    liquers_lib::register_lui_commands!(cr)?;
    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// Test the widget interaction with `/q/` (query-value) path:
///
/// 1. TestWidget receives a Custom update → submits `hello/q/ns-lui/add-instead`
/// 2. AppRunner processes SubmitQuery inline via evaluate_immediately
/// 3. The `/q/` instruction wraps "hello" as a `Value::Query("hello")`
/// 4. `add-instead` calls `insert_state`, which detects the Query value and creates
///    a pending node with `ElementSource::Query("hello")` and element=None
/// 5. AppRunner's Phase 2 picks up the pending node and starts async evaluation
/// 6. After polling, the element becomes an AssetViewElement wrapping the actual value
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_widget_interaction_query_value() {
    // 1. Create environment with hello + lui commands
    let env = setup_env();
    let envref = env.to_ref();

    // 2. Create AppState with a root node (ElementSource::None = no auto-eval)
    let mut direct_state = DirectAppState::new();
    let root_handle = direct_state
        .add_node(None, 0, ElementSource::None)
        .expect("add root node");

    // 3. Set TestWidget on the root — query uses /q/ to pass hello as a query value
    let widget = TestWidget::new("hello/q/ns-lui/add-instead");
    direct_state
        .set_element(root_handle, Box::new(widget))
        .expect("set TestWidget");

    // 4. Wrap in Arc<Mutex>, create channel and runner
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(direct_state));
    let (msg_tx, msg_rx) = app_message_channel();
    let ui_context =
        UIContext::new(app_state.clone(), msg_tx.clone()).with_handle(Some(root_handle));
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx);

    // 5. Extract element → trigger update → put back
    //    The update sends a SubmitQuery message via ui_context.submit_query_current
    {
        let mut state = app_state.lock().await;
        let mut elem = state.take_element(root_handle).expect("take element");
        let trigger = UpdateMessage::Custom(Box::new(()));
        let response = elem.update(&trigger, &ui_context);
        assert_eq!(response, UpdateResponse::NeedsRepaint);
        state.put_element(root_handle, elem).expect("put element");
    }

    // 6. Poll loop: first run() processes SubmitQuery → add-instead creates pending node,
    //    subsequent runs evaluate the pending query and poll for completion.
    let mut completed = false;
    for _ in 0..200 {
        runner.run(&app_state).await.expect("runner.run");

        if !runner.has_evaluating() {
            let state = app_state.lock().await;
            if let Ok(Some(elem)) = state.get_element(root_handle) {
                if elem.type_name() == "AssetViewElement" {
                    // Verify the actual value
                    if let Some(value) = elem.get_value() {
                        if let Ok(text) = value.try_into_string() {
                            if text == "Hello from test!" {
                                completed = true;
                                break;
                            }
                        }
                    }
                }
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    assert!(
        completed,
        "query-value evaluation should complete within timeout"
    );

    // 7. Verify final state
    let state = app_state.lock().await;
    assert_eq!(
        runner.element_status(&*state, root_handle),
        ElementStatusInfo::Ready
    );
    assert_eq!(state.roots().len(), 1);
}

/// Test the widget interaction with direct value path (no `/q/`):
///
/// 1. TestWidget receives a Custom update → submits `hello/ns-lui/add-instead`
/// 2. AppRunner processes SubmitQuery inline via evaluate_immediately
/// 3. `hello` is evaluated first, producing "Hello from test!" (a string value)
/// 4. `add-instead` calls `insert_state`, which wraps the string in a StateViewElement
/// 5. After a single run(), the root element is a StateViewElement (immediate, no polling)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_widget_interaction_direct_value() {
    // 1. Create environment with hello + lui commands
    let env = setup_env();
    let envref = env.to_ref();

    // 2. Create AppState with a root node (ElementSource::None = no auto-eval)
    let mut direct_state = DirectAppState::new();
    let root_handle = direct_state
        .add_node(None, 0, ElementSource::None)
        .expect("add root node");

    // 3. Set TestWidget on the root — query does NOT use /q/, so hello runs first
    let widget = TestWidget::new("hello/ns-lui/add-instead");
    direct_state
        .set_element(root_handle, Box::new(widget))
        .expect("set TestWidget");

    // 4. Wrap in Arc<Mutex>, create channel and runner
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(direct_state));
    let (msg_tx, msg_rx) = app_message_channel();
    let ui_context =
        UIContext::new(app_state.clone(), msg_tx.clone()).with_handle(Some(root_handle));
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx);

    // 5. Extract element → trigger update → put back
    {
        let mut state = app_state.lock().await;
        let mut elem = state.take_element(root_handle).expect("take element");
        let trigger = UpdateMessage::Custom(Box::new(()));
        let response = elem.update(&trigger, &ui_context);
        assert_eq!(response, UpdateResponse::NeedsRepaint);
        state.put_element(root_handle, elem).expect("put element");
    }

    // 6. Run AppRunner — processes SubmitQuery via evaluate_immediately.
    //    The `add-instead` command inserts a StateViewElement wrapping the hello output.
    runner.run(&app_state).await.expect("runner.run");

    // 7. Verify: root element replaced by StateViewElement with the actual value
    let state = app_state.lock().await;
    assert_eq!(
        runner.element_status(&*state, root_handle),
        ElementStatusInfo::Ready
    );
    let elem = state
        .get_element(root_handle)
        .expect("get element")
        .expect("element should be present");
    assert_eq!(elem.type_name(), "StateViewElement");

    // Verify actual value
    let value = elem.get_value().expect("element should have a value");
    let text = value
        .try_into_string()
        .expect("value should be convertible to string");
    assert_eq!(text, "Hello from test!");
    assert_eq!(state.roots().len(), 1);
}

/// Test that pending nodes (element=None, source=Query) are auto-evaluated
/// by AppRunner's Phase 2 + Phase 3.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_pending_auto_evaluation() {
    // 1. Create environment
    let env = setup_env();
    let envref = env.to_ref();

    // 2. Create AppState with a pending node (Query source, no element)
    let mut direct_state = DirectAppState::new();
    let root_handle = direct_state
        .add_node(None, 0, ElementSource::Query("hello".to_string()))
        .expect("add root node");
    // element is None → this is a pending node

    // 3. Wrap and create runner
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(direct_state));
    let (msg_tx, msg_rx) = app_message_channel();
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx);

    // 4. Poll loop: run() starts evaluation, then polls until complete
    let mut completed = false;
    for _ in 0..200 {
        runner.run(&app_state).await.expect("runner.run");

        if !runner.has_evaluating() {
            let state = app_state.lock().await;
            if let Ok(Some(elem)) = state.get_element(root_handle) {
                if elem.type_name() == "AssetViewElement" {
                    completed = true;
                    break;
                }
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    assert!(completed, "evaluation should complete within timeout");

    // 5. Verify element is AssetViewElement (value mode, wrapping hello output)
    let state = app_state.lock().await;
    assert_eq!(
        runner.element_status(&*state, root_handle),
        ElementStatusInfo::Ready
    );
    let elem = state
        .get_element(root_handle)
        .expect("get element")
        .expect("element should be present");
    assert_eq!(elem.type_name(), "AssetViewElement");

    // 6. Verify actual value via get_value()
    let value = elem.get_value().expect("element should have a value");
    let text = value
        .try_into_string()
        .expect("value should be convertible to string");
    assert_eq!(text, "Hello from test!");
}

/// Test that submitting an invalid query results in an AssetViewElement in Error mode.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_error_handling() {
    // 1. Create environment
    let env = setup_env();
    let envref = env.to_ref();

    // 2. Create AppState with a root node and a placeholder element
    let mut direct_state = DirectAppState::new();
    let root_handle = direct_state
        .add_node(None, 0, ElementSource::None)
        .expect("add root node");
    direct_state
        .set_element(root_handle, Box::new(liquers_lib::ui::Placeholder::new()))
        .expect("set placeholder");

    // 3. Wrap and create runner
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(direct_state));
    let (msg_tx, msg_rx) = app_message_channel();
    let ui_context = UIContext::new(app_state.clone(), msg_tx.clone());
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx);

    // 4. Submit a query for a nonexistent command
    ui_context.submit_query(root_handle, "nonexistent_command_xyz");

    // 5. Run — processes SubmitQuery, evaluation should fail
    runner.run(&app_state).await.expect("runner.run");

    // 6. Verify: element replaced by AssetViewElement (error mode)
    let state = app_state.lock().await;
    let elem = state
        .get_element(root_handle)
        .expect("get element")
        .expect("element should be present after error");
    assert_eq!(elem.type_name(), "AssetViewElement");
}

// ─── submit_root_query Tests ─────────────────────────────────────────────────

const TEST_SPEC_YAML: &str = r#"
layout: !vertical null
init:
  - "hello/ns-lui/add-child"
"#;

/// Return the test YAML spec.
fn test_spec(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(TEST_SPEC_YAML))
}

/// Replicate the ui_query_console_app flow:
///
/// 1. Empty AppState (no root)
/// 2. submit_root_query("test_spec/ns-lui/ui_spec/add-child")
/// 3. AppRunner processes SubmitQuery → evaluate_immediately:
///    - test_spec → YAML string
///    - ns-lui → set namespace
///    - ui_spec → parse YAML, create UISpecElement
///    - add-child → insert_state with InsertionPoint::Root (since handle=None)
/// 4. Verify root element is created (roots() is non-empty)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_submit_root_query_creates_root_element() {
    // 1. Create environment with test_spec + lui commands
    let mut env = setup_env();
    {
        let cr = env.get_mut_command_registry();
        register_command!(cr, fn test_spec(state) -> result).expect("register test_spec");
    }
    let envref = env.to_ref();

    // 2. Create empty AppState — no root node
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));

    // 3. Create UIContext and AppRunner
    let (msg_tx, msg_rx) = app_message_channel();
    let ui_context = UIContext::new(app_state.clone(), msg_tx.clone());
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx);

    // 4. Verify no roots initially
    {
        let state = app_state.lock().await;
        assert!(state.roots().is_empty(), "should start with no roots");
    }

    // 5. Submit root query — same pattern as ui_query_console_app
    ui_context.submit_root_query("test_spec/ns-lui/ui_spec/add-child");

    // 6. Run AppRunner — processes SubmitQuery via evaluate_immediately
    runner.run(&app_state).await.expect("runner.run");

    // 7. Verify: root element should be created
    let state = app_state.lock().await;
    let roots = state.roots();
    assert!(
        !roots.is_empty(),
        "root element should be created after submit_root_query"
    );

    // 8. Verify root element is a UISpecElement
    let root_handle = roots[0];
    let elem = state
        .get_element(root_handle)
        .expect("get element")
        .expect("root element should be present");
    assert_eq!(
        elem.type_name(),
        "UISpecElement",
        "root element should be a UISpecElement"
    );

    // 9. Verify handle is set on the element (init must have been called)
    assert_eq!(
        elem.handle(),
        Some(root_handle),
        "element handle should be set (init must be called after insertion)"
    );
}

/// Test that the app's full YAML (menu bar, separators, actions) parses correctly.
/// Uses the same YAML structure as ui_query_console_app but without init queries
/// (init queries reference app-specific commands not available in this test env).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_submit_root_query_with_app_yaml() {
    const APP_YAML: &str = r#"
menu:
  items:
  - !menu
    label: File
    items:
    - !button
      label: New Console
      shortcut: Ctrl+N
      action:
        query: "hello/q/ns-lui/add-child"
    - !separator null
    - !button
      label: Quit
      shortcut: Ctrl+Q
      action: quit
  - !menu
    label: Help
    items:
    - !button
      label: About
      action: null
layout: !windows null
"#;

    fn app_spec(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from(APP_YAML))
    }

    // 1. Create environment
    let mut env = setup_env();
    {
        let cr = env.get_mut_command_registry();
        register_command!(cr, fn app_spec(state) -> result).expect("register app_spec");
    }
    let envref = env.to_ref();

    // 2. Empty AppState
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));

    // 3. UIContext and AppRunner
    let (msg_tx, msg_rx) = app_message_channel();
    let ui_context = UIContext::new(app_state.clone(), msg_tx.clone());
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx);

    // 4. Submit root query
    ui_context.submit_root_query("app_spec/ns-lui/ui_spec/add-child");

    // 5. Run
    runner.run(&app_state).await.expect("runner.run");

    // 6. Verify root was created with correct type and handle
    let state = app_state.lock().await;
    let roots = state.roots();
    assert!(
        !roots.is_empty(),
        "root element should be created (app YAML parse must succeed)"
    );
    let root_handle = roots[0];
    let elem = state
        .get_element(root_handle)
        .expect("get element")
        .expect("root element should be present");
    assert_eq!(elem.type_name(), "UISpecElement");
    assert_eq!(
        elem.handle(),
        Some(root_handle),
        "element handle should be set"
    );
}

// ─── Runner events, stale results, environment replacement (design web-ui-use) ─

/// A command that takes long enough for a test to change the node's source meanwhile.
async fn slow(_state: State<Value>) -> Result<Value, Error> {
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    Ok(Value::from("slow result"))
}

fn setup_env_with_slow() -> DefaultEnvironment<Value, SimpleUIPayload> {
    let mut env = setup_env();
    let cr = env.get_mut_command_registry();
    register_command!(cr, async fn slow(state) -> result).expect("register slow");
    env
}

fn single_pending(query: &str) -> (Arc<tokio::sync::Mutex<dyn AppState>>, UIHandle) {
    let mut direct_state = DirectAppState::new();
    let handle = direct_state
        .add_node(None, 0, ElementSource::Query(query.to_string()))
        .expect("add node");
    (Arc::new(tokio::sync::Mutex::new(direct_state)), handle)
}

/// Runs the runner until no evaluation is in flight, or fails after about two seconds.
async fn run_until_idle(
    runner: &mut AppRunner<DefaultEnvironment<Value, SimpleUIPayload>>,
    app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
) {
    for _ in 0..200 {
        runner.run(app_state).await.expect("runner.run");
        let idle = !runner.has_evaluating() && app_state.lock().await.pending_nodes().is_empty();
        if idle {
            return;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }
    panic!("the runner did not become idle");
}

/// AC-4: a pending node records `Processing`, then `Ready`, and `take_events` drains them.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_events_record_processing_then_ready() {
    let envref = setup_env().to_ref();
    let (app_state, handle) = single_pending("hello");
    let (msg_tx, msg_rx) = app_message_channel();
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx);

    run_until_idle(&mut runner, &app_state).await;
    let events = runner.take_events();
    let statuses: Vec<(Option<UIHandle>, Status)> =
        events.iter().map(|e| (e.handle, e.status)).collect();
    assert_eq!(
        statuses,
        vec![
            (Some(handle), Status::Processing),
            (Some(handle), Status::Ready)
        ]
    );
    assert!(events.iter().all(|e| e.error.is_none()));
    assert!(runner.take_events().is_empty(), "take_events drains");
}

/// AC-7: a failing node records `Error` with its handle; a failing root query has no handle.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_events_record_errors_with_and_without_handle() {
    let envref = setup_env().to_ref();
    let (app_state, handle) = single_pending("nonexistent_command_xyz");
    let (msg_tx, msg_rx) = app_message_channel();
    let ui_context = UIContext::new(app_state.clone(), msg_tx.clone());
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx);

    run_until_idle(&mut runner, &app_state).await;
    let events = runner.take_events();
    let last = events.last().expect("an event");
    assert_eq!(last.handle, Some(handle));
    assert_eq!(last.status, Status::Error);
    assert!(last.error.is_some(), "the error is carried");

    ui_context.submit_root_query("another_nonexistent_command");
    runner.run(&app_state).await.expect("runner.run");
    let events = runner.take_events();
    assert!(
        events
            .iter()
            .any(|e| e.handle.is_none() && e.status == Status::Error && e.error.is_some()),
        "a failing root query is reported without a handle: {events:?}"
    );
}

/// AC-6: a result whose node changed source meanwhile is dropped, and the new query runs at once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_drops_result_when_source_changed() {
    let envref = setup_env_with_slow().to_ref();
    let (app_state, handle) = single_pending("slow");
    let (msg_tx, msg_rx) = app_message_channel();
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx);

    // Start the slow query; it is still running after this tick.
    runner.run(&app_state).await.expect("runner.run");
    assert!(runner.has_evaluating(), "slow is in flight");

    // Show something else on the same node, as `ui.show(query, handle)` does.
    app_state
        .lock()
        .await
        .insert_source(
            &liquers_lib::ui::InsertionPoint::Instead(handle),
            ElementSource::Query("hello".to_string()),
        )
        .expect("replace source");

    run_until_idle(&mut runner, &app_state).await;
    let state = app_state.lock().await;
    let value = state
        .get_element(handle)
        .expect("get")
        .expect("element")
        .get_value()
        .expect("value");
    assert_eq!(
        value.try_into_string().expect("text"),
        "Hello from test!",
        "the new query's result is shown, not the stale one"
    );
    drop(state);

    // The stale evaluation finishing later must not overwrite it.
    tokio::time::sleep(tokio::time::Duration::from_millis(400)).await;
    runner.run(&app_state).await.expect("runner.run");
    let state = app_state.lock().await;
    let value = state
        .get_element(handle)
        .expect("get")
        .expect("element")
        .get_value()
        .expect("value");
    assert_eq!(value.try_into_string().expect("text"), "Hello from test!");
    let readies = runner
        .take_events()
        .iter()
        .filter(|e| e.status == Status::Ready)
        .count();
    assert_eq!(readies, 1, "only the current query reports Ready");
}

/// AC-9: evaluations after `replace_envref` use the new environment.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_replace_envref_used_by_later_evaluations() {
    let first = setup_env().to_ref();
    let second = setup_env_with_slow().to_ref();
    let (app_state, handle) = single_pending("slow");
    let (msg_tx, msg_rx) = app_message_channel();
    let mut runner = AppRunner::new(first, msg_rx, msg_tx);
    runner.replace_envref(second);

    run_until_idle(&mut runner, &app_state).await;
    let state = app_state.lock().await;
    let value = state
        .get_element(handle)
        .expect("get")
        .expect("element")
        .get_value()
        .expect("value");
    assert_eq!(value.try_into_string().expect("text"), "slow result");
}
