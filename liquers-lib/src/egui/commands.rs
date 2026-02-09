use std::sync::Arc;

use liquers_core::{
    context::{Context, Environment},
    error::Error,
    state::State,
    value::ValueInterface,
};
use liquers_macro::register_command;

use crate::{
    egui::{widgets::display_asset_info, UIValueExtension},
    environment::CommandRegistryAccess,
};

pub fn label<E: Environment>(
    text: String,
    _context: Context<E>,
) -> Result<E::Value, Error>
where
    E::Value: UIValueExtension,
{
    Ok(E::Value::from_ui(move |ui| {
        ui.label(&text);
        Ok(())
    }))
}

pub fn text_editor<E: Environment>(
    state: &State<E::Value>,
    context: Context<E>,
) -> Result<E::Value, Error>
where
    E::Value: UIValueExtension,
{
    println!(
        "text_editor command called, state type: {}",
        state.data.type_name()
    );
    let key = state.data.try_into_key()?;
    Ok(E::Value::from_widget(Arc::new(std::sync::Mutex::new(
        crate::egui::widgets::TextEditor::<E>::new(key, context.get_envref()),
    ))))
}

pub fn show_asset_info<E: Environment>(state: &State<E::Value>, context:Context<E>) -> Result<E::Value, Error>
where
    E::Value: UIValueExtension,
{
    context.info("Showing asset info in GUI")?;
    let asset_info = state.metadata.get_asset_info()?;
    context.info(&format!("\nAsset Info: {:?}\n", asset_info))?;
    Ok(E::Value::from_ui(move |ui| {
        display_asset_info(ui, &asset_info);
        Ok(())
    }))
}

/// Register egui commands via macro.
///
/// The caller must define `type CommandEnvironment = ...` in scope before invoking.
#[macro_export]
macro_rules! register_egui_commands {
    ($cr:expr) => {{
        use liquers_macro::register_command;
        use $crate::egui::commands::*;

        register_command!($cr,
            fn label(txt : String, context) -> result
            label: "Show label"
            doc: "Show text as a GUI label"
        )?;
        register_command!($cr,
            fn text_editor(state, context) -> result
            label: "Text editor"
            doc: "Show a text editor widget for editing text identified by a key; state must contain a key"
        )?;
        register_command!($cr,
            fn show_asset_info(state, context) -> result
            label: "Show asset info"
            doc: "Show information about an asset sourced from the state metadata"
        )?;
        Ok::<(), liquers_core::error::Error>(())
    }};
}

/// Backward-compatible wrapper calling the `register_egui_commands!` macro.
pub fn register_commands(
    mut env: crate::environment::DefaultEnvironment<crate::value::Value>,
) -> Result<crate::environment::DefaultEnvironment<crate::value::Value>, Error> {
    let cr = env.get_mut_command_registry();
    type CommandEnvironment = crate::environment::DefaultEnvironment<crate::value::Value>;
    register_egui_commands!(cr)?;
    Ok(env)
}
