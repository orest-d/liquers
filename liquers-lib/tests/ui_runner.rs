//! Integration tests for AppRunner: widget interaction, pending evaluation, and error handling.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_macro::register_command;

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

/// Test the full widget interaction loop headlessly (no egui):
///
/// 1. TestWidget receives a Custom update → submits query via UIContext
/// 2. AppRunner processes SubmitQuery inline via evaluate_immediately
/// 3. The `hello` command produces a value
/// 4. The `add-instead` command (with default reference_word="current") replaces
///    the element at the current handle with an AssetViewElement wrapping the value
/// 5. After run(), the root element is an AssetViewElement (not the original TestWidget)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_widget_interaction_headless() {
    // 1. Create environment with hello + lui commands
    let env = setup_env();
    let envref = env.to_ref();

    // 2. Create AppState with a root node (ElementSource::None = no auto-eval)
    let mut direct_state = DirectAppState::new();
    let root_handle = direct_state
        .add_node(None, 0, ElementSource::None)
        .expect("add root node");

    // 3. Set TestWidget on the root — its query will be submitted on Custom update
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
    //    The update sends a SubmitQuery message via ui_context.submit_query_current
    {
        let mut state = app_state.lock().await;
        let mut elem = state.take_element(root_handle).expect("take element");
        let trigger = UpdateMessage::Custom(Box::new(()));
        let response = elem.update(&trigger, &ui_context);
        assert_eq!(response, UpdateResponse::NeedsRepaint);
        state.put_element(root_handle, elem).expect("put element");
    }

    // 6. Run AppRunner — processes SubmitQuery via evaluate_immediately.
    //    The `add-instead` command sets the element at root_handle directly.
    runner.run(&app_state).await.expect("runner.run");

    // 7. Verify: root element replaced by AssetViewElement, single root
    assert!(!runner.has_evaluating(), "no in-flight evaluations");
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
        .set_element(
            root_handle,
            Box::new(liquers_lib::ui::Placeholder::new()),
        )
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
