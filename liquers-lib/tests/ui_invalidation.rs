//! Integration tests for change recording through the runner (`specs/webui-fixes/`, W3).
//!
//! `app_state.rs`'s unit tests pin what each mutating method records. These tests pin the same
//! property one level up, where it actually matters: after `AppRunner::run` has processed a
//! message, has the model told the renderer what changed? A completed mutation that leaves nothing
//! recorded is exactly the W3 defect — the page would stay stale until unrelated async work
//! happened to trigger a repaint.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_macro::register_command;

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::ui::{
    app_message_channel, AppRunner, AppState, DirectAppState, ElementSource, Invalidation,
    QueryConsoleElement, UIChange, UIContext, UIElement, UIHandle, UpdateMessage, UpdateResponse,
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

/// Empty AppState + channel + runner, as in `tests/ui_runner.rs`.
fn harness() -> (
    Arc<tokio::sync::Mutex<dyn AppState>>,
    UIContext,
    AppRunner<CommandEnvironment>,
) {
    let envref = setup_env().to_ref();
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
    let (msg_tx, msg_rx) = app_message_channel();
    let ui_context = UIContext::new(app_state.clone(), msg_tx.clone());
    let runner = AppRunner::new(envref, msg_rx, msg_tx);
    (app_state, ui_context, runner)
}

/// Discard whatever is pending, so a test asserts only on what it triggered.
async fn drain(app_state: &Arc<tokio::sync::Mutex<dyn AppState>>) {
    let mut state = app_state.lock().await;
    let _ = state.take_invalidation();
}

/// Take the pending invalidation, requiring individual changes rather than an escalation.
async fn changes(app_state: &Arc<tokio::sync::Mutex<dyn AppState>>) -> Vec<UIChange> {
    let mut state = app_state.lock().await;
    match state.take_invalidation() {
        Invalidation::Changes(v) => v,
        Invalidation::None => panic!("a completed mutation must be recorded — this is W3"),
        Invalidation::All => panic!("expected individual changes, got All"),
    }
}

/// An element that never asks for a repaint, whatever it is told.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct QuietElement {
    handle: Option<UIHandle>,
}

#[typetag::serde]
impl UIElement for QuietElement {
    fn type_name(&self) -> &'static str {
        "QuietElement"
    }
    fn handle(&self) -> Option<UIHandle> {
        self.handle
    }
    fn set_handle(&mut self, handle: UIHandle) {
        self.handle = Some(handle);
    }
    fn set_title(&mut self, _title: String) {}
    fn clone_boxed(&self) -> Box<dyn UIElement> {
        Box::new(self.clone())
    }
    fn update(&mut self, message: &UpdateMessage, _ctx: &UIContext) -> UpdateResponse {
        match message {
            UpdateMessage::AssetUpdate(_)
            | UpdateMessage::AssetNotification(_)
            | UpdateMessage::Timer { .. }
            | UpdateMessage::Custom(_) => UpdateResponse::Unchanged,
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// I1 — the W3 case: a query that mutates the tree and resolves inline still leaves a record.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sync_mutation_records_change() {
    let (app_state, ui_context, mut runner) = harness();
    let root = {
        let mut state = app_state.lock().await;
        state
            .add_node(None, 0, ElementSource::None)
            .expect("add root node")
    };
    drain(&app_state).await;

    ui_context.submit_query(root, "hello/ns-lui/add-child");
    runner.run(&app_state).await.expect("runner.run");

    let recorded = changes(&app_state).await;
    assert!(
        recorded.iter().any(|c| matches!(
            c,
            UIChange::Inserted { parent: Some(p), .. } if *p == root
        )),
        "expected an Inserted under the root, got {recorded:?}"
    );
}

/// I2 — removal records `Removed` naming the parent, so a renderer can drop exactly one node.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remove_command_records_removed() {
    let (app_state, ui_context, mut runner) = harness();
    let root = {
        let mut state = app_state.lock().await;
        state
            .add_node(None, 0, ElementSource::None)
            .expect("add root node")
    };

    ui_context.submit_query(root, "hello/ns-lui/add-child");
    ui_context.submit_query(root, "hello/ns-lui/add-child");
    runner.run(&app_state).await.expect("runner.run");
    let children = {
        let state = app_state.lock().await;
        state.children(root).expect("children")
    };
    assert_eq!(children.len(), 2, "setup: two panels");
    drain(&app_state).await;

    // The demo's "Remove Last Panel" action: resolves fully inline, nothing left pending.
    ui_context.submit_query(root, "ns-lui/remove-last");
    runner.run(&app_state).await.expect("runner.run");

    let recorded = changes(&app_state).await;
    assert!(
        recorded.iter().any(|c| matches!(
            c,
            UIChange::Removed { parent: Some(p), handle } if *p == root && *handle == children[1]
        )),
        "expected the last panel removed from the root, got {recorded:?}"
    );

    let state = app_state.lock().await;
    assert_eq!(state.children(root).expect("children").len(), 1);
}

/// I3 — an idle run records nothing, so the renderer is not woken for free.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn idle_run_records_nothing() {
    let (app_state, _ui_context, mut runner) = harness();
    {
        let mut state = app_state.lock().await;
        state
            .add_node(None, 0, ElementSource::None)
            .expect("add root node");
    }
    drain(&app_state).await;

    runner.run(&app_state).await.expect("runner.run");

    let mut state = app_state.lock().await;
    assert_eq!(state.take_invalidation(), Invalidation::None);
}

/// I4 — an element that reports `NeedsRepaint` from a snapshot is recorded as `Replaced`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_delivery_records_replaced() {
    let (app_state, ui_context, mut runner) = harness();
    let handle = {
        let mut state = app_state.lock().await;
        let h = state
            .add_node(None, 0, ElementSource::None)
            .expect("add root node");
        state
            .set_element(
                h,
                Box::new(QueryConsoleElement::new("Console".into(), String::new())),
            )
            .expect("set console");
        h
    };
    drain(&app_state).await;

    ui_context.send_message(liquers_lib::ui::AppMessage::RequestAssetUpdates {
        handle,
        query: "hello".to_string(),
    });
    runner.run(&app_state).await.expect("runner.run");

    let recorded = changes(&app_state).await;
    assert!(
        recorded
            .iter()
            .any(|c| matches!(c, UIChange::Replaced { handle: h } if *h == handle)),
        "a console that consumed a snapshot must be marked out of date, got {recorded:?}"
    );
}

/// I5 — an element that reports `Unchanged` is not recorded, so a quiet monitored asset does not
/// force re-renders.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unchanged_snapshot_records_nothing() {
    let (app_state, ui_context, mut runner) = harness();
    let handle = {
        let mut state = app_state.lock().await;
        let h = state
            .add_node(None, 0, ElementSource::None)
            .expect("add root node");
        state
            .set_element(h, Box::new(QuietElement { handle: None }))
            .expect("set element");
        h
    };
    drain(&app_state).await;

    ui_context.send_message(liquers_lib::ui::AppMessage::RequestAssetUpdates {
        handle,
        query: "hello".to_string(),
    });
    runner.run(&app_state).await.expect("runner.run");

    // Guard against a vacuous pass: monitoring only starts once a snapshot was delivered to the
    // element, so this proves the element really did see the update and stayed quiet.
    assert!(
        runner.has_monitoring(),
        "setup: the snapshot must have reached the element"
    );

    let mut state = app_state.lock().await;
    assert_eq!(
        state.take_invalidation(),
        Invalidation::None,
        "an element that reports Unchanged must not cause a re-render"
    );
}

/// I6 — auto-evaluation of a pending node records the elements it installs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_evaluation_records_changes() {
    let (app_state, _ui_context, mut runner) = harness();
    let handle = {
        let mut state = app_state.lock().await;
        state
            .add_node(None, 0, ElementSource::Query("hello".to_string()))
            .expect("add pending node")
    };
    drain(&app_state).await;

    // Phase 2 installs a progress element, Phase 3 replaces it with the result.
    let mut recorded = Vec::new();
    for _ in 0..200 {
        runner.run(&app_state).await.expect("runner.run");
        {
            let mut state = app_state.lock().await;
            match state.take_invalidation() {
                Invalidation::Changes(v) => recorded.extend(v),
                Invalidation::None => {}
                Invalidation::All => panic!("unexpected escalation to All"),
            }
            if let Ok(Some(elem)) = state.get_element(handle) {
                if elem.type_name() == "AssetViewElement" {
                    break;
                }
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    assert!(
        recorded
            .iter()
            .any(|c| matches!(c, UIChange::Replaced { handle: h } if *h == handle)),
        "installing an element on a pending node must be recorded, got {recorded:?}"
    );
}
