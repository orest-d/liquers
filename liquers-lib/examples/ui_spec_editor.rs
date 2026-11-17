//! A UISpec app with editors bound to a recipe: a query editor and an action editor of
//! `reports/recipes.yaml` recipe 0, a key editor, and a query console showing the recipe's result.
//!
//! Edit an argument (the `times` slider of `repeat`, say) in either editor: the recipe in
//! `recipes.yaml` is rewritten, the other editor follows, and the console re-evaluates the result.
//! Untick "Update immediately" to collect edits and write them with Update.
//!
//! Run with: `cargo run --example ui_spec_editor -p liquers-lib`
//! Without a window (ticks the model and prints its elements):
//! `cargo run --example ui_spec_editor -p liquers-lib -- --check`

use std::sync::Arc;

use liquers_core::assets::AssetManager;
use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::metadata::MetadataRecord;
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::state::State;
use liquers_core::store::AsyncMemoryStore;
use liquers_lib::egui::app::{run_window, WindowCloser, WindowOptions};
use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::ui::widgets::ui_spec_element::{UISpec, UISpecElement};
use liquers_core::value::ValueInterface;
use liquers_lib::ui::editor::{EditorCommand, EditorElement};
use liquers_lib::ui::{
    app_message_channel, AppState, DirectAppState, ElementSource, NativeDriver, UIContext, UIElement,
    UIHandle,
};
use liquers_lib::value::Value;
use liquers_macro::register_command;

// The `register_*_commands!` macros expand to code naming these unqualified.
#[allow(unused_imports)]
use liquers_core::command_metadata::{ArgumentType, CommandDefinition, CommandParameterValue};

type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

// ─── Commands ────────────────────────────────────────────────────────────────

fn upper(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(state.try_into_string()?.to_uppercase()))
}

fn repeat(state: &State<Value>, times: i64, separator: String) -> Result<Value, Error> {
    let text = state.try_into_string()?;
    let count = usize::try_from(times.max(0)).unwrap_or(0);
    Ok(Value::from(vec![text; count].join(&separator)))
}

fn wrap(state: &State<Value>, left: String, right: String) -> Result<Value, Error> {
    Ok(Value::from(format!("{left}{}{right}", state.try_into_string()?)))
}

fn register(env: &mut CommandEnvironment) -> Result<(), Error> {
    let cr = env.get_mut_command_registry();
    liquers_lib::register_core_commands!(cr)?;
    liquers_lib::register_accessor_commands!(cr)?;
    liquers_lib::register_lui_commands!(cr)?;
    register_command!(cr,
        fn upper(state) -> result
        namespace: "demo"
        label: "Upper case"
        doc: "The text in upper case"
        next: "repeat-3" (label: "Repeat three times")
    )?;
    register_command!(cr,
        fn repeat(state,
            times: i64 = 2 (label: "Times", description: "How many copies", gui: IntegerSlider(1, 10, 1)),
            separator: String = " " (label: "Separator", description: "Put between the copies"))
            -> result
        namespace: "demo"
        label: "Repeat"
        doc: "The text repeated, joined by a separator"
        preset: "repeat-5" (label: "Five times", description: "Repeat five times")
        filename: "repeated.txt"
    )?;
    register_command!(cr,
        fn wrap(state,
            left: String = "[" (label: "Left", description: "Text before"),
            right: String = "]" (label: "Right", description: "Text after"))
            -> result
        namespace: "demo"
        label: "Wrap"
        doc: "The text between two markers"
    )?;
    Ok(())
}

// ─── Data and UI specification ───────────────────────────────────────────────

const RECIPES_YAML: &str = "title: Reports
recipes:
- query: -R/data/story.txt/-/ns-demo/upper/repeat-2/result.txt
  title: Result
";

const SPEC_YAML: &str = r#"
init:
- "-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query/ns-lui/query_editor/q/add-child"
- "-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query/action-2/ns-lui/action_editor/q/add-child"
- "-R/reports/result.txt/-/q/ns-lui/query_console/q/add-child"
menu:
  items:
  - !button
    label: Add key editor
    action:
      query: "-R-key/data/story.txt/-/ns-lui/key_editor/q/add-child"
  - !button
    label: Add another query editor
    action:
      query: "-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query/ns-lui/query_editor/q/add-child"
layout: vertical
"#;

fn text_record() -> MetadataRecord {
    let mut record = MetadataRecord::new();
    record.type_identifier = "Text".to_string();
    record.type_name = "text".to_string();
    record.data_format = Some("txt".to_string());
    record
}

async fn driver() -> Result<NativeDriver<CommandEnvironment>, Error> {
    let mut env = CommandEnvironment::new();
    register(&mut env)?;
    env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
    let envref = env.to_ref();
    let manager = envref.get_asset_manager();
    manager
        .set_binary(&parse_key("data/story.txt")?, b"Once upon a time", text_record())
        .await?;
    manager
        .set_binary(&parse_key("reports/recipes.yaml")?, RECIPES_YAML.as_bytes(), text_record())
        .await?;

    let mut app_state = DirectAppState::new();
    let root = app_state.add_node(None, 0, ElementSource::None)?;
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> = Arc::new(tokio::sync::Mutex::new(app_state));
    let (sender, receiver) = app_message_channel();

    // The root's init() submits the spec's init queries, which add the editors and the console.
    let mut element = UISpecElement::from_spec("Recipe editors".to_string(), UISpec::from_yaml(SPEC_YAML)?);
    let ctx = UIContext::new(app_state.clone(), sender.clone()).with_handle(Some(root));
    element.init(root, &ctx)?;
    app_state.lock().await.set_element(root, Box::new(element))?;
    Ok(NativeDriver::new(envref, app_state, receiver, sender))
}

async fn settle(driver: &mut NativeDriver<CommandEnvironment>) {
    for _ in 0..50 {
        driver.tick().await;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

/// Prints each element: its type, and an editor's text or a console's value.
async fn show(driver: &NativeDriver<CommandEnvironment>) -> Result<Vec<UIHandle>, Error> {
    let state = driver.app_state().lock().await;
    let mut handles = Vec::new();
    for root in state.roots() {
        for handle in std::iter::once(root).chain(state.children(root)?) {
            handles.push(handle);
            let Some(element) = state.get_element(handle)? else {
                println!("{handle:?}: (pending)");
                continue;
            };
            let detail = match element.as_any().and_then(|a| a.downcast_ref::<EditorElement>()) {
                Some(editor) => editor.tree().map(|t| t.text.clone()).unwrap_or_default(),
                None => element
                    .get_value()
                    .and_then(|v| v.try_into_string().ok())
                    .unwrap_or_default(),
            };
            println!("{handle:?}: {} {detail}", element.type_name());
        }
    }
    Ok(handles)
}

/// Without a window: ticks the model, shows it, sets `times` to 3 in the action editor and shows
/// the result again.
async fn check(driver: &mut NativeDriver<CommandEnvironment>) -> Result<(), Error> {
    settle(driver).await;
    let handles = show(driver).await?;
    let action_editor = handles.get(2).copied().ok_or_else(|| Error::general_error("no action editor".into()))?;
    {
        let ctx = driver.ui_context().with_handle(Some(action_editor));
        let mut state = driver.app_state().lock().await;
        let element = state.take_element(action_editor)?;
        let mut editor = element
            .as_any()
            .and_then(|a| a.downcast_ref::<EditorElement>())
            .cloned()
            .ok_or_else(|| Error::general_error("not an editor".into()))?;
        let path = editor
            .tree()
            .and_then(|t| t.nodes.iter().find(|n| !n.fields().is_empty()).map(|n| n.path().clone()))
            .ok_or_else(|| Error::general_error("no action node".into()))?;
        editor.apply_commands(
            vec![EditorCommand::SetField { path, arg: "times".into(), text: "3".into() }],
            &ctx,
        );
        state.put_element(action_editor, Box::new(editor))?;
    }
    settle(driver).await;
    println!("after setting times to 3:");
    show(driver).await?;
    Ok(())
}

fn main() -> Result<(), Error> {
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| Error::general_error(format!("cannot start a Tokio runtime: {e}")))?;
    let mut driver = runtime.block_on(driver())?;
    if std::env::args().any(|a| a == "--check") {
        return runtime.block_on(check(&mut driver));
    }
    let mut options = WindowOptions::new("Recipe editors");
    options.inner_size = [900.0, 800.0];
    run_window(options, &mut driver, runtime.handle(), &WindowCloser::default(), None)
}
