use std::sync::Arc;

use liquers_core::{
    context::{Context, Environment},
    error::Error,
    state::State,
    value::ValueInterface,
};

use crate::egui::{UIValueExtension, widgets::display_asset_info};

pub fn label<E: Environment>(
    _state: &State<E::Value>,
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

pub fn show_asset_info<E: Environment>(state: &State<E::Value>) -> Result<E::Value, Error> 
where
    E::Value: UIValueExtension,
{
    println!(
        "asset_info command called, state type: {}",
        state.data.type_name()
    );
    let asset_info = state.metadata.get_asset_info()?;
    println!("\nAsset Info: {:?}\n", asset_info);
    Ok(E::Value::from_ui(move |ui| {
        display_asset_info(ui, &asset_info);
        Ok(())
    }))
}
