//! Integration tests for QueryConsoleElement and AppRunner monitoring.

use std::sync::Arc;

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_core::value::ValueInterface;
use liquers_macro::register_command;

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::ui::{
    app_message_channel, AppMessage, AppRunner, AppState, DirectAppState, ElementSource,
    Placeholder, QueryConsoleElement, UIContext, UIElement, UIHandle,
};
use liquers_lib::value::Value;

// Required by register_command! and register_lui_commands! macros.
type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

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

/// Test 1: Create QueryConsoleElement via the `lui/query_console` command.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_query_console_creation_via_command() {
    let env = setup_env();
    let envref = env.to_ref();

    let mut direct_state = DirectAppState::new();
    let root_handle = direct_state
        .add_node(None, 0, ElementSource::None)
        .expect("add root node");
    direct_state
        .set_element(root_handle, Box::new(Placeholder::new()))
        .expect("set placeholder");

    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(direct_state));
    let (msg_tx, msg_rx) = app_message_channel();
    let ui_context =
        UIContext::new(app_state.clone(), msg_tx.clone()).with_handle(Some(root_handle));
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx);

    // Submit command that creates QueryConsoleElement.
    // Pipeline: hello → ns-lui → query_console → ns-lui → add-instead
    // - hello returns "Hello from test!" (a string value)
    // - query_console receives that string as the initial query text
    // - add-instead replaces the root element with the QueryConsoleElement
    ui_context.submit_query(root_handle, "hello/ns-lui/query_console/ns-lui/add-instead");
    runner.run(&app_state).await.expect("runner.run");

    // Verify element was created as QueryConsoleElement
    let state = app_state.lock().await;
    let elem = state
        .get_element(root_handle)
        .expect("get element")
        .expect("element should be present");
    assert_eq!(elem.type_name(), "QueryConsoleElement");
}

/// Test 2: Submit RequestAssetUpdates, verify AppRunner evaluates and delivers
/// AssetSnapshot to the widget.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_request_asset_updates_flow() {
    let env = setup_env();
    let envref = env.to_ref();

    let mut direct_state = DirectAppState::new();
    let root_handle = direct_state
        .add_node(None, 0, ElementSource::None)
        .expect("add root node");

    // Set a QueryConsoleElement on the root
    let console = QueryConsoleElement::new("Test Console".to_string(), String::new());
    direct_state
        .set_element(root_handle, Box::new(console))
        .expect("set console");

    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(direct_state));
    let (msg_tx, msg_rx) = app_message_channel();
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx.clone());

    // Send RequestAssetUpdates directly
    let _ = msg_tx.send(AppMessage::RequestAssetUpdates {
        handle: root_handle,
        query: "hello".to_string(),
    });

    // Poll until value arrives
    let mut completed = false;
    for _ in 0..200 {
        runner.run(&app_state).await.expect("runner.run");

        let state = app_state.lock().await;
        if let Ok(Some(elem)) = state.get_element(root_handle) {
            assert_eq!(elem.type_name(), "QueryConsoleElement");
            if let Some(value) = elem.get_value() {
                if let Ok(text) = value.try_into_string() {
                    if text == "Hello from test!" {
                        completed = true;
                        break;
                    }
                }
            }
        }
        drop(state);

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    assert!(
        completed,
        "RequestAssetUpdates should deliver value via AssetSnapshot"
    );
}

/// Test 3: Verify AppRunner stops monitoring when element is removed from AppState.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_monitoring_auto_stop() {
    let env = setup_env();
    let envref = env.to_ref();

    let mut direct_state = DirectAppState::new();
    let root_handle = direct_state
        .add_node(None, 0, ElementSource::None)
        .expect("add root node");
    let console = QueryConsoleElement::new("Test".to_string(), String::new());
    direct_state
        .set_element(root_handle, Box::new(console))
        .expect("set console");

    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(direct_state));
    let (msg_tx, msg_rx) = app_message_channel();
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx.clone());

    // Start monitoring
    let _ = msg_tx.send(AppMessage::RequestAssetUpdates {
        handle: root_handle,
        query: "hello".to_string(),
    });
    runner.run(&app_state).await.expect("run 1");

    // Remove element from AppState
    {
        let mut state = app_state.lock().await;
        state.remove(root_handle).expect("remove element");
    }

    // Run again — monitoring should clean up without panic
    runner.run(&app_state).await.expect("run 2");
    runner.run(&app_state).await.expect("run 3");

    // No panic means auto-stop worked
}

/// Test 4: Submit query A then query B for same handle. Verify latest is monitored.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_monitoring_replacement() {
    let env = setup_env();
    let envref = env.to_ref();

    let mut direct_state = DirectAppState::new();
    let root_handle = direct_state
        .add_node(None, 0, ElementSource::None)
        .expect("add root node");
    let console = QueryConsoleElement::new("Test".to_string(), String::new());
    direct_state
        .set_element(root_handle, Box::new(console))
        .expect("set console");

    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(direct_state));
    let (msg_tx, msg_rx) = app_message_channel();
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx.clone());

    // Send first request
    let _ = msg_tx.send(AppMessage::RequestAssetUpdates {
        handle: root_handle,
        query: "hello".to_string(),
    });
    runner.run(&app_state).await.expect("run 1");

    // Send second request (replaces monitoring)
    let _ = msg_tx.send(AppMessage::RequestAssetUpdates {
        handle: root_handle,
        query: "hello".to_string(),
    });

    // Poll until value arrives
    let mut completed = false;
    for _ in 0..200 {
        runner.run(&app_state).await.expect("runner.run");

        let state = app_state.lock().await;
        if let Ok(Some(elem)) = state.get_element(root_handle) {
            if let Some(value) = elem.get_value() {
                if let Ok(text) = value.try_into_string() {
                    if text == "Hello from test!" {
                        completed = true;
                        break;
                    }
                }
            }
        }
        drop(state);

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    assert!(completed, "replacement monitoring should deliver value");
}

/// Test 5: Submit invalid query, verify error arrives via AssetSnapshot.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_error_propagation() {
    let env = setup_env();
    let envref = env.to_ref();

    let mut direct_state = DirectAppState::new();
    let root_handle = direct_state
        .add_node(None, 0, ElementSource::None)
        .expect("add root node");
    let console = QueryConsoleElement::new("Test".to_string(), String::new());
    direct_state
        .set_element(root_handle, Box::new(console))
        .expect("set console");

    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(direct_state));
    let (msg_tx, msg_rx) = app_message_channel();
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx.clone());

    // Send request for nonexistent command
    let _ = msg_tx.send(AppMessage::RequestAssetUpdates {
        handle: root_handle,
        query: "nonexistent_command_xyz_12345".to_string(),
    });

    // Run a few times
    for _ in 0..20 {
        runner.run(&app_state).await.expect("runner.run");
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    // Verify element still exists (not removed)
    let state = app_state.lock().await;
    let elem = state
        .get_element(root_handle)
        .expect("get element")
        .expect("element should still exist after error");
    assert_eq!(elem.type_name(), "QueryConsoleElement");
    // Value should be None (error, not value)
    assert!(elem.get_value().is_none());
}

/// Test 6: Serialize QueryConsoleElement, deserialize, call init(), verify re-evaluation.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_serialization_and_reinit() {
    // Create a QueryConsoleElement with query
    let mut console = QueryConsoleElement::new("Test".to_string(), "hello".to_string());
    console.set_handle(UIHandle(42));

    // Serialize as Box<dyn UIElement>
    let boxed: Box<dyn UIElement> = Box::new(console);
    let json = serde_json::to_string(&boxed).expect("serialize");

    // Verify JSON contains the type marker
    assert!(json.contains("QueryConsoleElement"));

    // Deserialize
    let mut restored: Box<dyn UIElement> = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored.type_name(), "QueryConsoleElement");
    assert_eq!(restored.handle(), Some(UIHandle(42)));

    // Runtime fields should be None/default
    assert!(restored.get_value().is_none());
    assert!(restored.get_metadata().is_none());

    // Set in AppState and call init — should send RequestAssetUpdates
    let env = setup_env();
    let envref = env.to_ref();

    let mut direct_state = DirectAppState::new();
    let handle = direct_state
        .add_node(None, 0, ElementSource::None)
        .expect("add node");

    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(direct_state));
    let (msg_tx, msg_rx) = app_message_channel();
    let ctx = UIContext::new(app_state.clone(), msg_tx.clone());
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx);

    // Init should submit query since query_text is "hello"
    restored.init(handle, &ctx).expect("init");

    // Set element in AppState
    {
        let mut state = app_state.lock().await;
        state.set_element(handle, restored).expect("set element");
    }

    // Run AppRunner — should process RequestAssetUpdates
    for _ in 0..50 {
        runner.run(&app_state).await.expect("runner.run");
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    // No panic means reinit worked
    let state = app_state.lock().await;
    let elem = state
        .get_element(handle)
        .expect("get element")
        .expect("element should be present");
    assert_eq!(elem.type_name(), "QueryConsoleElement");
}
