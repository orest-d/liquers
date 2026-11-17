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
//! - [`crate::ui::widgets::QueryConsoleElement`] — a text console; its Parameters panel holds an
//!   action editor of its last action.
//!
//! Design: `specs/design/query-pipeline-editor/`.
//!
//! # Examples
//!
//! ```
//! use std::sync::Arc;
//! use liquers_core::command_metadata::{ArgumentInfo, CommandMetadata, CommandMetadataRegistry};
//! use liquers_core::query::Query;
//! use liquers_core::value::ValueInterface;
//! use liquers_lib::ui::editor::{EditorCommand, EditorEffect, EditorKind, EditorState, NodePath};
//! use liquers_lib::value::Value;
//!
//! let mut cmr = CommandMetadataRegistry::new();
//! let mut head = CommandMetadata::new("head");
//! head.with_argument(ArgumentInfo::integer_argument("n", false).with_default(5));
//! cmr.add_command(&head);
//!
//! let mut editor = EditorState::new(EditorKind::Query, Arc::new(cmr));
//! let query = liquers_core::parse::parse_query("-R/data.csv/-/head-3")?;
//! editor.on_snapshot(&Value::from_query(&query), None)?;
//! let effect = editor.apply(EditorCommand::SetField {
//!     path: NodePath::element(1),
//!     arg: "n".into(),
//!     text: "10".into(),
//! })?;
//! assert_eq!(editor.tree().text, "-R/data.csv/-/head-10");
//! assert!(matches!(effect, EditorEffect::Write { .. }));
//! // A refused edit changes nothing.
//! assert!(editor.apply(EditorCommand::SetField {
//!     path: NodePath::element(1), arg: "n".into(), text: "abc".into(),
//! }).is_err());
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
mod state;
mod tree;
mod view;

pub use catalog::command_catalog;
pub use command::{EditorCommand, EditorEffect, NewElement};
pub use element::{EditorBinding, EditorElement};
pub use state::{EditorKind, EditorState};
pub use tree::{
    default_text, effective_gui, is_numeric, json_text, scale_numeric, Browser, CatalogCommand,
    CatalogNamespace, Confirmation, DirectoryEntry, DirectoryListing, EditorField, EditorNode,
    EditorTree, FieldValue, NodePath, PathStep, Picker, PresetEntry, RESOURCE_HEADERS,
};
pub use view::{event_name, field_id, find_node, EditorView};

#[cfg(test)]
mod tests;
