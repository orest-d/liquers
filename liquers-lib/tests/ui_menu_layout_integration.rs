//! Integration tests for the menu/pane-layout feature (`specs/menu-pane-layout/`).
//!
//! `ui_spec_integration.rs` covers YAML parsing and element construction in isolation; this file
//! covers the *runtime* half that the Phase 3 test plan specified but never got written:
//!
//! - the `lui/ui_spec` command turning YAML into a live `UISpecElement` inside `AppState`,
//! - `init` queries creating children (and their order surviving into the layout),
//! - menu actions parsed from YAML dispatching through `UIContext` into `AppRunner`,
//! - the web rendering of menu + layout (only with the `webui` feature).
//!
//! Everything is headless: no egui context is created, so the same assertions hold for every
//! backend.

use std::sync::Arc;

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_macro::register_command;

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::ui::widgets::ui_spec_element::{MenuItem, TopLevelItem, UISpec};
use liquers_lib::ui::{
    app_message_channel, dispatch_action, AppMessage, AppRunner, AppState, DirectAppState,
    ElementSource, UIContext, UIHandle,
};
use liquers_lib::value::Value;

// Required by register_command! and register_lui_commands! macros.
type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

// ─── Fixtures ────────────────────────────────────────────────────────────────

/// Dashboard spec: a menu with a query action and a quit action, a grid layout, and two init
/// queries that create children as soon as the element is initialised.
const DASHBOARD_YAML: &str = r#"
menu:
  items:
  - !menu
    label: File
    items:
    - !button
      label: Add Panel
      shortcut: Ctrl+N
      action:
        query: "hello/ns-lui/add-child"
    - !separator null
    - !button
      label: Quit
      shortcut: Ctrl+Q
      action: quit
layout: !grid
  rows: 2
  columns: 2
init:
- "hello/ns-lui/add-child"
- "hello/ns-lui/add-child"
"#;

/// A command returning the dashboard YAML, so `lui/ui_spec` can be driven by a query.
fn dashboard_spec(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(DASHBOARD_YAML))
}

/// A command returning a greeting string — the payload of every child panel.
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
    register_command!(cr, fn dashboard_spec(state) -> result)?;
    liquers_lib::register_lui_commands!(cr)?;
    Ok(())
}

/// Empty AppState + channel + runner, the standard harness of the ui integration tests.
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

/// The first (and only) root handle.
async fn root_of(app_state: &Arc<tokio::sync::Mutex<dyn AppState>>) -> UIHandle {
    let state = app_state.lock().await;
    let roots = state.roots();
    assert_eq!(roots.len(), 1, "expected exactly one root");
    roots[0]
}

// ─── 1. `lui/ui_spec` command execution ──────────────────────────────────────

/// The `lui/ui_spec` command turns a YAML string into a `UISpecElement` that `add-child`
/// installs as a root — the whole "YAML → live element" path in one query.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ui_spec_command_creates_element_from_yaml() {
    let (app_state, ui_context, mut runner) = harness();

    ui_context.submit_root_query("dashboard_spec/ns-lui/ui_spec/add-child");
    runner.run(&app_state).await.expect("runner.run");

    let root = root_of(&app_state).await;
    let state = app_state.lock().await;
    let elem = state
        .get_element(root)
        .expect("get element")
        .expect("root element present");
    assert_eq!(elem.type_name(), "UISpecElement");
    assert_eq!(
        elem.handle(),
        Some(root),
        "init must run, which is what submits the init queries"
    );

    // The parsed spec (menu + grid layout) is carried by the element, not just by the YAML:
    // serialising it back proves the layout and the menu survived the command.
    let json = serde_json::to_string(elem).expect("serialize element");
    assert!(json.contains("grid"), "layout not preserved: {json}");
    assert!(json.contains("Add Panel"), "menu not preserved: {json}");
}

/// A YAML spec that does not parse must surface as an error element rather than a panic.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ui_spec_command_reports_invalid_yaml() {
    fn broken_spec(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("invalid: { yaml: [syntax"))
    }

    // `register_lui_commands!` expands to `?`, so it needs a `Result`-returning scope.
    fn register(env: &mut DefaultEnvironment<Value, SimpleUIPayload>) -> Result<(), Error> {
        let cr = env.get_mut_command_registry();
        register_command!(cr, fn broken_spec(state) -> result)?;
        liquers_lib::register_lui_commands!(cr)?;
        Ok(())
    }

    let mut env = DefaultEnvironment::<Value, SimpleUIPayload>::new();
    env.with_trivial_recipe_provider();
    register(&mut env).expect("register commands");
    let envref = env.to_ref();

    let mut direct_state = DirectAppState::new();
    let root = direct_state
        .add_node(None, 0, ElementSource::None)
        .expect("add root node");
    direct_state
        .set_element(root, Box::new(liquers_lib::ui::Placeholder::new()))
        .expect("set placeholder");
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(direct_state));
    let (msg_tx, msg_rx) = app_message_channel();
    let ui_context = UIContext::new(app_state.clone(), msg_tx.clone());
    let mut runner = AppRunner::new(envref, msg_rx, msg_tx);

    ui_context.submit_query(root, "broken_spec/ns-lui/ui_spec/add-instead");
    runner.run(&app_state).await.expect("runner.run");

    let state = app_state.lock().await;
    let elem = state
        .get_element(root)
        .expect("get element")
        .expect("element present after failed spec");
    assert_eq!(
        elem.type_name(),
        "AssetViewElement",
        "a broken spec must leave an error element, not a UISpecElement"
    );
}

// ─── 2. Init queries create children ─────────────────────────────────────────

/// `UISpecElement::init` submits the spec's init queries; each `add-child` appends one child to
/// the spec element. The two init queries of DASHBOARD_YAML therefore yield two children.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn init_queries_create_children() {
    let (app_state, ui_context, mut runner) = harness();

    ui_context.submit_root_query("dashboard_spec/ns-lui/ui_spec/add-child");
    runner.run(&app_state).await.expect("runner.run");
    let root = root_of(&app_state).await;

    let created = run_until(&mut runner, &app_state, 100, |state| {
        state.children(root).map(|c| c.len()) == Ok(2)
    })
    .await;
    assert!(created, "both init queries should create a child");

    let state = app_state.lock().await;
    let children = state.children(root).expect("children");
    for child in &children {
        let elem = state
            .get_element(*child)
            .expect("get child")
            .expect("child element present");
        assert_eq!(elem.type_name(), "StateViewElement");
    }
    assert_eq!(state.parent(children[0]).expect("parent"), Some(root));
}

/// Layout indexing (grid cells, tab selection) depends on child order, so insertion order must be
/// the order `children()` reports.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn children_keep_insertion_order_for_layout() {
    let (app_state, ui_context, mut runner) = harness();

    ui_context.submit_root_query("dashboard_spec/ns-lui/ui_spec/add-child");
    runner.run(&app_state).await.expect("runner.run");
    let root = root_of(&app_state).await;
    run_until(&mut runner, &app_state, 100, |state| {
        state.children(root).map(|c| c.len()) == Ok(2)
    })
    .await;

    // A third panel, added later by a menu action, must come last.
    ui_context.submit_query(root, "hello/ns-lui/add-child");
    let three = run_until(&mut runner, &app_state, 100, |state| {
        state.children(root).map(|c| c.len()) == Ok(3)
    })
    .await;
    assert!(three, "menu-added panel should become the third child");

    let state = app_state.lock().await;
    let children = state.children(root).expect("children");
    let mut sorted = children.clone();
    sorted.sort_by_key(|h| h.0);
    assert_eq!(
        children, sorted,
        "children must stay in insertion order for grid/tab indexing"
    );
}

// ─── 3. Menu actions ─────────────────────────────────────────────────────────

/// Pull the action out of the parsed YAML (rather than hand-building one) and dispatch it the way
/// a menu click does: the query must reach `AppRunner` and create a child.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn menu_action_from_yaml_submits_query() {
    let spec = UISpec::from_yaml(DASHBOARD_YAML).expect("parse dashboard yaml");
    let menu = spec.menu.expect("menu present");
    let action = match &menu.items[0] {
        TopLevelItem::Menu { items, .. } => match &items[0] {
            MenuItem::Button { label, action, .. } => {
                assert_eq!(label, "Add Panel");
                action.clone()
            }
            MenuItem::Submenu { .. } | MenuItem::Separator => panic!("expected a button"),
        },
        TopLevelItem::Button { .. } => panic!("expected a menu"),
    };

    let (app_state, ui_context, mut runner) = harness();
    let root = {
        let mut state = app_state.lock().await;
        state
            .add_node(None, 0, ElementSource::None)
            .expect("add root node")
    };

    // A menu click dispatches the action with the owning element's handle.
    dispatch_action(&action, &ui_context, Some(root));

    let created = run_until(&mut runner, &app_state, 100, |state| {
        state.children(root).map(|c| c.len()) == Ok(1)
    })
    .await;
    assert!(created, "the menu action query should create a child");
}

/// The `quit` menu action produces `AppMessage::Quit` — dispatch must not confuse it with a query.
#[test]
fn menu_quit_action_sends_quit_message() {
    let spec = UISpec::from_yaml(DASHBOARD_YAML).expect("parse dashboard yaml");
    let menu = spec.menu.expect("menu present");
    let action = match &menu.items[0] {
        TopLevelItem::Menu { items, .. } => match &items[2] {
            MenuItem::Button { label, action, .. } => {
                assert_eq!(label, "Quit");
                action.clone()
            }
            MenuItem::Submenu { .. } | MenuItem::Separator => panic!("expected a button"),
        },
        TopLevelItem::Button { .. } => panic!("expected a menu"),
    };

    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
    let (msg_tx, mut msg_rx) = app_message_channel();
    let ui_context = UIContext::new(app_state, msg_tx);

    dispatch_action(&action, &ui_context, Some(UIHandle(1)));

    match msg_rx.try_recv() {
        Ok(AppMessage::Quit) => {}
        other => panic!("expected Quit, got {other:?}"),
    }
}

/// Every shortcut in the dashboard menu is unique, so the spec is accepted without warnings.
#[test]
fn dashboard_shortcuts_have_no_conflicts() {
    let spec = UISpec::from_yaml(DASHBOARD_YAML).expect("parse dashboard yaml");
    let menu = spec.menu.expect("menu present");
    assert!(
        menu.validate_shortcuts().is_empty(),
        "Ctrl+N and Ctrl+Q must not conflict"
    );
}

// ─── 4. Web rendering of menu + layout ───────────────────────────────────────

/// The web backend renders the menu bar (with dispatchable actions) and tags the container with
/// the layout class, so CSS can arrange the children.
#[cfg(feature = "webui")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn menu_and_layout_render_to_html() {
    let (app_state, ui_context, mut runner) = harness();

    ui_context.submit_root_query("dashboard_spec/ns-lui/ui_spec/add-child");
    runner.run(&app_state).await.expect("runner.run");
    let root = root_of(&app_state).await;
    run_until(&mut runner, &app_state, 100, |state| {
        state.children(root).map(|c| c.len()) == Ok(2)
    })
    .await;

    let html = liquers_lib::ui::render_app_ssr(&app_state)
        .await
        .expect("render ssr");

    assert!(html.contains("lq-UISpecElement"), "html: {html}");
    assert!(html.contains("lq-layout-grid"), "grid layout class missing: {html}");
    assert!(
        html.contains(&format!("data-lq-children=\"{}\"", root.0)),
        "the layout wrapper must declare itself as the child container, carrying its own \
         handle so the lookup cannot match a descendant's container: {html}"
    );
    assert!(html.contains("lq-menubar"), "menu bar missing: {html}");
    assert!(html.contains("Add Panel"), "menu item missing: {html}");
    assert!(
        html.contains("data-lq-action"),
        "menu action not dispatchable: {html}"
    );
    assert!(
        html.contains("Hello from test!"),
        "children not rendered inside the layout: {html}"
    );
}
