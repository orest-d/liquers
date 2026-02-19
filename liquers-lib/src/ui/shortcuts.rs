//! Platform-independent keyboard shortcuts library
//!
//! This module provides a unified representation for keyboard shortcuts that works
//! across macOS, Windows, and Linux by using a **semantic command modifier** approach.
//!
//! # Semantic Command Modifier
//!
//! The key insight is that the `ctrl` field in [`Modifiers`] is **semantic**, not physical:
//! - On macOS: `ctrl: true` represents the ⌘ Command key
//! - On Windows/Linux: `ctrl: true` represents the Control key
//!
//! This means you can write **one shortcut definition** that works on all platforms:
//! ```
//! use liquers_lib::ui::shortcuts::KeyboardShortcut;
//!
//! let save_shortcut: KeyboardShortcut = "Ctrl+S".parse().unwrap();
//! // On macOS: triggers on ⌘S
//! // On Windows/Linux: triggers on Ctrl+S
//! ```
//!
//! # Examples
//!
//! ## Parsing shortcuts
//! ```
//! use liquers_lib::ui::shortcuts::KeyboardShortcut;
//!
//! let shortcut: KeyboardShortcut = "Ctrl+Shift+S".parse()?;
//! println!("{}", shortcut);  // "Cmd+Shift+S" on macOS, "Ctrl+Shift+S" elsewhere
//! # Ok::<(), liquers_core::error::Error>(())
//! ```
//!
//! ## Converting to egui
//! ```
//! use liquers_lib::ui::shortcuts::KeyboardShortcut;
//!
//! let shortcut: KeyboardShortcut = "Ctrl+Q".parse()?;
//! let egui_shortcut = shortcut.to_egui();
//! // Use with egui::Ui::input_mut(|i| i.consume_shortcut(&egui_shortcut))
//! # Ok::<(), liquers_core::error::Error>(())
//! ```
//!
//! ## Detecting conflicts
//! ```
//! use liquers_lib::ui::shortcuts::{KeyboardShortcut, find_conflicts};
//!
//! let shortcuts = vec![
//!     "Ctrl+S".parse()?,
//!     "Cmd+S".parse()?,  // Same as Ctrl+S (semantic)
//! ];
//! let conflicts = find_conflicts(&shortcuts);
//! assert_eq!(conflicts.len(), 1);  // Ctrl+S and Cmd+S conflict
//! # Ok::<(), liquers_core::error::Error>(())
//! ```

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;
use liquers_core::error::Error;

/// Keyboard key representation
///
/// Covers common keyboard keys including letters, numbers, function keys, navigation,
/// editing keys, and punctuation. Does not cover every possible key (keeps enum manageable).
///
/// # Examples
///
/// ```
/// use liquers_lib::ui::shortcuts::Key;
///
/// assert_eq!(Key::from_name("A"), Some(Key::A));
/// assert_eq!(Key::from_name("Esc"), Some(Key::Escape));  // Aliases supported
/// assert_eq!(Key::from_name("Unknown"), None);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    // Letters
    A, B, C, D, E, F, G, H, I, J, K, L, M,
    N, O, P, Q, R, S, T, U, V, W, X, Y, Z,

    // Numbers
    Num0, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9,

    // Function keys
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,

    // Navigation
    ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
    Home, End, PageUp, PageDown,

    // Editing
    Insert, Delete, Backspace,
    Enter, Tab, Escape, Space,

    // Punctuation
    Comma, Period, Slash, Backslash,
    Semicolon, Quote, Backtick,
    Minus, Equals,
    LeftBracket, RightBracket,

    // Special
    PrintScreen, ScrollLock, Pause,
}

impl Key {
    /// Parse key from string (case-insensitive, with aliases)
    pub fn from_name(name: &str) -> Option<Self> {
        let normalized = name.to_lowercase();
        match normalized.as_str() {
            // Letters
            "a" | "keya" => Some(Key::A),
            "b" | "keyb" => Some(Key::B),
            "c" | "keyc" => Some(Key::C),
            "d" | "keyd" => Some(Key::D),
            "e" | "keye" => Some(Key::E),
            "f" | "keyf" => Some(Key::F),
            "g" | "keyg" => Some(Key::G),
            "h" | "keyh" => Some(Key::H),
            "i" | "keyi" => Some(Key::I),
            "j" | "keyj" => Some(Key::J),
            "k" | "keyk" => Some(Key::K),
            "l" | "keyl" => Some(Key::L),
            "m" | "keym" => Some(Key::M),
            "n" | "keyn" => Some(Key::N),
            "o" | "keyo" => Some(Key::O),
            "p" | "keyp" => Some(Key::P),
            "q" | "keyq" => Some(Key::Q),
            "r" | "keyr" => Some(Key::R),
            "s" | "keys" => Some(Key::S),
            "t" | "keyt" => Some(Key::T),
            "u" | "keyu" => Some(Key::U),
            "v" | "keyv" => Some(Key::V),
            "w" | "keyw" => Some(Key::W),
            "x" | "keyx" => Some(Key::X),
            "y" | "keyy" => Some(Key::Y),
            "z" | "keyz" => Some(Key::Z),

            // Numbers
            "0" | "digit0" | "key0" => Some(Key::Num0),
            "1" | "digit1" | "key1" => Some(Key::Num1),
            "2" | "digit2" | "key2" => Some(Key::Num2),
            "3" | "digit3" | "key3" => Some(Key::Num3),
            "4" | "digit4" | "key4" => Some(Key::Num4),
            "5" | "digit5" | "key5" => Some(Key::Num5),
            "6" | "digit6" | "key6" => Some(Key::Num6),
            "7" | "digit7" | "key7" => Some(Key::Num7),
            "8" | "digit8" | "key8" => Some(Key::Num8),
            "9" | "digit9" | "key9" => Some(Key::Num9),

            // Function keys
            "f1" => Some(Key::F1),
            "f2" => Some(Key::F2),
            "f3" => Some(Key::F3),
            "f4" => Some(Key::F4),
            "f5" => Some(Key::F5),
            "f6" => Some(Key::F6),
            "f7" => Some(Key::F7),
            "f8" => Some(Key::F8),
            "f9" => Some(Key::F9),
            "f10" => Some(Key::F10),
            "f11" => Some(Key::F11),
            "f12" => Some(Key::F12),

            // Navigation
            "arrowup" | "up" => Some(Key::ArrowUp),
            "arrowdown" | "down" => Some(Key::ArrowDown),
            "arrowleft" | "left" => Some(Key::ArrowLeft),
            "arrowright" | "right" => Some(Key::ArrowRight),
            "home" => Some(Key::Home),
            "end" => Some(Key::End),
            "pageup" | "pgup" => Some(Key::PageUp),
            "pagedown" | "pgdn" | "pgdown" => Some(Key::PageDown),

            // Editing
            "insert" | "ins" => Some(Key::Insert),
            "delete" | "del" => Some(Key::Delete),
            "backspace" | "back" => Some(Key::Backspace),
            "enter" | "return" => Some(Key::Enter),
            "tab" => Some(Key::Tab),
            "escape" | "esc" => Some(Key::Escape),
            "space" | " " => Some(Key::Space),

            // Punctuation
            "comma" | "," => Some(Key::Comma),
            "period" | "." => Some(Key::Period),
            "slash" | "/" => Some(Key::Slash),
            "backslash" | "\\" => Some(Key::Backslash),
            "semicolon" | ";" => Some(Key::Semicolon),
            "quote" | "'" | "apostrophe" => Some(Key::Quote),
            "backtick" | "`" | "grave" => Some(Key::Backtick),
            "minus" | "-" | "hyphen" => Some(Key::Minus),
            "equals" | "=" | "equal" => Some(Key::Equals),
            "leftbracket" | "[" | "bracketleft" => Some(Key::LeftBracket),
            "rightbracket" | "]" | "bracketright" => Some(Key::RightBracket),

            // Special
            "printscreen" | "prtsc" | "print" => Some(Key::PrintScreen),
            "scrolllock" | "scroll" => Some(Key::ScrollLock),
            "pause" | "break" => Some(Key::Pause),

            _ => None,
        }
    }

    /// Canonical name for display
    pub const fn name(&self) -> &'static str {
        match self {
            Key::A => "A",
            Key::B => "B",
            Key::C => "C",
            Key::D => "D",
            Key::E => "E",
            Key::F => "F",
            Key::G => "G",
            Key::H => "H",
            Key::I => "I",
            Key::J => "J",
            Key::K => "K",
            Key::L => "L",
            Key::M => "M",
            Key::N => "N",
            Key::O => "O",
            Key::P => "P",
            Key::Q => "Q",
            Key::R => "R",
            Key::S => "S",
            Key::T => "T",
            Key::U => "U",
            Key::V => "V",
            Key::W => "W",
            Key::X => "X",
            Key::Y => "Y",
            Key::Z => "Z",
            Key::Num0 => "0",
            Key::Num1 => "1",
            Key::Num2 => "2",
            Key::Num3 => "3",
            Key::Num4 => "4",
            Key::Num5 => "5",
            Key::Num6 => "6",
            Key::Num7 => "7",
            Key::Num8 => "8",
            Key::Num9 => "9",
            Key::F1 => "F1",
            Key::F2 => "F2",
            Key::F3 => "F3",
            Key::F4 => "F4",
            Key::F5 => "F5",
            Key::F6 => "F6",
            Key::F7 => "F7",
            Key::F8 => "F8",
            Key::F9 => "F9",
            Key::F10 => "F10",
            Key::F11 => "F11",
            Key::F12 => "F12",
            Key::ArrowUp => "ArrowUp",
            Key::ArrowDown => "ArrowDown",
            Key::ArrowLeft => "ArrowLeft",
            Key::ArrowRight => "ArrowRight",
            Key::Home => "Home",
            Key::End => "End",
            Key::PageUp => "PageUp",
            Key::PageDown => "PageDown",
            Key::Insert => "Insert",
            Key::Delete => "Delete",
            Key::Backspace => "Backspace",
            Key::Enter => "Enter",
            Key::Tab => "Tab",
            Key::Escape => "Escape",
            Key::Space => "Space",
            Key::Comma => "Comma",
            Key::Period => "Period",
            Key::Slash => "Slash",
            Key::Backslash => "Backslash",
            Key::Semicolon => "Semicolon",
            Key::Quote => "Quote",
            Key::Backtick => "Backtick",
            Key::Minus => "Minus",
            Key::Equals => "Equals",
            Key::LeftBracket => "LeftBracket",
            Key::RightBracket => "RightBracket",
            Key::PrintScreen => "PrintScreen",
            Key::ScrollLock => "ScrollLock",
            Key::Pause => "Pause",
        }
    }

    /// Convert to egui::Key
    pub fn to_egui(&self) -> egui::Key {
        match self {
            Key::A => egui::Key::A,
            Key::B => egui::Key::B,
            Key::C => egui::Key::C,
            Key::D => egui::Key::D,
            Key::E => egui::Key::E,
            Key::F => egui::Key::F,
            Key::G => egui::Key::G,
            Key::H => egui::Key::H,
            Key::I => egui::Key::I,
            Key::J => egui::Key::J,
            Key::K => egui::Key::K,
            Key::L => egui::Key::L,
            Key::M => egui::Key::M,
            Key::N => egui::Key::N,
            Key::O => egui::Key::O,
            Key::P => egui::Key::P,
            Key::Q => egui::Key::Q,
            Key::R => egui::Key::R,
            Key::S => egui::Key::S,
            Key::T => egui::Key::T,
            Key::U => egui::Key::U,
            Key::V => egui::Key::V,
            Key::W => egui::Key::W,
            Key::X => egui::Key::X,
            Key::Y => egui::Key::Y,
            Key::Z => egui::Key::Z,
            Key::Num0 => egui::Key::Num0,
            Key::Num1 => egui::Key::Num1,
            Key::Num2 => egui::Key::Num2,
            Key::Num3 => egui::Key::Num3,
            Key::Num4 => egui::Key::Num4,
            Key::Num5 => egui::Key::Num5,
            Key::Num6 => egui::Key::Num6,
            Key::Num7 => egui::Key::Num7,
            Key::Num8 => egui::Key::Num8,
            Key::Num9 => egui::Key::Num9,
            Key::F1 => egui::Key::F1,
            Key::F2 => egui::Key::F2,
            Key::F3 => egui::Key::F3,
            Key::F4 => egui::Key::F4,
            Key::F5 => egui::Key::F5,
            Key::F6 => egui::Key::F6,
            Key::F7 => egui::Key::F7,
            Key::F8 => egui::Key::F8,
            Key::F9 => egui::Key::F9,
            Key::F10 => egui::Key::F10,
            Key::F11 => egui::Key::F11,
            Key::F12 => egui::Key::F12,
            Key::ArrowUp => egui::Key::ArrowUp,
            Key::ArrowDown => egui::Key::ArrowDown,
            Key::ArrowLeft => egui::Key::ArrowLeft,
            Key::ArrowRight => egui::Key::ArrowRight,
            Key::Home => egui::Key::Home,
            Key::End => egui::Key::End,
            Key::PageUp => egui::Key::PageUp,
            Key::PageDown => egui::Key::PageDown,
            Key::Insert => egui::Key::Insert,
            Key::Delete => egui::Key::Delete,
            Key::Backspace => egui::Key::Backspace,
            Key::Enter => egui::Key::Enter,
            Key::Tab => egui::Key::Tab,
            Key::Escape => egui::Key::Escape,
            Key::Space => egui::Key::Space,
            Key::Comma => egui::Key::Comma,
            Key::Period => egui::Key::Period,
            Key::Slash => egui::Key::Slash,
            Key::Backslash => egui::Key::Backslash,
            Key::Semicolon => egui::Key::Semicolon,
            Key::Quote => egui::Key::Quote,
            Key::Backtick => egui::Key::Backtick,
            Key::Minus => egui::Key::Minus,
            Key::Equals => egui::Key::Equals,
            Key::LeftBracket => egui::Key::OpenBracket,
            Key::RightBracket => egui::Key::CloseBracket,
            // These keys don't exist in egui::Key, use closest approximation
            Key::PrintScreen => egui::Key::F13, // No direct equivalent
            Key::ScrollLock => egui::Key::F14,  // No direct equivalent
            Key::Pause => egui::Key::F15,       // No direct equivalent
        }
    }

    /// Convert from egui::Key (returns None for unsupported keys)
    pub fn from_egui(key: egui::Key) -> Option<Self> {
        match key {
            egui::Key::A => Some(Key::A),
            egui::Key::B => Some(Key::B),
            egui::Key::C => Some(Key::C),
            egui::Key::D => Some(Key::D),
            egui::Key::E => Some(Key::E),
            egui::Key::F => Some(Key::F),
            egui::Key::G => Some(Key::G),
            egui::Key::H => Some(Key::H),
            egui::Key::I => Some(Key::I),
            egui::Key::J => Some(Key::J),
            egui::Key::K => Some(Key::K),
            egui::Key::L => Some(Key::L),
            egui::Key::M => Some(Key::M),
            egui::Key::N => Some(Key::N),
            egui::Key::O => Some(Key::O),
            egui::Key::P => Some(Key::P),
            egui::Key::Q => Some(Key::Q),
            egui::Key::R => Some(Key::R),
            egui::Key::S => Some(Key::S),
            egui::Key::T => Some(Key::T),
            egui::Key::U => Some(Key::U),
            egui::Key::V => Some(Key::V),
            egui::Key::W => Some(Key::W),
            egui::Key::X => Some(Key::X),
            egui::Key::Y => Some(Key::Y),
            egui::Key::Z => Some(Key::Z),
            egui::Key::Num0 => Some(Key::Num0),
            egui::Key::Num1 => Some(Key::Num1),
            egui::Key::Num2 => Some(Key::Num2),
            egui::Key::Num3 => Some(Key::Num3),
            egui::Key::Num4 => Some(Key::Num4),
            egui::Key::Num5 => Some(Key::Num5),
            egui::Key::Num6 => Some(Key::Num6),
            egui::Key::Num7 => Some(Key::Num7),
            egui::Key::Num8 => Some(Key::Num8),
            egui::Key::Num9 => Some(Key::Num9),
            egui::Key::F1 => Some(Key::F1),
            egui::Key::F2 => Some(Key::F2),
            egui::Key::F3 => Some(Key::F3),
            egui::Key::F4 => Some(Key::F4),
            egui::Key::F5 => Some(Key::F5),
            egui::Key::F6 => Some(Key::F6),
            egui::Key::F7 => Some(Key::F7),
            egui::Key::F8 => Some(Key::F8),
            egui::Key::F9 => Some(Key::F9),
            egui::Key::F10 => Some(Key::F10),
            egui::Key::F11 => Some(Key::F11),
            egui::Key::F12 => Some(Key::F12),
            egui::Key::ArrowUp => Some(Key::ArrowUp),
            egui::Key::ArrowDown => Some(Key::ArrowDown),
            egui::Key::ArrowLeft => Some(Key::ArrowLeft),
            egui::Key::ArrowRight => Some(Key::ArrowRight),
            egui::Key::Home => Some(Key::Home),
            egui::Key::End => Some(Key::End),
            egui::Key::PageUp => Some(Key::PageUp),
            egui::Key::PageDown => Some(Key::PageDown),
            egui::Key::Insert => Some(Key::Insert),
            egui::Key::Delete => Some(Key::Delete),
            egui::Key::Backspace => Some(Key::Backspace),
            egui::Key::Enter => Some(Key::Enter),
            egui::Key::Tab => Some(Key::Tab),
            egui::Key::Escape => Some(Key::Escape),
            egui::Key::Space => Some(Key::Space),
            egui::Key::Comma => Some(Key::Comma),
            egui::Key::Period => Some(Key::Period),
            egui::Key::Slash => Some(Key::Slash),
            egui::Key::Backslash => Some(Key::Backslash),
            egui::Key::Semicolon => Some(Key::Semicolon),
            egui::Key::Quote => Some(Key::Quote),
            egui::Key::Backtick => Some(Key::Backtick),
            egui::Key::Minus => Some(Key::Minus),
            egui::Key::Equals => Some(Key::Equals),
            egui::Key::OpenBracket => Some(Key::LeftBracket),
            egui::Key::CloseBracket => Some(Key::RightBracket),
            // Special keys mapped to function keys in to_egui
            egui::Key::F13 => Some(Key::PrintScreen),
            egui::Key::F14 => Some(Key::ScrollLock),
            egui::Key::F15 => Some(Key::Pause),
            _ => None,  // Unsupported keys (e.g., Numpad)
        }
    }
}

/// Modifier keys with semantic command modifier
///
/// The `ctrl` field is **semantic**, not physical - it represents "the platform's primary
/// command modifier" (⌘ Command on macOS, Ctrl elsewhere). This enables writing one
/// shortcut definition that works on all platforms.
///
/// # Platform Awareness
///
/// - **macOS**: `ctrl: true` means ⌘ Command key
/// - **Windows/Linux**: `ctrl: true` means Control key
///
/// # Examples
///
/// ```
/// use liquers_lib::ui::shortcuts::Modifiers;
///
/// let cmd_only = Modifiers::command();
/// assert_eq!(cmd_only.ctrl, true);
/// assert_eq!(cmd_only.alt, false);
///
/// let empty = Modifiers::none();
/// assert!(empty.is_empty());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    /// Alt/Option key
    pub alt: bool,
    /// Shift key
    pub shift: bool,
    /// Semantic command modifier (Cmd on macOS, Ctrl elsewhere)
    pub ctrl: bool,
}

impl Modifiers {
    /// Empty modifiers (no keys pressed)
    pub const fn none() -> Self {
        Self {
            ctrl: false,
            alt: false,
            shift: false,
        }
    }

    /// Check if any modifiers are active
    pub const fn is_empty(&self) -> bool {
        !self.ctrl && !self.alt && !self.shift
    }

    /// Command modifier (convenience for ctrl: true)
    pub const fn command() -> Self {
        Self {
            ctrl: true,
            alt: false,
            shift: false,
        }
    }

    /// Convert to egui::Modifiers (WASM-safe)
    pub fn to_egui(&self) -> egui::Modifiers {
        egui::Modifiers {
            alt: self.alt,
            shift: self.shift,
            command: self.ctrl, // Semantic → semantic, egui handles platform
            ..Default::default()
        }
    }

    /// Convert from egui::Modifiers
    pub fn from_egui(m: egui::Modifiers) -> Self {
        Self {
            alt: m.alt,
            shift: m.shift,
            ctrl: m.command, // Semantic ← semantic
        }
    }
}

/// Platform-independent keyboard shortcut
///
/// Combines modifiers and a key to represent a keyboard shortcut that works
/// across all platforms. Uses semantic command modifier for true cross-platform support.
///
/// # Examples
///
/// ```
/// use liquers_lib::ui::shortcuts::{KeyboardShortcut, Modifiers, Key};
///
/// // Parse from string
/// let shortcut: KeyboardShortcut = "Ctrl+Shift+S".parse()?;
///
/// // Create programmatically
/// let save = KeyboardShortcut::new(Modifiers::command(), Key::S);
///
/// // Convert to egui for use in UI
/// let egui_shortcut = save.to_egui();
/// # Ok::<(), liquers_core::error::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyboardShortcut {
    pub modifiers: Modifiers,
    pub key: Key,
}

impl KeyboardShortcut {
    /// Create a new keyboard shortcut
    pub const fn new(modifiers: Modifiers, key: Key) -> Self {
        Self { modifiers, key }
    }

    /// Parse from string, returning error details
    pub fn parse(s: &str) -> Result<Self, Error> {
        s.parse()
    }

    /// Convert to egui::KeyboardShortcut
    pub fn to_egui(&self) -> egui::KeyboardShortcut {
        egui::KeyboardShortcut::new(self.modifiers.to_egui(), self.key.to_egui())
    }
}

impl From<egui::KeyboardShortcut> for KeyboardShortcut {
    fn from(shortcut: egui::KeyboardShortcut) -> Self {
        Self {
            modifiers: Modifiers::from_egui(shortcut.modifiers),
            key: Key::from_egui(shortcut.logical_key).unwrap_or(Key::Space), // Fallback for unsupported keys
        }
    }
}

impl FromStr for KeyboardShortcut {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(Error::general_error(
                "Empty shortcut string".to_string(),
            ));
        }

        let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();

        if parts.is_empty() {
            return Err(Error::general_error(
                "Empty shortcut string".to_string(),
            ));
        }

        let mut modifiers = Modifiers::none();
        let mut found_key: Option<Key> = None;

        // Name-based detection: check each token
        for part in parts {
            let normalized = part.to_lowercase();

            // Check if modifier
            match normalized.as_str() {
                "ctrl" | "cmd" | "command" | "meta" | "control" => {
                    modifiers.ctrl = true;
                    continue;
                }
                "alt" | "option" => {
                    modifiers.alt = true;
                    continue;
                }
                "shift" => {
                    modifiers.shift = true;
                    continue;
                }
                _ => {}
            }

            // Check if key
            if let Some(key) = Key::from_name(part) {
                if found_key.is_some() {
                    return Err(Error::general_error(format!(
                        "Multiple keys found: {:?} and {}",
                        found_key, part
                    )));
                }
                found_key = Some(key);
            } else {
                return Err(Error::general_error(format!(
                    "Unknown modifier or key: {}",
                    part
                )));
            }
        }

        match found_key {
            Some(key) => Ok(KeyboardShortcut::new(modifiers, key)),
            None => Err(Error::general_error(
                "No valid key found in shortcut".to_string(),
            )),
        }
    }
}

impl Display for KeyboardShortcut {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();

        // Modifier order: Ctrl, Alt, Shift
        if self.modifiers.ctrl {
            #[cfg(target_os = "macos")]
            parts.push("Cmd");
            #[cfg(not(target_os = "macos"))]
            parts.push("Ctrl");
        }

        if self.modifiers.alt {
            #[cfg(target_os = "macos")]
            parts.push("Option");
            #[cfg(not(target_os = "macos"))]
            parts.push("Alt");
        }

        if self.modifiers.shift {
            parts.push("Shift");
        }

        // Key last
        parts.push(self.key.name());

        write!(f, "{}", parts.join("+"))
    }
}

impl serde::Serialize for KeyboardShortcut {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for KeyboardShortcut {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

use std::collections::HashMap;

/// Detect duplicate shortcuts in a collection
///
/// Returns shortcuts that appear more than once with their counts. Note that
/// semantically equivalent shortcuts (e.g., "Ctrl+S" and "Cmd+S") are treated
/// as duplicates since they represent the same action.
///
/// # Examples
///
/// ```
/// use liquers_lib::ui::shortcuts::{KeyboardShortcut, find_conflicts};
///
/// let shortcuts = vec![
///     "Ctrl+S".parse()?,
///     "Cmd+S".parse()?,  // Conflicts with Ctrl+S (semantic equivalence)
///     "Alt+F4".parse()?,
/// ];
///
/// let conflicts = find_conflicts(&shortcuts);
/// assert_eq!(conflicts.len(), 1);
/// assert_eq!(conflicts[0].1, 2);  // Count of Ctrl+S / Cmd+S
/// # Ok::<(), liquers_core::error::Error>(())
/// ```
pub fn find_conflicts<'a, I>(shortcuts: I) -> Vec<(KeyboardShortcut, usize)>
where
    I: IntoIterator<Item = &'a KeyboardShortcut>,
{
    let mut counts: HashMap<KeyboardShortcut, usize> = HashMap::new();

    for shortcut in shortcuts {
        *counts.entry(shortcut.clone()).or_insert(0) += 1;
    }

    counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .collect()
}

/// Validate multiple shortcut strings
///
/// Parses each string and returns those that fail with error details.
/// Useful for batch validation of shortcuts from configuration files.
///
/// # Examples
///
/// ```
/// use liquers_lib::ui::shortcuts::validate_shortcut_strings;
///
/// let shortcuts = vec!["Ctrl+S", "Invalid+Foo", "Alt+F4"];
/// let errors = validate_shortcut_strings(shortcuts.iter().copied());
///
/// assert_eq!(errors.len(), 1);
/// assert_eq!(errors[0].0, "Invalid+Foo");
/// ```
pub fn validate_shortcut_strings<'a, I>(shortcuts: I) -> Vec<(String, Error)>
where
    I: IntoIterator<Item = &'a str>,
{
    shortcuts
        .into_iter()
        .filter_map(|s| match KeyboardShortcut::parse(s) {
            Ok(_) => None,
            Err(e) => Some((s.to_string(), e)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_from_name_basic() {
        assert_eq!(Key::from_name("A"), Some(Key::A));
        assert_eq!(Key::from_name("a"), Some(Key::A));
        assert_eq!(Key::from_name("KeyA"), Some(Key::A));
    }

    #[test]
    fn key_from_name_aliases() {
        assert_eq!(Key::from_name("Esc"), Some(Key::Escape));
        assert_eq!(Key::from_name("Escape"), Some(Key::Escape));
        assert_eq!(Key::from_name("Return"), Some(Key::Enter));
        assert_eq!(Key::from_name("Enter"), Some(Key::Enter));
    }

    #[test]
    fn key_from_name_unknown() {
        assert_eq!(Key::from_name("Foo"), None);
    }

    #[test]
    fn modifiers_constructors() {
        assert_eq!(
            Modifiers::none(),
            Modifiers {
                ctrl: false,
                alt: false,
                shift: false
            }
        );
        assert_eq!(
            Modifiers::command(),
            Modifiers {
                ctrl: true,
                alt: false,
                shift: false
            }
        );
        assert!(Modifiers::none().is_empty());
        assert!(!Modifiers::command().is_empty());
    }

    #[test]
    fn parse_simple_shortcut() -> Result<(), Box<dyn std::error::Error>> {
        let shortcut: KeyboardShortcut = "Ctrl+S".parse()?;
        assert_eq!(shortcut.modifiers.ctrl, true);
        assert_eq!(shortcut.key, Key::S);
        Ok(())
    }

    #[test]
    fn parse_cmd_as_ctrl() -> Result<(), Box<dyn std::error::Error>> {
        let shortcut: KeyboardShortcut = "Cmd+S".parse()?;
        assert_eq!(shortcut.modifiers.ctrl, true);
        assert_eq!(shortcut.key, Key::S);
        Ok(())
    }

    #[test]
    fn parse_multiple_modifiers() -> Result<(), Box<dyn std::error::Error>> {
        let shortcut: KeyboardShortcut = "Ctrl+Shift+Alt+A".parse()?;
        assert_eq!(shortcut.modifiers.ctrl, true);
        assert_eq!(shortcut.modifiers.shift, true);
        assert_eq!(shortcut.modifiers.alt, true);
        assert_eq!(shortcut.key, Key::A);
        Ok(())
    }

    #[test]
    fn parse_order_independent() -> Result<(), Box<dyn std::error::Error>> {
        let s1: KeyboardShortcut = "Ctrl+Shift+S".parse()?;
        let s2: KeyboardShortcut = "Shift+Ctrl+S".parse()?;
        assert_eq!(s1, s2);
        Ok(())
    }

    #[test]
    fn parse_empty_string() {
        let result = "".parse::<KeyboardShortcut>();
        assert!(result.is_err());
    }

    #[test]
    fn parse_unknown_key() {
        let result = "Ctrl+Foo".parse::<KeyboardShortcut>();
        assert!(result.is_err());
    }

    #[test]
    fn parse_unknown_modifier() {
        let result = "Foo+S".parse::<KeyboardShortcut>();
        assert!(result.is_err());
    }

    #[test]
    fn parse_no_key() {
        let result = "Ctrl+Shift".parse::<KeyboardShortcut>();
        assert!(result.is_err());
    }

    #[test]
    fn display_ctrl_shortcut() {
        let shortcut = KeyboardShortcut::new(Modifiers::command(), Key::S);
        let display = shortcut.to_string();
        // Platform-specific: Cmd on macOS, Ctrl elsewhere
        #[cfg(target_os = "macos")]
        assert_eq!(display, "Cmd+S");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(display, "Ctrl+S");
    }

    #[test]
    fn display_multiple_modifiers() {
        let shortcut = KeyboardShortcut::new(
            Modifiers {
                ctrl: true,
                alt: true,
                shift: true,
            },
            Key::A,
        );
        let display = shortcut.to_string();
        // Modifier order: Ctrl, Alt, Shift
        #[cfg(target_os = "macos")]
        assert_eq!(display, "Cmd+Option+Shift+A");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(display, "Ctrl+Alt+Shift+A");
    }

    #[test]
    fn serde_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let original = KeyboardShortcut::new(Modifiers::command(), Key::S);
        let json = serde_json::to_string(&original)?;
        let deserialized: KeyboardShortcut = serde_json::from_str(&json)?;
        assert_eq!(original, deserialized);
        Ok(())
    }

    #[test]
    fn find_conflicts_none() {
        let shortcuts = vec![
            KeyboardShortcut::new(Modifiers::command(), Key::S),
            KeyboardShortcut::new(Modifiers::command(), Key::Q),
        ];
        let conflicts = find_conflicts(shortcuts.iter());
        assert_eq!(conflicts.len(), 0);
    }

    #[test]
    fn find_conflicts_duplicates() {
        let shortcut = KeyboardShortcut::new(Modifiers::command(), Key::S);
        let shortcuts = vec![shortcut.clone(), shortcut.clone(), shortcut];
        let conflicts = find_conflicts(shortcuts.iter());
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].1, 3); // Count = 3
    }

    #[test]
    fn validate_shortcut_strings_all_valid() {
        let strings = vec!["Ctrl+S", "Alt+F4", "Shift+A"];
        let errors = validate_shortcut_strings(strings.iter().copied());
        assert_eq!(errors.len(), 0);
    }

    #[test]
    fn validate_shortcut_strings_some_invalid() {
        let strings = vec!["Ctrl+S", "Invalid+Foo", "Shift+A"];
        let errors = validate_shortcut_strings(strings.iter().copied());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, "Invalid+Foo");
    }

    #[test]
    fn egui_conversion_round_trip() {
        let original = KeyboardShortcut::new(
            Modifiers {
                ctrl: true,
                shift: true,
                alt: false,
            },
            Key::S,
        );
        let egui_shortcut = original.to_egui();
        let converted = KeyboardShortcut::from(egui_shortcut);
        assert_eq!(original.modifiers, converted.modifiers);
        // Key conversion might differ for unsupported keys, but S is supported
        assert_eq!(original.key, converted.key);
    }
}
