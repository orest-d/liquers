//! Recipe editors: a narrow side panel over a folder's `recipes.yaml` (an outline of its recipes,
//! or the tree of their dependencies), a form for one recipe, and a YAML window with highlighting
//! (design `recipes-editor`).
//!
//! The recipe list is not a value type. The panel reads and writes `recipes.yaml` as document
//! text through a value accessor, keeps a working copy as a
//! [`TreeEditor`](liquers_core::recipe_tree::TreeEditor) (so a rename rewrites the references to
//! the node, a used name is made unique, and an edit that would add a cycle is refused), and opens
//! one [`RecipeForm`] per recipe, keyed by filename. New adds an empty `<n>.node` node; a file with
//! a `tree:` section that fails its check is edited as plain recipes under a notice
//! ([`RecipesState::tree_notice`]), and edits a failed write left in memory are marked
//! ([`RecipesState::unsaved`]). A single recipe is edited through the `acc/recipe_file` accessor,
//! which finds the recipe by its filename, not by its position.
//!
//! # Main components
//!
//! - [`RecipesElement`]: the panel, built by `ns-lui/recipes_editor`. Its model is a
//!   [`RecipesState`], changed by [`RecipesCommand`]s, which return the [`RecipesEffect`] the
//!   element performs (an accessor write, a tree save with stored-file renames, a query).
//! - [`RecipeElement`]: one recipe as a form (`ns-lui/recipe_editor`).
//! - [`RecipeForm`], [`FormCommand`], [`FormChange`]: the form, with the query editor inside.
//! - [`RecipeRow`], [`RowSource`], [`RowStatus`], [`menu_items`]: the panel's rows, their status
//!   (a coloured circle, or a percentage while in progress), their tooltips and context menus.
//! - [`yaml_spans`], [`parse_recipes`]: the YAML window's highlighting and parse errors.
//!
//! # Where it is used
//!
//! - `crate::egui::recipes` (feature `egui`) draws the panel, the form windows and the YAML window.
//! - `crate::ui::web::recipes` (feature `webui`) renders them as HTML.
//! - `crate::ui::commands`: `lui/recipes_editor` and `lui/recipe_editor` build the elements.
//!
//! # Related concepts
//!
//! - `specs/reference/RECIPE_TREES.md`: nodes, references and the checks a tree edit passes.
//! - `specs/reference/VALUE_ACCESSORS.md`: how the panel reads and writes the file.
//! - `specs/reference/UI_INTERFACE_FSD.md`: elements, messages and the runner.
//!
//! # Examples
//!
//! ```
//! use liquers_lib::ui::recipes::{RecipesCommand, RecipesEffect, RecipesState};
//! use liquers_lib::value::Value;
//!
//! let mut state = RecipesState::new(Default::default()).with_folder(Some("data".into()));
//! state.on_snapshot(&Value::from("recipes:\n- {query: -R/data/x.csv/-/head/b.csv, title: B}\n"), None).unwrap();
//! assert_eq!(state.rows()[0].name, "b.csv");
//! // The working copy changes at once; the write is what the effect asks for.
//! let effect = state.apply(RecipesCommand::Rename { from: "b.csv".into(), to: "c".into() }).unwrap();
//! assert!(matches!(effect, RecipesEffect::None | RecipesEffect::Write { .. }));
//! ```

mod element;
mod form;
mod rows;
mod state;
mod yaml;
#[cfg(test)]
mod tests;

pub use element::{form_field_changes, panel_commands, recipes_folder, NewDialog, RecipeElement, RecipesElement, RecipesUi};
pub use form::{FormChange, FormCommand, RecipeForm};
pub use rows::{
    findings_by_row, menu_items, status_label, MenuAction, MenuItem, RecipeLabel, RecipeRow, RecipesView, RowSource,
    RowStatus,
};
pub use state::{RecipesCommand, RecipesEffect, RecipesState, YamlBuffer};
pub use yaml::{parse_recipes, yaml_spans};
