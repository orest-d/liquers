//! Editor elements bound to a recipe's query through the runner: two editors of one recipe, the
//! on-demand Update, an action editor, and a binding that is not an accessor.
//!
//! Design: `specs/design/query-pipeline-editor/` (Phase 3, integration tests).

use std::sync::Arc;
use std::time::Duration;

use liquers_core::assets::AssetManager;
use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::Error;
use liquers_core::metadata::MetadataRecord;
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::state::State;
use liquers_core::store::AsyncMemoryStore;
use liquers_core::value::ValueInterface;
use liquers_macro::register_command;

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::editor::{EditorBinding, EditorCommand, EditorElement, EditorKind, EditorTree, NodePath, UpdateMode};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::ui::{
    app_message_channel, AppMessageSender, AppRunner, AppState, DirectAppState, ElementSource, UIContext,
    UIElement, UIHandle,
};
use liquers_lib::value::Value;

// The `register_*_commands!` macros expand to code naming these unqualified.
#[allow(unused_imports)]
use liquers_core::{
    command_metadata::{ArgumentType, CommandDefinition, CommandParameterValue},
    context::Context,
};

type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

fn from_csv(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("table"))
}
fn is_in(_state: &State<Value>, _column: String, _values: Vec<String>) -> Result<Value, Error> {
    Ok(Value::from("filtered"))
}
fn head(_state: &State<Value>, n: i64) -> Result<Value, Error> {
    Ok(Value::from(format!("head {n}")))
}
fn shout(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(state.try_into_string()?.to_uppercase()))
}
fn quiet(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(state.try_into_string()?.to_lowercase()))
}

const QUERY: &str = "-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/head-10/top.csv";
const RECIPE_QUERY: &str = "-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query";
/// Recipe 1 reads a file of its own folder: `./data.txt` is `reports/data.txt`.
const LOUD: &str = "-R/./data.txt/-/shout/loud.txt";
const LOUD_RECIPE: &str = "-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-1/query";

fn key(text: &str) -> Key {
    parse_key(text).expect("key")
}

fn text_record() -> MetadataRecord {
    let mut record = MetadataRecord::new();
    record.type_identifier = "Text".to_string();
    record.type_name = "text".to_string();
    record.data_format = Some("txt".to_string());
    record
}

fn register(env: &mut CommandEnvironment) -> Result<(), Error> {
    let cr = env.get_mut_command_registry();
    liquers_lib::register_core_commands!(cr)?;
    liquers_lib::register_accessor_commands!(cr)?;
    register_command!(cr, fn from_csv(state) -> result namespace: "pl")?;
    register_command!(cr, fn is_in(state, column: String, values: Vec<String> multiple) -> result namespace: "pl")?;
    register_command!(cr, fn head(state, n: i64 = 10 (gui: IntegerField)) -> result namespace: "pl")?;
    register_command!(cr, fn shout(state) -> result)?;
    register_command!(cr, fn quiet(state) -> result)?;
    Ok(())
}

async fn environment() -> EnvRef<CommandEnvironment> {
    let mut env = CommandEnvironment::new();
    register(&mut env).expect("register");
    env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
    let envref = env.to_ref();
    let recipes = format!(
        "title: Reports\nrecipes:\n- query: {QUERY}\n  title: Top\n- query: {LOUD}\n  title: Loud\n"
    );
    envref
        .get_asset_manager()
        .set_binary(&key("reports/recipes.yaml"), recipes.as_bytes(), text_record())
        .await
        .expect("recipes.yaml");
    envref
        .get_asset_manager()
        .set_binary(&key("reports/data.txt"), b"Hello", text_record())
        .await
        .expect("data.txt");
    envref
}

struct Harness {
    envref: EnvRef<CommandEnvironment>,
    app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
    runner: AppRunner<CommandEnvironment>,
    sender: AppMessageSender,
}

impl Harness {
    async fn new(elements: Vec<EditorElement>) -> (Harness, Vec<UIHandle>) {
        Harness::of(elements.into_iter().map(|e| Box::new(e) as Box<dyn UIElement>).collect()).await
    }

    /// A harness over any elements; `with_element` and `tree` work on its editors only.
    async fn of(elements: Vec<Box<dyn UIElement>>) -> (Harness, Vec<UIHandle>) {
        let envref = environment().await;
        let mut direct = DirectAppState::new();
        let mut handles = Vec::new();
        for (i, element) in elements.into_iter().enumerate() {
            let handle = direct.add_node(None, i, ElementSource::None).expect("node");
            direct.set_element(handle, element).expect("element");
            handles.push(handle);
        }
        let app_state: Arc<tokio::sync::Mutex<dyn AppState>> = Arc::new(tokio::sync::Mutex::new(direct));
        let (tx, rx) = app_message_channel();
        let runner = AppRunner::new(envref.clone(), rx, tx.clone());
        let harness = Harness { envref, app_state, runner, sender: tx };
        for handle in handles.iter() {
            let ctx = harness.context(*handle);
            let mut state = harness.app_state.lock().await;
            let mut element = state.take_element(*handle).expect("take");
            element.init(*handle, &ctx).expect("init");
            state.put_element(*handle, element).expect("put");
        }
        (harness, handles)
    }

    fn context(&self, handle: UIHandle) -> UIContext {
        UIContext::new(self.app_state.clone(), self.sender.clone())
            .with_handle(Some(handle))
            .with_command_registry(self.runner.command_registry().clone())
    }

    async fn with_element<R>(&self, handle: UIHandle, f: impl FnOnce(&mut EditorElement, &UIContext) -> R) -> R {
        let ctx = self.context(handle);
        let mut state = self.app_state.lock().await;
        let element = state.take_element(handle).expect("take");
        let mut editor = element
            .as_any()
            .and_then(|a| a.downcast_ref::<EditorElement>())
            .cloned()
            .expect("an editor element");
        let result = f(&mut editor, &ctx);
        state.put_element(handle, Box::new(editor)).expect("put");
        result
    }

    async fn apply(&self, handle: UIHandle, command: EditorCommand) {
        self.with_element(handle, |e, ctx| e.apply_commands(vec![command], ctx)).await;
    }

    async fn settle(&mut self) {
        for _ in 0..30 {
            tokio::time::timeout(Duration::from_secs(10), self.runner.run(&self.app_state))
                .await
                .expect("run finishes")
                .expect("run");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    async fn tree(&self, handle: UIHandle) -> EditorTree {
        let state = self.app_state.lock().await;
        let element = state.get_element(handle).expect("get").expect("element");
        element
            .as_any()
            .and_then(|a| a.downcast_ref::<EditorElement>())
            .and_then(EditorElement::tree)
            .cloned()
            .expect("tree")
    }

    /// Adds and initialises one more element.
    async fn add(&self, element: Box<dyn UIElement>) -> UIHandle {
        let ctx = |handle| self.context(handle);
        let mut state = self.app_state.lock().await;
        let n = state.roots().len();
        let handle = state.add_node(None, n, ElementSource::None).expect("node");
        let mut element = element;
        element.init(handle, &ctx(handle)).expect("init");
        state.set_element(handle, element).expect("element");
        handle
    }

    /// Whether a console shows a preview.
    async fn console_previews(&self, handle: UIHandle) -> bool {
        let state = self.app_state.lock().await;
        let element = state.get_element(handle).expect("get").expect("element");
        element
            .as_any()
            .and_then(|a| a.downcast_ref::<liquers_lib::ui::QueryConsoleElement>())
            .expect("a console")
            .display()
            .is_preview()
    }

    /// The value an element shows, as text.
    async fn shown(&self, handle: UIHandle) -> Option<String> {
        let state = self.app_state.lock().await;
        let element = state.get_element(handle).expect("get").expect("element");
        element.get_value().and_then(|v| v.try_into_string().ok())
    }

    async fn recipes(&self) -> String {
        let bytes = self.envref.get_async_store().get_bytes(&key("reports/recipes.yaml")).await.expect("stored");
        String::from_utf8(bytes).expect("utf-8")
    }
}

/// An editor of a recipe in Apply immediately (the tests of writing); Live preview is tested on
/// its own.
fn recipe_editor(kind: EditorKind, query: &str) -> EditorElement {
    EditorElement::new("Editor", kind, EditorBinding::Accessor(query.to_string()))
        .with_update_mode(UpdateMode::ApplyImmediately)
}

fn append_apac() -> EditorCommand {
    EditorCommand::InsertItem { path: NodePath::element(2), arg: "values".into(), at: 2, text: "APAC".into() }
}

/// query-pipeline-editor AC-9, AC-26
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_editors_share_a_recipe_query() {
    let (mut h, handles) =
        Harness::new(vec![recipe_editor(EditorKind::Query, RECIPE_QUERY), recipe_editor(EditorKind::Query, RECIPE_QUERY)])
            .await;
    h.settle().await;
    assert_eq!(h.tree(handles[0]).await.text, QUERY);
    assert_eq!(h.tree(handles[1]).await.text, QUERY);

    h.apply(handles[0], append_apac()).await;
    h.settle().await;
    let expected = "-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US-APAC/head-10/top.csv";
    assert_eq!(h.tree(handles[1]).await.text, expected);
    assert!(h.recipes().await.contains("EU-US-APAC"), "{}", h.recipes().await);
    assert!(h.tree(handles[0]).await.message.is_none());
}

/// query-pipeline-editor AC-26, query-update-modes AC-8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn on_demand_editor_writes_only_on_update() {
    let (mut h, handles) = Harness::new(vec![
        recipe_editor(EditorKind::Query, RECIPE_QUERY).with_update_mode(UpdateMode::OnDemand),
        recipe_editor(EditorKind::Query, RECIPE_QUERY),
    ])
    .await;
    h.settle().await;
    h.apply(handles[0], append_apac()).await;
    h.settle().await;
    assert!(h.tree(handles[0]).await.pending);
    assert_eq!(h.tree(handles[1]).await.text, QUERY);
    assert!(!h.recipes().await.contains("APAC"));

    h.apply(handles[0], EditorCommand::Update).await;
    h.settle().await;
    assert!(!h.tree(handles[0]).await.pending);
    assert!(h.tree(handles[1]).await.text.contains("EU-US-APAC"));
    assert!(h.recipes().await.contains("EU-US-APAC"));
}

/// query-pipeline-editor AC-8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn action_editor_writes_through_the_action_accessor() {
    let action = format!("{RECIPE_QUERY}/action-3");
    let (mut h, handles) = Harness::new(vec![recipe_editor(EditorKind::Action, &action)]).await;
    h.settle().await;
    let tree = h.tree(handles[0]).await;
    assert!(tree.nodes.iter().any(|n| n.fields().iter().any(|f| f.name == "n")), "{:?}", tree.nodes);
    h.apply(
        handles[0],
        EditorCommand::SetField { path: NodePath::element(0), arg: "n".into(), text: "4".into() },
    )
    .await;
    h.settle().await;
    assert!(h.recipes().await.contains("head-4"), "{}", h.recipes().await);
}

/// query-pipeline-editor AC-9, AC-13
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binding_that_is_not_an_accessor_is_read_only() {
    let (mut h, handles) =
        Harness::new(vec![recipe_editor(EditorKind::Query, "-R-key/reports/recipes.yaml")]).await;
    h.settle().await;
    let tree = h.tree(handles[0]).await;
    assert!(tree.read_only);
    assert!(tree.message.is_some());
    h.apply(
        handles[0],
        EditorCommand::SetText("-R/x".into()),
    )
    .await;
    h.settle().await;
    assert!(h.tree(handles[0]).await.nodes.is_empty());
}

/// Codex review of PR #278 (query-editor-ergonomics AC-3): switching mode in the browser commits
/// the text typed into a field, instead of repainting it away.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_mode_switch_keeps_typed_field_text() {
    use liquers_lib::ui::editor::{field_id, EditorMode};
    use liquers_lib::ui::{UIEvent, UIEventKind, UpdateMessage};
    let (mut h, handles) = Harness::new(vec![recipe_editor(EditorKind::Query, RECIPE_QUERY)]).await;
    h.settle().await;
    let tree = h.tree(handles[0]).await;
    let head = tree.nodes[tree.last_action().expect("head")].path().clone();
    let event = UIEvent::new(UIEventKind::Named(format!("ed:mode:{}:advanced", head.id())))
        .with_field(field_id(&head, "n", None), Value::from("25".to_string()));
    let mode = h
        .with_element(handles[0], |e, ctx| {
            e.update(&UpdateMessage::UI(event), ctx);
            e.mode()
        })
        .await;
    assert_eq!(mode, EditorMode::Advanced);
    h.settle().await;
    assert!(h.tree(handles[0]).await.text.contains("head-25"), "{}", h.tree(handles[0]).await.text);
}

/// query-update-modes AC-18, checked first in Part B: a preview with the recipe's folder as its
/// working directory reads the recipe's relative keys as the recipe's own evaluation does, and
/// writes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn preview_in_the_recipe_folder_resolves_like_the_recipe() {
    use liquers_lib::ui::{AppMessage, QueryConsoleElement};
    let console = QueryConsoleElement::new("C".into(), String::new());
    let (mut h, handles) = Harness::of(vec![Box::new(console)]).await;
    let own = h.envref.evaluate(&liquers_core::parse::parse_query("-R/reports/loud.txt").expect("q")).await;
    let own = own.expect("the recipe").get().await.expect("value");
    assert_eq!(own.try_into_string().expect("text"), "HELLO");
    let before = h.recipes().await;
    h.sender
        .send(AppMessage::Preview {
            handle: handles[0],
            query: "-R/./data.txt/-/shout/quiet".into(),
            cwd: Some("reports".into()),
        })
        .expect("send");
    h.settle().await;
    assert_eq!(h.shown(handles[0]).await.as_deref(), Some("hello"));
    assert_eq!(h.recipes().await, before, "a preview writes nothing");
}

/// query-update-modes AC-17, AC-18, AC-19: a Live preview of a recipe is shown in the followed
/// console, framed, with `recipes.yaml` byte-identical; ✓ writes it, the preview ends and the
/// console shows its own query, recalculated from the new recipe.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_preview_of_a_recipe_leaves_recipes_yaml_unchanged() {
    use liquers_lib::ui::QueryConsoleElement;
    let console = QueryConsoleElement::new("C".into(), "-R/reports/loud.txt".into());
    let (mut h, handles) = Harness::of(vec![Box::new(console)]).await;
    let (console, editor_handle) = (handles[0], {
        let editor = EditorElement::new("E", EditorKind::Query, EditorBinding::Accessor(LOUD_RECIPE.into()))
            .with_follow(handles[0]);
        assert_eq!(editor.update_mode(), UpdateMode::LivePreview);
        h.add(Box::new(editor)).await
    });
    h.settle().await;
    assert_eq!(h.shown(console).await.as_deref(), Some("HELLO"));
    let before = h.recipes().await;

    h.apply(editor_handle, EditorCommand::SetText("-R/./data.txt/-/shout/quiet/loud.txt".into())).await;
    h.settle().await;
    assert_eq!(h.shown(console).await.as_deref(), Some("hello"), "the preview, in place");
    assert!(h.console_previews(console).await);
    assert_eq!(h.recipes().await, before, "nothing written");

    h.apply(editor_handle, EditorCommand::Update).await;
    h.settle().await;
    assert!(h.recipes().await.contains("shout/quiet"), "{}", h.recipes().await);
    assert!(!h.console_previews(console).await, "the preview ended");
    assert_eq!(h.shown(console).await.as_deref(), Some("hello"), "the recipe's own value, recalculated");
}

/// query-update-modes AC-9, AC-19: an editor in Apply immediately writes the recipe; a console
/// showing it gets `Expired` and recalculates.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn apply_expires_a_dependent_view() {
    use liquers_lib::ui::QueryConsoleElement;
    let console = QueryConsoleElement::new("C".into(), "-R/reports/loud.txt".into());
    let (mut h, handles) = Harness::of(vec![Box::new(console)]).await;
    let editor = h.add(Box::new(recipe_editor(EditorKind::Query, LOUD_RECIPE))).await;
    h.settle().await;
    assert_eq!(h.shown(handles[0]).await.as_deref(), Some("HELLO"));
    h.apply(editor, EditorCommand::SetText("-R/./data.txt/-/shout/quiet/loud.txt".into())).await;
    h.settle().await;
    assert!(h.recipes().await.contains("shout/quiet"));
    assert_eq!(h.shown(handles[0]).await.as_deref(), Some("hello"));
}

/// query-update-modes AC-17: Revert in Live preview reloads the working copy, and the followed
/// console shows the applied query again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revert_of_a_live_preview_restores_the_applied_value() {
    use liquers_lib::ui::QueryConsoleElement;
    let console = QueryConsoleElement::new("C".into(), "-R/reports/data.txt/-/shout".into());
    let (mut h, handles) = Harness::of(vec![Box::new(console)]).await;
    let editor = EditorElement::new("E", EditorKind::Query, EditorBinding::Memory("-R/reports/data.txt/-/shout".into()))
        .with_follow(handles[0]);
    let editor = h.add(Box::new(editor)).await;
    h.settle().await;
    assert_eq!(h.shown(handles[0]).await.as_deref(), Some("HELLO"));
    h.apply(editor, EditorCommand::SetText("-R/reports/data.txt/-/quiet".into())).await;
    h.settle().await;
    assert_eq!(h.shown(handles[0]).await.as_deref(), Some("hello"), "the preview");
    h.apply(editor, EditorCommand::Revert).await;
    h.settle().await;
    assert_eq!(h.shown(handles[0]).await.as_deref(), Some("HELLO"), "the applied value again");
    assert!(!h.console_previews(handles[0]).await);
    let text = h.with_element(editor, |e, _| e.tree().map(|t| t.text.clone())).await;
    assert_eq!(text.as_deref(), Some("-R/reports/data.txt/-/shout"), "the working copy reloaded");
}

/// query-update-modes AC-17: a standalone editor's preview that is dropped before it settles is
/// not shown when it does.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropped_standalone_preview_stays_dropped() {
    let editor = EditorElement::new("E", EditorKind::Query, EditorBinding::Memory("-R/reports/data.txt/-/shout".into()));
    let (mut h, handles) = Harness::new(vec![editor]).await;
    h.settle().await;
    // Edit and revert before the runner answers the preview.
    h.apply(handles[0], EditorCommand::SetText("-R/reports/data.txt/-/quiet".into())).await;
    h.apply(handles[0], EditorCommand::Revert).await;
    h.settle().await;
    let previewing = h.with_element(handles[0], |e, _| e.display().value().is_some()).await;
    assert!(!previewing, "the dropped preview is not shown");
}

/// query-update-modes AC-17: a pipeline editor's value is the pipeline's JSON; its preview is
/// evaluated as that pipeline.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn preview_of_a_pipeline_value() {
    use liquers_lib::ui::{AppMessage, QueryConsoleElement};
    let console = QueryConsoleElement::new("C".into(), String::new());
    let (mut h, handles) = Harness::of(vec![Box::new(console)]).await;
    let pipeline = liquers_core::pipeline::Pipeline::from_query_text("-R/reports/data.txt/-/shout").expect("pipeline");
    h.sender
        .send(AppMessage::Preview { handle: handles[0], query: pipeline.to_json().to_string(), cwd: None })
        .expect("send");
    h.settle().await;
    assert_eq!(h.shown(handles[0]).await.as_deref(), Some("HELLO"));
}
