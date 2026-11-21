//! The editors' keyboard model (AC-9, AC-10): a renderer-neutral keymap. The egui renderer reads
//! it now; the browser renderer will read the same table later. `Ctrl` means Cmd on macOS
//! ([`KeyboardShortcut`]).

use std::sync::OnceLock;

use crate::ui::shortcuts::KeyboardShortcut;

/// What a key does in an editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditorKey {
    /// Toggle Simple / Advanced.
    ToggleMode,
    /// Focus the query text.
    FocusQuery,
    /// Between the text cursor and the field it is on.
    Jump,
    NextRegion,
    PrevRegion,
    NextCard,
    PrevCard,
    FirstCard,
    LastCard,
    /// Into the focused card's first field.
    Enter,
    /// Up one focus level.
    Leave,
    ToggleCard,
    Collapse,
    Expand,
    CollapseAll,
    ExpandAll,
    Reset,
    Presets,
    Next,
    /// Apply preset `n` (1-based as typed: `Alt+1` is preset index 0).
    ApplyPreset(u8),
    AddAction,
    MoveUp,
    MoveDown,
    DeleteCard,
    ToggleLink,
    Complete,
    Update,
}

/// Where a key applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyScope {
    /// Anywhere in the editor, also inside text fields.
    Anywhere,
    /// When a card header has focus (not while typing).
    Header,
    /// In a field.
    Field,
}

/// The table of `reference-design.md` §3, as written.
const TABLE: &[(&str, EditorKey, KeyScope)] = &[
    ("Ctrl+B", EditorKey::ToggleMode, KeyScope::Anywhere),
    ("Ctrl+L", EditorKey::FocusQuery, KeyScope::Anywhere),
    ("Ctrl+J", EditorKey::Jump, KeyScope::Anywhere),
    ("F6", EditorKey::NextRegion, KeyScope::Anywhere),
    ("Shift+F6", EditorKey::PrevRegion, KeyScope::Anywhere),
    ("PageDown", EditorKey::NextCard, KeyScope::Header),
    ("PageUp", EditorKey::PrevCard, KeyScope::Header),
    ("Alt+PageDown", EditorKey::NextCard, KeyScope::Anywhere),
    ("Alt+PageUp", EditorKey::PrevCard, KeyScope::Anywhere),
    ("Home", EditorKey::FirstCard, KeyScope::Header),
    ("End", EditorKey::LastCard, KeyScope::Header),
    ("Enter", EditorKey::Enter, KeyScope::Header),
    ("Escape", EditorKey::Leave, KeyScope::Header),
    ("Space", EditorKey::ToggleCard, KeyScope::Header),
    ("ArrowLeft", EditorKey::Collapse, KeyScope::Header),
    ("ArrowRight", EditorKey::Expand, KeyScope::Header),
    ("Ctrl+Shift+[", EditorKey::CollapseAll, KeyScope::Anywhere),
    ("Ctrl+Shift+]", EditorKey::ExpandAll, KeyScope::Anywhere),
    ("Alt+R", EditorKey::Reset, KeyScope::Anywhere),
    ("Alt+P", EditorKey::Presets, KeyScope::Anywhere),
    // Not `Alt+N`: the bundled browser console uses it for New Console.
    ("Alt+Shift+N", EditorKey::Next, KeyScope::Anywhere),
    ("Alt+1", EditorKey::ApplyPreset(1), KeyScope::Anywhere),
    ("Alt+2", EditorKey::ApplyPreset(2), KeyScope::Anywhere),
    ("Alt+3", EditorKey::ApplyPreset(3), KeyScope::Anywhere),
    ("Alt+4", EditorKey::ApplyPreset(4), KeyScope::Anywhere),
    ("Alt+5", EditorKey::ApplyPreset(5), KeyScope::Anywhere),
    ("Alt+6", EditorKey::ApplyPreset(6), KeyScope::Anywhere),
    ("Alt+7", EditorKey::ApplyPreset(7), KeyScope::Anywhere),
    ("Alt+8", EditorKey::ApplyPreset(8), KeyScope::Anywhere),
    ("Alt+9", EditorKey::ApplyPreset(9), KeyScope::Anywhere),
    ("Ctrl+K", EditorKey::AddAction, KeyScope::Anywhere),
    ("Alt+ArrowUp", EditorKey::MoveUp, KeyScope::Anywhere),
    ("Alt+ArrowDown", EditorKey::MoveDown, KeyScope::Anywhere),
    ("Ctrl+Delete", EditorKey::DeleteCard, KeyScope::Header),
    ("Alt+L", EditorKey::ToggleLink, KeyScope::Field),
    ("Ctrl+Space", EditorKey::Complete, KeyScope::Field),
    ("Ctrl+Enter", EditorKey::Update, KeyScope::Anywhere),
];

/// The keymap: each parsed shortcut, what it does and where. An entry that does not parse is
/// skipped (and reported on stderr); a test checks that none is.
pub fn editor_keymap() -> &'static [(KeyboardShortcut, EditorKey, KeyScope)] {
    static MAP: OnceLock<Vec<(KeyboardShortcut, EditorKey, KeyScope)>> = OnceLock::new();
    MAP.get_or_init(|| {
        TABLE
            .iter()
            .filter_map(|(text, key, scope)| match text.parse::<KeyboardShortcut>() {
                Ok(shortcut) => Some((shortcut, *key, *scope)),
                Err(e) => {
                    eprintln!("editor keymap: '{text}' does not parse: {e}");
                    None
                }
            })
            .collect()
    })
}

/// The written form of every keymap entry, for documentation and the collision test.
pub fn keymap_table() -> impl Iterator<Item = (&'static str, EditorKey, KeyScope)> {
    TABLE.iter().copied()
}

/// The shortcut text that triggers `key` (the first entry), for tooltips.
pub fn shortcut_text(key: EditorKey) -> Option<&'static str> {
    TABLE.iter().find(|(_, k, _)| *k == key).map(|(t, _, _)| *t)
}
