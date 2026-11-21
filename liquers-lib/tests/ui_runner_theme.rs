//! The runner loads a theme given as a store key, reloads it when the key changes and keeps the
//! previous theme when an edit is not a theme; it loads icon images by key.
//!
//! Design: `specs/design/query-editor-ergonomics/` (Phase 3, tests 23 and 26, AC-16, AC-17).

use std::sync::Arc;
use std::time::Duration;

use liquers_core::assets::AssetManager;
use liquers_core::context::{EnvRef, Environment};
use liquers_core::metadata::MetadataRecord;
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::store::AsyncMemoryStore;

use liquers_lib::environment::DefaultEnvironment;
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::ui::theme::{Rgba, ThemeCell, ThemeMode, UiConfig};
use liquers_lib::ui::{app_message_channel, AppMessage, AppRunner, AppState, DirectAppState};
use liquers_lib::value::Value;

type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

fn text_record() -> MetadataRecord {
    let mut record = MetadataRecord::new();
    record.type_identifier = "Text".to_string();
    record.type_name = "text".to_string();
    record.data_format = Some("txt".to_string());
    record
}

async fn put(envref: &EnvRef<CommandEnvironment>, key: &str, text: &str) {
    envref
        .get_asset_manager()
        .set_binary(&parse_key(key).expect("key"), text.as_bytes(), text_record())
        .await
        .expect("set");
}

async fn environment() -> EnvRef<CommandEnvironment> {
    let mut env = CommandEnvironment::new();
    env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
    env.to_ref()
}

async fn settle(
    runner: &mut AppRunner<CommandEnvironment>,
    app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
) {
    for _ in 0..10 {
        tokio::time::timeout(Duration::from_secs(10), runner.run(app_state))
            .await
            .expect("run in time")
            .expect("run");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

fn card(cell: &ThemeCell) -> Rgba {
    cell.get().theme.palette(ThemeMode::Dark).surface.card
}

// Test 23 (AC-16).
#[tokio::test]
async fn theme_from_store_key_reloads() {
    let envref = environment().await;
    put(&envref, "config/theme.yaml", "dark: { surface: { card: \"#101010\" } }\n").await;

    let ui: UiConfig = serde_yaml::from_str("theme: -R/config/theme.yaml").expect("ui");
    let cell = ui.cell(true);
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
    let (tx, rx) = app_message_channel();
    let mut runner = AppRunner::new(envref.clone(), rx, tx.clone()).with_ui_config(&ui, cell.clone());
    assert_eq!(card(&cell), liquers_lib::ui::theme::Theme::builtin().dark.surface.card);

    settle(&mut runner, &app_state).await;
    assert_eq!(card(&cell), Rgba::rgb(0x10, 0x10, 0x10));
    assert!(!runner.needs_repaint(), "watching the theme does not keep the UI repainting");

    put(&envref, "config/theme.yaml", "dark: { surface: { card: \"#202020\" } }\n").await;
    settle(&mut runner, &app_state).await;
    assert_eq!(card(&cell), Rgba::rgb(0x20, 0x20, 0x20));

    put(&envref, "config/theme.yaml", "dark: { surface: { card: red } }\n").await;
    settle(&mut runner, &app_state).await;
    assert_eq!(card(&cell), Rgba::rgb(0x20, 0x20, 0x20), "a bad edit keeps the previous theme");
}

// The runner half of test 26 (AC-17): an icon image by key reaches the cache.
#[tokio::test]
async fn icon_by_key_is_loaded_into_the_cache() {
    let envref = environment().await;
    put(&envref, "icons/brush.svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"/>").await;
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
    let (tx, rx) = app_message_channel();
    let mut runner = AppRunner::new(envref, rx, tx.clone());
    tx.send(AppMessage::LoadIcon { key: "-R/icons/brush.svg".to_string() }).expect("send");
    tx.send(AppMessage::LoadIcon { key: "-R/icons/missing.svg".to_string() }).expect("send");
    settle(&mut runner, &app_state).await;
    let cache = runner.icon_cache();
    match cache.peek("-R/icons/brush.svg") {
        Some(liquers_lib::ui::theme::IconLoad::Ready(bytes)) => assert!(bytes.starts_with(b"<svg")),
        other => panic!("not loaded: {other:?}"),
    }
    assert!(matches!(cache.peek("-R/icons/missing.svg"), Some(liquers_lib::ui::theme::IconLoad::Failed)));
}
