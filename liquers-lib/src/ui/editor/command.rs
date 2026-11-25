//! The user's intentions, as the renderers report them to [`super::EditorState::apply`], and
//! what applying one asks the runner to do.

use liquers_core::accessor::Fingerprint;
use liquers_core::command_metadata::CommandKey;

use super::tree::NodePath;
use crate::value::Value;

/// A new element for [`EditorCommand::Insert`] and [`EditorCommand::Append`].
#[derive(Debug, Clone, PartialEq)]
pub enum NewElement {
    /// A command; its namespace is added as an `ns` when it needs one (AC-17).
    Action(CommandKey),
    /// A resource read of `key`.
    Resource(String),
    /// A `q` directive.
    Quote,
    /// A `v` directive.
    Volatile,
}

/// One intention of the user. Paths name the node it acts on: an action for the argument
/// commands, a resource for the key commands, any element for `Remove` and `Move`.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorCommand {
    /// The whole query text was edited (AC-4).
    SetText(String),
    /// An argument's literal, as typed (AC-2, AC-5).
    SetField { path: NodePath, arg: String, text: String },
    /// One item of a variadic argument, as typed.
    SetItem { path: NodePath, arg: String, at: usize, text: String },
    /// An argument becomes a link with this query text (AC-19).
    SetLink { path: NodePath, arg: String, query: String },
    /// An argument becomes a link to a new resource subtree (AC-19).
    ToLink { path: NodePath, arg: String },
    /// A link argument becomes a value: its default (AC-19).
    ToValue { path: NodePath, arg: String },
    /// An argument back to its default (AC-18).
    Reset { path: NodePath, arg: String },
    /// Variadic items (AC-2).
    InsertItem { path: NodePath, arg: String, at: usize, text: String },
    RemoveItem { path: NodePath, arg: String, at: usize },
    MoveItem { path: NodePath, arg: String, from: usize, to: usize },
    /// A whole element of a shaped variadic argument (a tuple, or a key and value pair) before
    /// row `at`: one text per component. A dictionary refuses a key it already has, so the
    /// query never holds part of an element (design `composite-variadic-arguments`, AC-13).
    InsertRow { path: NodePath, arg: String, at: usize, texts: Vec<String> },
    /// Removes row `at`: every component of the element.
    RemoveRow { path: NodePath, arg: String, at: usize },
    /// Moves row `from` to position `to`.
    MoveRow { path: NodePath, arg: String, from: usize, to: usize },
    /// Replaces every item of a variadic argument: a JSON value changing its form.
    SetItems { path: NodePath, arg: String, texts: Vec<String> },
    /// A new element before `path` (AC-6).
    Insert { path: NodePath, element: NewElement },
    /// A new element at the end of the top level (AC-6).
    Append(NewElement),
    /// Removes the element at `path` (AC-6).
    Remove { path: NodePath },
    /// Moves the element at `path` to index `to` of its level (AC-6).
    Move { path: NodePath, to: usize },
    /// The `ns` list of the run of actions at `path` (AC-16, AC-25).
    SetNamespaces { path: NodePath, namespaces: Vec<String> },
    /// Accepts the namespace edit awaiting confirmation (AC-25).
    ConfirmNamespaces,
    /// Drops the namespace edit awaiting confirmation.
    CancelNamespaces,
    /// Replaces the action at `path` by its command's preset number `index` (AC-20).
    ApplyPreset { path: NodePath, index: usize },
    /// Inserts the command's next preset number `index` after the action at `path` (AC-21).
    ApplyNext { path: NodePath, index: usize },
    /// Sets an argument to its preset number `index` (AC-24).
    ApplyArgumentPreset { path: NodePath, arg: String, index: usize },
    /// The header of the resource at `path` (AC-7): one of [`super::tree::RESOURCE_HEADERS`].
    SetHeader { path: NodePath, header: String },
    /// The key of the resource at `path`, typed or picked (AC-7, AC-14, AC-15).
    SetKey { path: NodePath, key: String },
    /// Opens the file selector of the resource at `path` on `folder` (AC-14).
    Browse { path: NodePath, folder: String },
    CloseBrowser,
    /// Opens or filters the command picker (AC-17): before `insert_at`, or appended.
    SearchCommands { insert_at: Option<NodePath>, filter: String },
    /// Inserts the picked command where the picker was opened.
    PickCommand(CommandKey),
    ClosePicker,
    /// The query's filename; empty clears it (AC-22).
    SetFilename(String),
    /// The filename declared by the command of the action at `path` (AC-22).
    FilenameFrom { path: NodePath },
    /// The filename declared by the last command (AC-22).
    DefaultFilename,
    /// A controller sets numeric argument `index` of the edited action to `value` in `0..=max`
    /// (AC-13).
    SetNumeric { index: usize, value: f64, max: f64 },
    /// Sets the pending edits into the accessor (AC-26).
    Update,
    /// Drops the pending edits and reloads the accessor's value (AC-26).
    Revert,
    /// Sets the update mode: what a committed edit does (design `query-update-modes`).
    SetUpdateMode(super::UpdateMode),
    /// Restores the working copy before the last edit (AC-10).
    Undo,
    /// Re-applies the edit the last [`EditorCommand::Undo`] took back (AC-10).
    Redo,
    /// Renames node `from` of the folder's recipe tree to `to`, rewriting every reference in
    /// `recipes.yaml` and renaming a stored file (design `recipe-tree`, AC-25). Needs a
    /// [`super::NodeContext`].
    RenameNode { from: String, to: String },
}

/// What [`super::EditorState::apply`] asks the runner to do.
#[derive(Debug, Clone)]
pub enum EditorEffect {
    None,
    /// Set `value` into the accessor if its fingerprint is still `expected`.
    Write { value: Value, expected: Option<Fingerprint> },
    /// Read the accessor again (Revert).
    Reload,
    /// List the store directory `folder` for the file selector.
    ListDirectory(String),
    /// Save a renamed recipe tree: rename the stored files, then write `recipes.yaml` of
    /// `folder` (`TreeEditor::save`; design `recipe-tree`, AC-25).
    RenameData {
        folder: String,
        editor: Box<liquers_core::recipe_tree::TreeEditor>,
    },
    /// Show `value` as a preview (Live preview); nothing is written (AC-17).
    Preview(Value),
    /// Drop the preview and show the applied value again (Revert, or a write in Live preview).
    ClearPreview,
}
