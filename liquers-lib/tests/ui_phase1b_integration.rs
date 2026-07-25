//! Integration tests for the Phase 1b evaluation lifecycle (`specs/UI_INTERFACE_PHASE1b.md`
//! → "Testing Strategy → Integration Tests").
//!
//! `ui_runner.rs` covers the headless widget-interaction flow and the single-node happy/error
//! path. The Phase 1b plan also asked for the lifecycle properties that only show up in the
//! large: element status transitions, many evaluations in flight at once, recovery after an
//! error, and an `AppState` that survives a serialization round trip. Those live here.
//!
//! Note on the spec's API: Phase 1b predates the `AppRunner` split, so its snippets call
//! `app_state.run()` / `get_element_status()`. The equivalents used below are
//! `AppRunner::run(&app_state)` and `AppRunner::element_status(&state, handle)`.

use std::sync::Arc;

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_core::value::ValueInterface;
use liquers_macro::register_command;

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::ui::widgets::markdown_element::MarkdownElement;
use liquers_lib::ui::{
    app_message_channel, AppRunner, AppState, DirectAppState, ElementSource, ElementStatusInfo,
    Placeholder, UIContext, UIHandle,
};
use liquers_lib::value::Value;

// Required by register_command! and register_lui_commands! macros.
type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// A command that returns a greeting string.
fn hello(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("Hello from test!"))
}

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

/// Run the runner until `predicate` holds, or give up after `max_iterations` ticks.
async fn run_until<F>(
    runner: &mut AppRunner<CommandEnvironment>,
    app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
    max_iterations: usize,
    predicate: F,
) -> bool
where
    F: Fn(&dyn AppState) -> bool,
{
    for _ in 0..max_iterations {
        runner.run(app_state).await.expect("runner.run");
        {
            let state = app_state.lock().await;
            if predicate(&*state) {
                return true;
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }
    false
}

/// True when the node carries a ready element wrapping the `hello` output.
fn is_ready_hello(state: &dyn AppState, handle: UIHandle) -> bool {
    match state.get_element(handle) {
        Ok(Some(elem)) => elem
            .get_value()
            .and_then(|v| v.try_into_string().ok())
            .map(|text| text == "Hello from test!")
            .unwrap_or(false),
        Ok(None) | Err(_) => false,
    }
}

// ─── 1. Full evaluation flow / status transitions ────────────────────────────

/// A pending node (element = None, source = Query) walks Pending → Evaluating → Ready, and the
/// element it ends up with wraps the command's value.
///
/// The Pending and Ready ends are asserted directly. The intermediate `Evaluating` state is not:
/// `AppRunner::run` starts and polls evaluations within a single call, so a fast command can pass
/// through it entirely inside one tick — asserting it would be a race. What is asserted instead is
/// that the evaluation map is drained afterwards, which only holds if the node went through it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_node_transitions_to_ready() {
    let envref = setup_env().to_ref();

    let mut direct_state = DirectAppState::new();
    let handle = direct_state
        .add_node(None, 0, ElementSource::Query("hello".to_string()))
        .expect("add pending node");
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(direct_state));
    let (msg_tx, msg_rx) = app_message_channel();
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx);

    // Before any run: no element, no evaluation → Pending.
    {
        let state = app_state.lock().await;
        assert_eq!(
            runner.element_status(&*state, handle),
            ElementStatusInfo::Pending
        );
        assert_eq!(state.pending_nodes(), vec![handle]);
    }

    let ready = run_until(&mut runner, &app_state, 200, |state| {
        is_ready_hello(state, handle)
    })
    .await;
    assert!(ready, "pending node should reach Ready within the timeout");

    let state = app_state.lock().await;
    assert_eq!(
        runner.element_status(&*state, handle),
        ElementStatusInfo::Ready
    );
    assert!(
        state.pending_nodes().is_empty(),
        "a node with an element is no longer pending"
    );
    assert!(!runner.has_evaluating(), "evaluation map must be drained");
}

/// A pending node whose query names no command ends up with an error element, not a panic and
/// not a node stuck in `Evaluating` forever.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_node_with_invalid_query_becomes_error_element() {
    let envref = setup_env().to_ref();

    let mut direct_state = DirectAppState::new();
    let handle = direct_state
        .add_node(
            None,
            0,
            ElementSource::Query("nonexistent_command_xyz".to_string()),
        )
        .expect("add pending node");
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(direct_state));
    let (msg_tx, msg_rx) = app_message_channel();
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx);

    let settled = run_until(&mut runner, &app_state, 200, |state| {
        match state.get_element(handle) {
            Ok(Some(elem)) => elem.type_name() == "AssetViewElement" && elem.get_value().is_none(),
            Ok(None) | Err(_) => false,
        }
    })
    .await;
    assert!(settled, "a failing query must settle into an error element");

    let state = app_state.lock().await;
    assert_eq!(
        runner.element_status(&*state, handle),
        ElementStatusInfo::Ready,
        "the node holds an (error) element, so it is no longer pending or evaluating"
    );
    assert!(!runner.has_evaluating());
}

// ─── 2. Concurrent evaluations ───────────────────────────────────────────────

/// Ten pending nodes evaluate concurrently; every one of them completes with the right value and
/// the evaluation map is empty afterwards.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_evaluations_all_complete() {
    const COUNT: usize = 10;

    let envref = setup_env().to_ref();

    let mut direct_state = DirectAppState::new();
    let mut handles = Vec::with_capacity(COUNT);
    for _ in 0..COUNT {
        handles.push(
            direct_state
                .add_node(None, 0, ElementSource::Query("hello".to_string()))
                .expect("add pending node"),
        );
    }
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(direct_state));
    let (msg_tx, msg_rx) = app_message_channel();
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx);

    let all_ready = {
        let expected = handles.clone();
        run_until(&mut runner, &app_state, 300, move |state| {
            expected.iter().all(|h| is_ready_hello(state, *h))
        })
        .await
    };
    assert!(all_ready, "all {COUNT} evaluations should complete");

    let state = app_state.lock().await;
    assert_eq!(state.roots().len(), COUNT);
    for handle in &handles {
        assert_eq!(
            runner.element_status(&*state, *handle),
            ElementStatusInfo::Ready
        );
    }
    assert!(!runner.has_evaluating());
    assert_eq!(runner.evaluating_count(), 0);
}

// ─── 3. Error → retry → ready ────────────────────────────────────────────────

/// After a failed submission the same handle can be retried with a valid query and reaches Ready:
/// an error is a recoverable state, not a dead end.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn error_then_retry_reaches_ready() {
    let envref = setup_env().to_ref();

    let mut direct_state = DirectAppState::new();
    let handle = direct_state
        .add_node(None, 0, ElementSource::None)
        .expect("add root node");
    direct_state
        .set_element(handle, Box::new(Placeholder::new()))
        .expect("set placeholder");
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(direct_state));
    let (msg_tx, msg_rx) = app_message_channel();
    let ui_context = UIContext::new(app_state.clone(), msg_tx.clone());
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx);

    // 1. Failing submission → error element.
    ui_context.submit_query(handle, "nonexistent_command_xyz");
    runner.run(&app_state).await.expect("runner.run");
    {
        let state = app_state.lock().await;
        let elem = state
            .get_element(handle)
            .expect("get element")
            .expect("error element present");
        assert_eq!(elem.type_name(), "AssetViewElement");
        assert!(elem.get_value().is_none(), "error element holds no value");
    }

    // 2. Retry the same handle with a valid query.
    ui_context.submit_query(handle, "hello/ns-lui/add-instead");
    let recovered = run_until(&mut runner, &app_state, 200, |state| {
        is_ready_hello(state, handle)
    })
    .await;
    assert!(recovered, "a retry should replace the error element");

    let state = app_state.lock().await;
    assert_eq!(
        runner.element_status(&*state, handle),
        ElementStatusInfo::Ready
    );
    assert_eq!(state.roots(), vec![handle], "retry must not add a second root");
}

// ─── 4. Serialization round trip ─────────────────────────────────────────────

/// `AppState` is the persisted part of the UI: a round trip must preserve topology, sources, the
/// active handle, and the elements themselves (via typetag).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn app_state_round_trip_preserves_tree_and_sources() {
    let mut direct_state = DirectAppState::new();
    let root = direct_state
        .add_node(None, 0, ElementSource::None)
        .expect("add root");
    direct_state
        .set_element(root, Box::new(MarkdownElement::new("Doc".into(), "# Title".into())))
        .expect("set root element");
    let ready_child = direct_state
        .add_node(Some(root), 0, ElementSource::Query("hello".to_string()))
        .expect("add ready child");
    direct_state
        .set_element(ready_child, Box::new(Placeholder::new()))
        .expect("set child element");
    // Second child stays pending (element = None) — the interesting case for restart.
    let pending_child = direct_state
        .add_node(Some(root), 1, ElementSource::Query("hello".to_string()))
        .expect("add pending child");
    direct_state.set_active_handle(Some(ready_child));

    let json = serde_json::to_string(&direct_state).expect("serialize app state");
    let restored: DirectAppState = serde_json::from_str(&json).expect("deserialize app state");

    assert_eq!(restored.node_count(), 3);
    assert_eq!(restored.roots(), vec![root]);
    assert_eq!(
        restored.children(root).expect("children"),
        vec![ready_child, pending_child]
    );
    assert_eq!(restored.active_handle(), Some(ready_child));
    assert_eq!(
        restored.get_element(root).expect("get root").map(|e| e.type_name()),
        Some("MarkdownElement")
    );
    assert!(
        restored.get_element(pending_child).expect("get pending").is_none(),
        "a pending node stays pending across a round trip"
    );
    assert_eq!(restored.pending_nodes(), vec![pending_child]);
    match restored.get_source(pending_child).expect("get source") {
        ElementSource::Query(q) => assert_eq!(q, "hello"),
        ElementSource::None | ElementSource::Recipe(_) => panic!("query source not preserved"),
    }
}

/// A restored `AppState` is live: its pending nodes are evaluated by `AppRunner` exactly as if
/// they had just been created — which is what makes save/restore useful.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restored_pending_node_is_evaluated() {
    let mut direct_state = DirectAppState::new();
    let handle = direct_state
        .add_node(None, 0, ElementSource::Query("hello".to_string()))
        .expect("add pending node");
    let json = serde_json::to_string(&direct_state).expect("serialize app state");
    let restored: DirectAppState = serde_json::from_str(&json).expect("deserialize app state");

    let envref = setup_env().to_ref();
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(restored));
    let (msg_tx, msg_rx) = app_message_channel();
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx);

    let ready = run_until(&mut runner, &app_state, 200, |state| {
        is_ready_hello(state, handle)
    })
    .await;
    assert!(ready, "a restored pending node must be evaluated");
}
