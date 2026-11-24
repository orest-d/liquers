//! stub
use crate::ui::recipes::{RecipeForm, RecipesElement, FormCommand};
pub struct FormOutput { pub commands: Vec<FormCommand>, pub response: egui::Response }
pub fn show_recipes(_e: &mut RecipesElement, ui: &mut egui::Ui, _c: &crate::ui::ui_context::UIContext, _a: &mut dyn crate::ui::app_state::AppState) -> egui::Response { ui.label("") }
pub fn show_form(ui: &mut egui::Ui, _f: &mut RecipeForm, _t: &crate::ui::theme::ThemeState, _c: Option<&crate::ui::ui_context::UIContext>, _m: Option<&str>, _w: bool) -> FormOutput { FormOutput{commands: vec![], response: ui.label("")} }
