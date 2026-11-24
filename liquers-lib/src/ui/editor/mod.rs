//! Query, pipeline, action, resource and key editors as UI elements.
//!
//! An editor is a view and control bound to where its value lives by a value accessor
//! (`liquers_core::accessor`): editing a recipe's query in `recipes.yaml` writes the recipe, and
//! every other editor and view of it follows. Its working copy is a
//! [`liquers_core::pipeline::QueryEditor`] or [`liquers_core::pipeline::PipelineEditor`], which
//! also gives it everything it shows from the command metadata registry: labels, `doc`, argument
//! descriptions, types, defaults, `gui_info` widgets, presets, `next` and `filename`.
//!
//! # Main components
//!
//! - [`EditorElement`]: the `UIElement`, with an [`EditorKind`] (`Query`, `Action`, `Resource`,
//!   `Key`) and an [`EditorBinding`] (an accessor's query, or the value in memory). "Update
//!   immediately" sets each committed edit into the accessor; otherwise edits wait for Update.
//! - [`EditorState`]: the backend-free core. It applies every [`EditorCommand`] as a validated
//!   edit (a refused edit changes nothing and is shown at its node), keeps at most one write in
//!   flight, turns a change elsewhere into a reload or a conflict, and builds the [`EditorTree`].
//! - [`EditorTree`] and [`EditorNode`]: what the renderers draw — the query text, resource,
//!   `ns`, action, directive and subtree nodes, each action's [`EditorField`]s with the widget
//!   [`effective_gui`] picks, the command picker ([`command_catalog`]) and the file selector.
//! - [`EditorView`]: what is being typed between frames, and the decoding of browser events.
//! - [`NodeContext`] and [`recipe_folder`]: an editor bound to a recipe of a `recipes.yaml` asks
//!   the runner for the folder's recipes and listing (`AppMessage::NodeContext`); its Node
//!   arguments (`ArgumentType::Node`) then list the folder's nodes and offer a rename
//!   ([`EditorCommand::RenameNode`], applied with `liquers_core::recipe_tree::TreeEditor` and
//!   saved by the runner as `AppMessage::RenameData`).
//!
//! # Where it is used
//!
//! - The `lui` commands `query_editor`, `action_editor`, `resource_editor` and `key_editor`
//!   (`ui::commands`) create the element.
//! - `AppRunner` (`ui::runner`) binds, reads and writes its accessor (`AppMessage::Accessor`)
//!   and lists store folders for it (`AppMessage::ListDirectory`).
//! - `egui::editor::show_editor` draws it in egui; `ui::web::editor::editor_html` in the browser.
//!
//! # Related concepts
//!
//! - `liquers_core::pipeline` — the validated edits, including the safe `ns` edits
//!   (`NamespaceImpact`) and the argument reset and variadic item edits of `ActionEditor`.
//! - `liquers_core::accessor` — the binding: `set_if` conflicts and change events.
//! - `liquers_core::recipe_tree` — recipe trees, node names and the tree editor behind Rename.
//! - [`crate::ui::widgets::QueryConsoleElement`] — a text console; its Parameters panel holds an
//!   action editor of its last action.
//!
//! Design: `specs/design/query-pipeline-editor/`.
//!
//! # Examples
//!
//! ```
//! # use std::sync::Arc;
//! # use liquers_core::command_metadata::{ArgumentInfo, CommandMetadata, CommandMetadataRegistry};
//! # use liquers_core::value::ValueInterface;
//! # use liquers_lib::ui::editor::{EditorCommand, EditorEffect, EditorKind, EditorState, NodePath};
//! # use liquers_lib::value::Value;
//! # let mut cmr = CommandMetadataRegistry::new();
//! # let mut head = CommandMetadata::new("head");
//! # head.with_argument(ArgumentInfo::integer_argument("n", false).with_default(5));
//! # cmr.add_command(&head);
//! let mut editor = EditorState::new(EditorKind::Query, Arc::new(cmr));
//! let query = liquers_core::parse::parse_query("-R/data.csv/-/head-3")?;
//! editor.on_snapshot(&Value::from_query(&query), None)?;
//! let set = |text: &str| EditorCommand::SetField { path: NodePath::element(1), arg: "n".into(), text: text.into() };
//! assert!(matches!(editor.apply(set("10"))?, EditorEffect::Write { .. }));
//! assert_eq!(editor.tree().text, "-R/data.csv/-/head-10");
//! assert!(editor.apply(set("abc")).is_err()); // a refused edit changes nothing
//! assert_eq!(editor.tree().text, "-R/data.csv/-/head-10");
//! # Ok::<(), liquers_core::error::Error>(())
//! ```
//!
//! - [Tests](https://github.com/orest-d/liquers/blob/main/liquers-lib/src/ui/editor/tests.rs):
//!   every command of the core; `liquers-lib/tests/ui_editor_binding.rs`: two editors of one
//!   recipe through the runner.

mod catalog;
mod command;
mod element;
mod keys;
mod layout;
mod spans;
mod state;
mod tree;
mod view;

pub use catalog::command_catalog;
pub use command::{EditorCommand, EditorEffect, NewElement};
pub use element::{recipe_folder, EditorBinding, EditorElement};
pub use keys::{editor_keymap, keymap_table, shortcut_text, EditorKey, KeyScope};
pub use layout::{
    card_marks, command_id, editor_layout, form_rows, header_parts, node_label, offers_key, simple_card, toolbar_items, Card,
    CardGroup, CardGroupKind, CardMarks, EditorLayout, EditorMode, FormRow, HeaderPart, StepChip, ToolItem,
};
pub use state::{EditorKind, EditorState, UpdateMode};
pub use tree::{
    default_text, effective_gui, is_numeric, json_text, scale_numeric, Browser, CatalogCommand,
    CatalogNamespace, Confirmation, DirectoryEntry, DirectoryListing, EditorField, EditorNode,
    EditorTree, FieldValue, NodeContext, NodePath, PathStep, Picker, PresetEntry, SpanTarget, TextSpan,
    RESOURCE_HEADERS,
};
pub use view::{event_name, field_id, find_node, open_card, CardMenu, EditorView, FocusTarget};

#[cfg(test)]
mod tests;
