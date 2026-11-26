//! The recipe editors through the runner: the panel writes `recipes.yaml`, a recipe form writes
//! one recipe by filename, a rename moves stored data, and Save as writes a local file.
//!
//! Design: `specs/design/recipes-editor/` (Phase 3, integration tests).

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
use liquers_lib::ui::editor::EditorBinding;
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::ui::recipes::{FormCommand, RecipeElement, RecipesCommand, RecipesElement};
use liquers_lib::ui::{
    app_message_channel, AppMessageSender, AppRunner, AppState, DirectAppState, ElementSource, UIContext, UIElement,
    UIHandle,
};
use liquers_lib::value::Value;

// The `register_*_commands!` macros expand to code naming these unqualified.
#[allow(unused_imports)]
use liquers_core::{
    command_metadata::{ArgumentType, CommandDefinition, CommandParameterValue},
    context::Context,
};

type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

fn shout(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(format!("{}!", state.try_into_string()?)))
}

/// The keys `clicked` was run on (design `recipes-editor-open-action`).
static CLICKED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// An open command: records the key it gets as its input.
fn clicked(state: &State<Value>) -> Result<Value, Error> {
    let key = state.data_unchecked().try_into_key().map(|k| k.encode()).unwrap_or_else(|_| "?".to_owned());
    CLICKED.lock().expect("lock").push(key);
    Ok(Value::from("clicked"))
}

const PANEL: &str = "-R-key/reports/recipes.yaml/-/ns-acc/accessor";
const SCENES: &str = "-R-key/scenes/recipes.yaml/-/ns-acc/accessor";

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
    register_command!(cr, fn shout(state) -> result)?;
    register_command!(cr, fn clicked(state) -> result namespace: "test")?;
    liquers_lib::register_lui_commands!(cr)?;
    Ok(())
}

async fn environment() -> EnvRef<CommandEnvironment> {
    let mut env = CommandEnvironment::new();
    register(&mut env).expect("register");
    env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
    let envref = env.to_ref();
    let manager = envref.get_asset_manager();
    let files = [
        ("data/hello.txt", "hello".to_owned()),
        (
            "reports/recipes.yaml",
            "recipes:\n- query: -R/data/hello.txt/-/shout/loud.txt\n  title: Loud\n".to_owned(),
        ),
        ("scenes/tyre.txt", "round".to_owned()),
        (
            "scenes/recipes.yaml",
            "tree: {}\nrecipes:\n- {query: tyre.txt, title: Tyre}\n- {query: \"-R/./tyre.txt/-/shout/wheel.txt\", title: Wheel}\n"
                .to_owned(),
        ),
    ];
    for (k, text) in files {
        manager.set_binary(&key(k), text.as_bytes(), text_record()).await.expect(k);
    }
    envref
}

struct Harness {
    envref: EnvRef<CommandEnvironment>,
    app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
    runner: AppRunner<CommandEnvironment>,
    sender: AppMessageSender,
    handle: UIHandle,
}

impl Harness {
    async fn new(element: Box<dyn UIElement>) -> Harness {
        let envref = environment().await;
        let mut direct = DirectAppState::new();
        let handle = direct.add_node(None, 0, ElementSource::None).expect("node");
        direct.set_element(handle, element).expect("element");
        let app_state: Arc<tokio::sync::Mutex<dyn AppState>> = Arc::new(tokio::sync::Mutex::new(direct));
        let (tx, rx) = app_message_channel();
        let runner = AppRunner::new(envref.clone(), rx, tx.clone());
        let mut harness = Harness { envref, app_state, runner, sender: tx, handle };
        harness.with(|e, ctx| e.init(handle, ctx).expect("init")).await;
        harness.settle().await;
        harness
    }

    fn context(&self) -> UIContext {
        UIContext::new(self.app_state.clone(), self.sender.clone())
            .with_handle(Some(self.handle))
            .with_command_registry(self.runner.command_registry().clone())
    }

    async fn with<R>(&mut self, f: impl FnOnce(&mut Box<dyn UIElement>, &UIContext) -> R) -> R {
        let ctx = self.context();
        let mut state = self.app_state.lock().await;
        let mut element = state.take_element(self.handle).expect("take");
        let result = f(&mut element, &ctx);
        state.put_element(self.handle, element).expect("put");
        result
    }

    async fn panel(&mut self, commands: Vec<RecipesCommand>) {
        self.with(|e, ctx| {
            let mut panel = e.as_any().and_then(|a| a.downcast_ref::<RecipesElement>()).cloned().expect("panel");
            panel.apply_commands(commands, ctx);
            *e = Box::new(panel);
        })
        .await;
    }

    async fn panel_state<R>(&mut self, f: impl FnOnce(&RecipesElement) -> R) -> R {
        self.with(|e, _| f(e.as_any().and_then(|a| a.downcast_ref::<RecipesElement>()).expect("panel"))).await
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

    async fn text(&self, k: &str) -> Option<String> {
        let bytes = self.envref.get_async_store().get_bytes(&key(k)).await.ok()?;
        Some(String::from_utf8(bytes).expect("utf-8"))
    }
}

fn panel(address: &str) -> Box<dyn UIElement> {
    Box::new(RecipesElement::new("Recipes", EditorBinding::Accessor(address.to_owned())))
}

/// AC-1, AC-7, AC-13: the panel reads the file, and a new recipe is written to it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recipes_editor_writes_through_the_runner() {
    let mut h = Harness::new(panel(PANEL)).await;
    let names = h
        .panel_state(|p| p.state().expect("state").rows().iter().map(|r| r.name.clone()).collect::<Vec<_>>())
        .await;
    assert_eq!(names, vec!["loud.txt"]);
    h.panel(vec![RecipesCommand::New { title: "Quiet note".into(), extension: "txt".into() }]).await;
    h.settle().await;
    let stored = h.text("reports/recipes.yaml").await.expect("stored");
    let message = h.panel_state(|p| p.state().and_then(|s| s.message().map(str::to_owned))).await;
    assert!(stored.contains("Quiet note"), "{stored} {message:?}");
    assert!(stored.contains("Loud"), "{stored}");
    let message = h.panel_state(|p| p.state().and_then(|s| s.message().map(str::to_owned))).await;
    assert!(message.is_none(), "{message:?}");
}

/// AC-5, AC-6: a recipe form edits one recipe, found by its filename.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recipe_editor_edits_one_recipe_by_filename() {
    let address = format!("{PANEL}/recipe_file-loud.txt");
    let element = RecipeElement::new("Recipe", EditorBinding::Accessor(address));
    let mut h = Harness::new(Box::new(element)).await;
    h.with(|e, ctx| {
        let mut form = e.as_any().and_then(|a| a.downcast_ref::<RecipeElement>()).cloned().expect("form");
        assert_eq!(form.form().expect("read").recipe().title, "Loud");
        form.apply_commands(vec![FormCommand::SetTitle("Very loud".into())], ctx);
        *e = Box::new(form);
    })
    .await;
    h.settle().await;
    let stored = h.text("reports/recipes.yaml").await.expect("stored");
    assert!(stored.contains("Very loud"), "{stored}");
}

/// AC-8: renaming a node with stored data moves the data and rewrites the references.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_with_stored_data_through_the_runner() {
    let mut h = Harness::new(panel(SCENES)).await;
    h.panel(vec![RecipesCommand::Rename { from: "tyre".into(), to: "tire".into() }]).await;
    h.settle().await;
    let stored = h.text("scenes/recipes.yaml").await.expect("stored");
    assert!(stored.contains("tire.txt") && !stored.contains("tyre.txt"), "{stored}");
    assert_eq!(h.text("scenes/tire.txt").await.as_deref(), Some("round"));
    assert!(h.text("scenes/tyre.txt").await.is_none());
    let names = h
        .panel_state(|p| p.state().expect("state").rows().iter().map(|r| r.name.clone()).collect::<Vec<_>>())
        .await;
    assert_eq!(names, vec!["tire", "wheel"]);
}

/// AC-19: Save as evaluates the file and writes it to this computer, not to the store.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn save_as_writes_a_file() {
    let mut h = Harness::new(panel(PANEL)).await;
    let dir = std::env::temp_dir().join(format!("lq-save-as-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("loud.txt");
    h.panel(vec![RecipesCommand::SaveAs {
        filename: "loud.txt".into(),
        format: None,
        path: path.to_string_lossy().into_owned(),
    }])
    .await;
    h.settle().await;
    assert_eq!(std::fs::read_to_string(&path).expect("written"), "hello!");
    let message = h.panel_state(|p| p.state().and_then(|s| s.message().map(str::to_owned))).await;
    assert!(message.expect("notice").starts_with("Saved"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// `recipes-editor-open-action` AC-2, AC-6: `lui/recipes_editor-<ns>-<command>` keeps the open
/// command; an unknown command or half of one is refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recipes_editor_takes_an_open_command() {
    use liquers_lib::value::ExtValueInterface;
    let envref = environment().await;
    let state = envref
        .evaluate(&format!("{PANEL}/ns-lui/recipes_editor-test-clicked"))
        .await
        .expect("evaluate")
        .get()
        .await
        .expect("panel");
    let element = state.value().expect("value").as_ui_element().expect("element").clone_boxed();
    let panel = element.as_any().and_then(|a| a.downcast_ref::<RecipesElement>()).expect("panel");
    assert_eq!(panel.open_command(), Some(&liquers_lib::ui::recipes::OpenCommand::new("test", "clicked")));
    for query in ["ns-lui/recipes_editor-test-missing", "ns-lui/recipes_editor-test"] {
        let result = match envref.evaluate(&format!("{PANEL}/{query}")).await {
            Ok(asset) => asset.get().await.and_then(|s| s.value().map(|_| ())),
            Err(e) => Err(e),
        };
        assert!(result.is_err(), "{query} should be refused");
    }
}

/// `recipes-editor-open-action` AC-7: through the runner, a click runs the open command on the
/// node's key and opens no recipe window.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn click_runs_the_open_command_through_the_runner() {
    let element = RecipesElement::new("Recipes", EditorBinding::Accessor(PANEL.to_owned())).with_open_command("test", "clicked");
    let mut h = Harness::new(Box::new(element)).await;
    h.panel(vec![RecipesCommand::Open("loud.txt".into())]).await;
    h.settle().await;
    let clicked = CLICKED.lock().expect("lock").clone();
    assert!(clicked.iter().any(|k| k.contains("reports/loud.txt")), "{clicked:?}");
    let forms = h.panel_state(|p| p.state().expect("state").forms().len()).await;
    assert_eq!(forms, 0);
}

