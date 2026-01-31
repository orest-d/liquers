use std::sync::Arc;

use egui::RichText;
use liquers_core::error::Error;

use crate::{egui::widgets::{display_asset_info, display_asset_info_table, display_error, display_recipe, display_styled_query}, value::{ExtValue, Value, simple::SimpleValue}};

pub mod widgets;
pub mod commands;
pub mod dataframe;

type UiClosure = Box<dyn FnMut(&mut egui::Ui) -> Result<(), Error> + Send>;
#[derive(Clone)]
pub struct UiCommand {
    value: Arc<std::sync::Mutex<UiClosure>>,
}

impl UiCommand {
    pub fn new<F>(f: F) -> Self
    where
        F: FnMut(&mut egui::Ui) -> Result<(), Error> + Send + 'static,
    {
        UiCommand {
            value: Arc::new(std::sync::Mutex::new(Box::new(f))),
        }
    }

    pub fn execute(&self, ui: &mut egui::Ui) {
        let mut closure = self.value.lock().unwrap();
        let res = (closure)(ui);
        if let Err(e) = res {
            let _ = display_error(ui, &e);
        }
    }
}

impl std::fmt::Debug for UiCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "UiCommand")
    }
}


pub trait UIValueExtension{
    fn show(&self, ui: &mut egui::Ui);
    fn from_ui<F>(f: F) -> Self
    where
        F: FnMut(&mut egui::Ui) -> Result<(), Error> + Send + 'static;

    fn from_widget(widget: Arc<std::sync::Mutex<dyn crate::egui::widgets::WidgetValue>>) -> Self;
}

impl UIValueExtension for Value {
    fn from_ui<F>(f: F) -> Self
    where
        F: FnMut(&mut egui::Ui) -> Result<(), Error> + Send + 'static,
    {
        Self::new_extended(ExtValue::UiCommand {
            value: crate::egui::UiCommand::new(f),
        })
    }
    fn from_widget(widget: Arc<std::sync::Mutex<dyn crate::egui::widgets::WidgetValue>>) -> Self {
        Self::new_extended(ExtValue::Widget { value: widget })
    }
    fn show(&self, ui: &mut egui::Ui) {
        match self {
            Self::Extended(ExtValue::UiCommand { value }) => {
                value.execute(ui);
            }
            Self::Extended(ExtValue::Widget { value }) => {
                let mut widget = value.lock().expect("Failed to lock widget mutex");
                widget.show(ui);
            }
            Self::Extended(ExtValue::Image { value }) => {
                crate::egui::widgets::display_image(ui, value);
            },
            Self::Extended(ExtValue::PolarsDataFrame { value }) => {
                let mut sort_state: Option<(usize, bool)> = None;
                crate::egui::dataframe::display_polars_dataframe(ui, &value, &mut sort_state);
            }
            Self::Base(SimpleValue::None {}) => {
                ui.label(RichText::new("None").italics());
            }
            Self::Base(SimpleValue::Bool { value }) => {
                ui.label(RichText::new(format!("Bool: {}", value)).italics());
            }
            Self::Base(SimpleValue::I32 { value }) => {
                ui.label(RichText::new(format!("I32: {}", value)).italics());
            }
            Self::Base(SimpleValue::I64 { value }) => {
                ui.label(RichText::new(format!("I64: {}", value)).italics());
            }
            Self::Base(SimpleValue::F64 { value }) => {
                ui.label(RichText::new(format!("F64: {}", value)).italics());
            }
            Self::Base(SimpleValue::Text { value }) => {
                ui.label(value);
            }
            Self::Base(SimpleValue::Array { value }) => {
                ui.label(RichText::new("Array").italics());
            }
            Self::Base(SimpleValue::Object { value }) => {
                ui.label(RichText::new("Object").italics());
            }
            Self::Base(SimpleValue::Bytes { value }) => {
                ui.label(RichText::new(format!("Bytes: {} bytes", value.len())).italics());
            }
            Self::Base(SimpleValue::Metadata { value }) => todo!(),
            Self::Base(SimpleValue::AssetInfo { value }) => {
                if value.is_empty() {
                    ui.label(RichText::new("Asset Info: <empty>").italics());
                } else {
                    if value.len() == 1 {
                        display_asset_info(ui, &value[0]);
                    } else {
                        display_asset_info_table(ui, &value);
                    }
                }
            }
            Self::Base(SimpleValue::Recipe { value }) => {
                display_recipe(ui, value);
            },
            Self::Base(SimpleValue::CommandMetadata { value }) => todo!(),
            Self::Base(SimpleValue::Query { value }) => {
                ui.label("Query:");
                display_styled_query(ui, value.clone());
            },
            Self::Base(SimpleValue::Key { value }) => {
                ui.label("Key:");
                display_styled_query(ui, value.clone());
            },
        }
    }
}