use std::sync::Arc;

use egui::RichText;
use liquers_core::error::Error;

use crate::{egui::widgets::display_error, value::Value};

pub mod widgets;
pub mod commands;

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
        Value::UiCommand {
            value: crate::egui::UiCommand::new(f),
        }
    }
    fn from_widget(widget: Arc<std::sync::Mutex<dyn crate::egui::widgets::WidgetValue>>) -> Self {
        Value::Widget { value: widget }
    }
    fn show(&self, ui: &mut egui::Ui) {
        match self {
            Value::UiCommand { value } => {
                value.execute(ui);
            }
            Value::Widget { value } => {
                let mut widget = value.lock().expect("Failed to lock widget mutex");
                widget.show(ui);
            }
            Value::None {} => {
                ui.label(RichText::new("None").italics());
            }
            Value::Bool { value } => {
                ui.label(RichText::new(format!("Bool: {}", value)).italics());
            }
            Value::I32 { value } => {
                ui.label(RichText::new(format!("I32: {}", value)).italics());
            }
            Value::I64 { value } => {
                ui.label(RichText::new(format!("I64: {}", value)).italics());
            }
            Value::F64 { value } => {
                ui.label(RichText::new(format!("F64: {}", value)).italics());
            }
            Value::Text { value } => {
                ui.label(value);
            }
            Value::Array { value } => {
                ui.label(RichText::new("Array").italics());
            }
            Value::Object { value } => {
                ui.label(RichText::new("Object").italics());
            }
            Value::Bytes { value } => {
                ui.label(RichText::new(format!("Bytes: {} bytes", value.len())).italics());
            }
            Value::Metadata { value } => todo!(),
            Value::AssetInfo { value } => todo!(),
            Value::Recipe { value } => todo!(),
            Value::CommandMetadata { value } => todo!(),
            Value::Query { value } => todo!(),
            Value::Key { value } => todo!(),
            Value::Image { value } => todo!(),
            Value::PolarsDataFrame { value } => todo!(),
        }
    }
}