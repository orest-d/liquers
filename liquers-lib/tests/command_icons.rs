//! The bundled `pl`, `img` and `rec` commands declare `hint icon` with names the icon set knows,
//! and their presets and next presets parse.
//!
//! Design: `specs/design/query-editor-ergonomics/` (Phase 3, test 19, AC-13).
#![cfg(all(feature = "image-support", feature = "polars", feature = "records"))]

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::ui::theme::{is_known_set_icon, Icon};
use liquers_lib::value::Value;

type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

fn environment() -> Result<CommandEnvironment, Error> {
    let mut env = CommandEnvironment::new();
    let cr = env.get_mut_command_registry();
    liquers_lib::register_image_commands!(cr)?;
    liquers_lib::register_polars_commands!(cr)?;
    liquers_lib::register_records_commands!(cr)?;
    let _ = (std::marker::PhantomData::<State<Value>>, std::marker::PhantomData::<Context<CommandEnvironment>>);
    Ok(env)
}

// Test 19 (AC-13).
#[test]
fn bundled_commands_declare_icons_and_valid_presets() {
    let env = environment().expect("commands");
    let registry = env.get_command_metadata_registry();
    let mut with_icon = std::collections::BTreeMap::<String, usize>::new();
    for md in &registry.commands {
        let ns = md.namespace.as_str();
        if !matches!(ns, "pl" | "img" | "rec") {
            continue;
        }
        if let Some(hint) = md.hints.get("icon") {
            let icon = Icon::from_hint(hint).unwrap_or_else(|e| panic!("{ns}/{}: {e}", md.name));
            assert!(
                icon.variants.iter().all(is_known_set_icon),
                "{ns}/{}: unknown icon {hint}",
                md.name
            );
            *with_icon.entry(ns.to_string()).or_default() += 1;
        }
        for preset in md.presets.iter().chain(md.next.iter()) {
            assert!(!preset.action.encode().is_empty(), "{ns}/{}: {preset:?}", md.name);
        }
    }
    for ns in ["pl", "img", "rec"] {
        assert!(with_icon.get(ns).copied().unwrap_or(0) >= 10, "{ns}: {with_icon:?}");
    }
    let head = registry
        .commands
        .iter()
        .find(|md| md.namespace == "pl" && md.name == "head")
        .expect("pl/head");
    assert_eq!(head.label, "Get first rows");
    assert!(head.presets.len() >= 3 && !head.next.is_empty());
    assert_eq!(head.arguments[0].label, "Number of rows");
}
