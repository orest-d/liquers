# Phase 4: Implementation Plan - keyboard-shortcuts

## Overview

**Feature:** keyboard-shortcuts

**Architecture:** Platform-independent keyboard shortcuts library in `liquers_lib::ui::shortcuts` module. Core types (KeyboardShortcut, Modifiers, Key) with semantic command modifier, string parsing/serialization, egui conversions, and conflict detection utilities.

**Estimated complexity:** Medium

**Estimated time:** 8-12 hours

**Prerequisites:**
- Phase 1, 2, 3 approved ✓
- All open questions resolved ✓
- Dependencies identified: All dependencies already present (liquers-core, egui, serde) ✓

## Implementation Steps

### Step 1: Create Core Module File with Key Enum

**File:** `liquers-lib/src/ui/shortcuts.rs`

**Action:**
- Create new file with module documentation
- Define `Key` enum with all 60+ variants (letters, numbers, function keys, navigation, editing, punctuation)
- Implement `Key::from_name()` for parsing (case-insensitive, with aliases)
- Implement `Key::name()` for canonical display names
- Implement `Key::to_egui()` conversion (maps each variant to egui::Key)
- Implement `Key::from_egui()` conversion (returns Option for unsupported keys)
- Derive `Debug, Clone, Copy, PartialEq, Eq, Hash`

**Code changes:**
```rust
// NEW: Add this file
//! Platform-independent keyboard shortcuts library
//!
//! Provides unified shortcut representation with semantic command modifier
//! support for cross-platform applications (macOS Cmd vs Windows/Linux Ctrl).

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;
use liquers_core::error::Error;

/// Key enum with all supported keys
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
            // ... (complete all variants with aliases)

            // Aliases
            "esc" | "escape" => Some(Key::Escape),
            "return" | "enter" => Some(Key::Enter),

            _ => None,
        }
    }

    /// Canonical name for display
    pub const fn name(&self) -> &'static str {
        match self {
            Key::A => "A",
            // ... (complete all variants)
            Key::Escape => "Escape",
            Key::Enter => "Enter",
            // ...
        }
    }

    /// Convert to egui::Key
    pub fn to_egui(&self) -> egui::Key {
        match self {
            Key::A => egui::Key::A,
            // ... (complete all variants)
        }
    }

    /// Convert from egui::Key (returns None for unsupported keys)
    pub fn from_egui(key: egui::Key) -> Option<Self> {
        match key {
            egui::Key::A => Some(Key::A),
            // ... (complete all variants)
            _ => None,  // Unsupported keys (e.g., Numpad)
        }
    }
}
```

**Validation:**
```bash
cargo check -p liquers-lib
```

**Rollback:**
```bash
rm liquers-lib/src/ui/shortcuts.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture (Key enum variants, function signatures), egui Key enum documentation
- **Rationale:** Straightforward enum definition with match arms - haiku sufficient for boilerplate

---

### Step 2: Add Modifiers Struct

**File:** `liquers-lib/src/ui/shortcuts.rs`

**Action:**
- Add `Modifiers` struct with 3 bool fields (ctrl, alt, shift)
- Derive `Debug, Clone, Copy, PartialEq, Eq, Hash, Default`
- Implement `Modifiers::none()`, `is_empty()`, `command()` constructors
- Implement `Modifiers::to_egui()` conversion (maps ctrl to command field)
- Implement `Modifiers::from_egui()` conversion (maps command to ctrl)

**Code changes:**
```rust
// NEW: Add to shortcuts.rs after Key enum

/// Modifier keys with semantic command modifier
///
/// The `ctrl` field is semantic: represents "the platform's primary command modifier"
/// (Cmd on macOS, Ctrl on Windows/Linux). This enables writing one shortcut definition
/// that works on all platforms.
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
        Self { ctrl: false, alt: false, shift: false }
    }

    /// Check if any modifiers are active
    pub const fn is_empty(&self) -> bool {
        !self.ctrl && !self.alt && !self.shift
    }

    /// Command modifier (convenience for ctrl: true)
    pub const fn command() -> Self {
        Self { ctrl: true, alt: false, shift: false }
    }

    /// Convert to egui::Modifiers (WASM-safe)
    pub fn to_egui(&self) -> egui::Modifiers {
        egui::Modifiers {
            alt: self.alt,
            shift: self.shift,
            command: self.ctrl,  // Semantic → semantic, egui handles platform
            ..Default::default()
        }
    }

    /// Convert from egui::Modifiers
    pub fn from_egui(m: egui::Modifiers) -> Self {
        Self {
            alt: m.alt,
            shift: m.shift,
            ctrl: m.command,  // Semantic ← semantic
        }
    }
}
```

**Validation:**
```bash
cargo check -p liquers-lib
```

**Rollback:**
```bash
git checkout liquers-lib/src/ui/shortcuts.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture (Modifiers struct, WASM safety rationale)
- **Rationale:** Simple struct with conversions - haiku sufficient

---

### Step 3: Add KeyboardShortcut Struct and Constructor

**File:** `liquers-lib/src/ui/shortcuts.rs`

**Action:**
- Add `KeyboardShortcut` struct with modifiers and key fields
- Derive `Debug, Clone, PartialEq, Eq, Hash`
- Implement `KeyboardShortcut::new()` constructor
- Implement `KeyboardShortcut::parse()` helper (delegates to FromStr)
- Implement `KeyboardShortcut::to_egui()` conversion

**Code changes:**
```rust
// NEW: Add to shortcuts.rs after Modifiers

/// Platform-independent keyboard shortcut
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
            key: Key::from_egui(shortcut.logical_key).unwrap_or(Key::Space),  // Fallback for unsupported keys
        }
    }
}
```

**Validation:**
```bash
cargo check -p liquers-lib
```

**Rollback:**
```bash
git checkout liquers-lib/src/ui/shortcuts.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture (KeyboardShortcut struct, egui conversions)
- **Rationale:** Simple struct and conversions - haiku sufficient

---

### Step 4: Implement FromStr for KeyboardShortcut (Parser)

**File:** `liquers-lib/src/ui/shortcuts.rs`

**Action:**
- Implement `FromStr` trait for `KeyboardShortcut`
- Parser logic: split on '+', name-based detection of modifiers vs key
- Semantic command modifier: "Ctrl", "Cmd", "Command", "Meta" → `ctrl: true`
- Case-insensitive, with key aliases
- Error handling via `Error::general_error()`

**Code changes:**
```rust
// NEW: Add to shortcuts.rs after KeyboardShortcut impl

impl FromStr for KeyboardShortcut {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(Error::general_error("Empty shortcut string".to_string()));
        }

        let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();

        if parts.is_empty() {
            return Err(Error::general_error("Empty shortcut string".to_string()));
        }

        let mut modifiers = Modifiers::none();
        let mut found_key: Option<Key> = None;

        // Name-based detection: check each token
        for part in parts {
            let normalized = part.to_lowercase();

            // Check if modifier
            match normalized.as_str() {
                "ctrl" | "cmd" | "command" | "meta" => {
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
                    return Err(Error::general_error(
                        format!("Multiple keys found: {:?} and {}", found_key, part)
                    ));
                }
                found_key = Some(key);
            } else {
                return Err(Error::general_error(
                    format!("Unknown modifier or key: {}", part)
                ));
            }
        }

        match found_key {
            Some(key) => Ok(KeyboardShortcut::new(modifiers, key)),
            None => Err(Error::general_error(
                "No valid key found in shortcut".to_string()
            )),
        }
    }
}
```

**Validation:**
```bash
cargo check -p liquers-lib
# Quick test
cargo test -p liquers-lib shortcut_parse  # Will add test in Step 9
```

**Rollback:**
```bash
git checkout liquers-lib/src/ui/shortcuts.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture (parsing rules, error handling), Phase 3 examples (parsing cases)
- **Rationale:** Parser logic requires careful error handling and edge cases - sonnet recommended

---

### Step 5: Implement Display for KeyboardShortcut (Formatter)

**File:** `liquers-lib/src/ui/shortcuts.rs`

**Action:**
- Implement `Display` trait for `KeyboardShortcut`
- Platform-aware display: `ctrl: true` displays as "Cmd" on macOS native, "Ctrl" elsewhere
- Modifier order: Ctrl, Alt, Shift
- Use compile-time cfg for native (acceptable fallback for WASM)

**Code changes:**
```rust
// NEW: Add to shortcuts.rs after FromStr impl

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
```

**Validation:**
```bash
cargo check -p liquers-lib
# Quick test
cargo test -p liquers-lib shortcut_display  # Will add test in Step 9
```

**Rollback:**
```bash
git checkout liquers-lib/src/ui/shortcuts.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture (display rules, platform-aware formatting)
- **Rationale:** Straightforward formatting logic - haiku sufficient

---

### Step 6: Implement Serde for KeyboardShortcut

**File:** `liquers-lib/src/ui/shortcuts.rs`

**Action:**
- Implement `Serialize` and `Deserialize` traits via string representation
- Delegates to `Display` and `FromStr`

**Code changes:**
```rust
// NEW: Add to shortcuts.rs after Display impl

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
```

**Validation:**
```bash
cargo check -p liquers-lib
# Quick test
cargo test -p liquers-lib shortcut_serde  # Will add test in Step 9
```

**Rollback:**
```bash
git checkout liquers-lib/src/ui/shortcuts.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture (serde implementation via string)
- **Rationale:** Standard serde pattern - haiku sufficient

---

### Step 7: Add Utility Functions (Conflict Detection)

**File:** `liquers-lib/src/ui/shortcuts.rs`

**Action:**
- Implement `find_conflicts()` function using HashMap
- Implement `validate_shortcut_strings()` function for parsing multiple shortcuts

**Code changes:**
```rust
// NEW: Add to shortcuts.rs after serde impl

use std::collections::HashMap;

/// Detect duplicate shortcuts in a collection
///
/// Returns shortcuts that appear more than once with their counts.
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

/// Helper for menu spec validation (used by ui_spec_element.rs)
///
/// Parse each string, return those that fail with error details.
pub fn validate_shortcut_strings<'a, I>(shortcuts: I) -> Vec<(String, Error)>
where
    I: IntoIterator<Item = &'a str>,
{
    shortcuts
        .into_iter()
        .filter_map(|s| {
            match KeyboardShortcut::parse(s) {
                Ok(_) => None,
                Err(e) => Some((s.to_string(), e)),
            }
        })
        .collect()
}
```

**Validation:**
```bash
cargo check -p liquers-lib
```

**Rollback:**
```bash
git checkout liquers-lib/src/ui/shortcuts.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture (utility function signatures), Phase 3 examples (conflict detection usage)
- **Rationale:** Simple HashMap operations - haiku sufficient

---

### Step 8: Export Module in ui/mod.rs

**File:** `liquers-lib/src/ui/mod.rs`

**Action:**
- Add `pub mod shortcuts;` declaration
- Re-export main types for convenience

**Code changes:**
```rust
// MODIFY: Add to existing ui/mod.rs

pub mod shortcuts;

// Re-exports
pub use shortcuts::{KeyboardShortcut, Modifiers, Key, find_conflicts, validate_shortcut_strings};
```

**Validation:**
```bash
cargo check -p liquers-lib
```

**Rollback:**
```bash
git restore liquers-lib/src/ui/mod.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture (exports)
- **Rationale:** Trivial module export - haiku sufficient

---

### Step 9: Add Unit Tests (Inline)

**File:** `liquers-lib/src/ui/shortcuts.rs`

**Action:**
- Add `#[cfg(test)] mod tests { ... }` at end of file
- Unit tests for:
  - Key::from_name() with aliases
  - Modifiers constructors and conversions
  - KeyboardShortcut parsing (valid, invalid, edge cases)
  - Display formatting (platform-aware)
  - Serde round-trip
  - Conflict detection
  - Error handling

**Code changes:**
```rust
// NEW: Add to end of shortcuts.rs

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
        assert_eq!(Modifiers::none(), Modifiers { ctrl: false, alt: false, shift: false });
        assert_eq!(Modifiers::command(), Modifiers { ctrl: true, alt: false, shift: false });
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
            Modifiers { ctrl: true, alt: true, shift: true },
            Key::A
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
        assert_eq!(conflicts[0].1, 3);  // Count = 3
    }

    #[test]
    fn validate_shortcut_strings_all_valid() {
        let strings = vec!["Ctrl+S", "Alt+F4", "Shift+A"];
        let errors = validate_shortcut_strings(strings.iter().map(|s| *s));
        assert_eq!(errors.len(), 0);
    }

    #[test]
    fn validate_shortcut_strings_some_invalid() {
        let strings = vec!["Ctrl+S", "Invalid+Foo", "Shift+A"];
        let errors = validate_shortcut_strings(strings.iter().map(|s| *s));
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, "Invalid+Foo");
    }

    #[test]
    fn egui_conversion_round_trip() {
        let original = KeyboardShortcut::new(
            Modifiers { ctrl: true, shift: true, alt: false },
            Key::S
        );
        let egui_shortcut = original.to_egui();
        let converted = KeyboardShortcut::from(egui_shortcut);
        assert_eq!(original.modifiers, converted.modifiers);
        // Key conversion might differ for unsupported keys, but S is supported
        assert_eq!(original.key, converted.key);
    }
}
```

**Validation:**
```bash
cargo test -p liquers-lib shortcuts::tests
```

**Rollback:**
```bash
git checkout liquers-lib/src/ui/shortcuts.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** Phase 3 test plan, liquers testing conventions
- **Rationale:** Comprehensive test suite with edge cases - sonnet recommended for thoroughness

---

### Step 10: Migrate ui_spec_element.rs to Use Shortcuts Library

**File:** `liquers-lib/src/ui/widgets/ui_spec_element.rs`

**Action:**
- Remove old `check_shortcut()` and `parse_key()` methods (lines 378-417)
- Replace with shortcuts library calls
- Update `MenuBarSpec::validate_shortcuts()` to use `find_conflicts()`
- Add `use crate::ui::shortcuts::KeyboardShortcut;` import

**Code changes:**
```rust
// MODIFY: Replace existing methods in ui_spec_element.rs

use crate::ui::shortcuts::KeyboardShortcut;

impl UISpecElement {
    // DELETE: remove check_shortcut and parse_key methods (lines 378-417)

    // Keep existing render_menu_bar logic but update shortcut checking:
    fn render_menu_bar(&mut self, ui: &mut egui::Ui, ui_context: &UIContext) {
        // ... existing code ...

        // MODIFY: Replace shortcut checking
        if let Some(shortcut_str) = &item.shortcut {
            match KeyboardShortcut::parse(shortcut_str) {
                Ok(shortcut) => {
                    let egui_shortcut = shortcut.to_egui();
                    if ui.input_mut(|i| i.consume_shortcut(&egui_shortcut)) {
                        // Execute action
                    }
                }
                Err(_) => {
                    // Invalid shortcut string - already validated, should not happen
                }
            }
        }

        // ... existing code ...
    }
}

impl MenuBarSpec {
    // MODIFY: Replace validate_shortcuts implementation
    pub fn validate_shortcuts(&self) -> Vec<(String, usize)> {
        use crate::ui::shortcuts::find_conflicts;

        // Extract shortcut strings (existing logic)
        let mut shortcut_strings = Vec::new();
        self.collect_shortcuts_recursive(&mut shortcut_strings);

        // Parse and detect conflicts
        let parsed: Vec<KeyboardShortcut> = shortcut_strings
            .iter()
            .filter_map(|s| KeyboardShortcut::parse(s).ok())
            .collect();

        let conflicts = find_conflicts(&parsed);
        conflicts
            .into_iter()
            .map(|(sc, count)| (sc.to_string(), count))
            .collect()
    }

    // Helper to collect shortcuts recursively (existing logic)
    fn collect_shortcuts_recursive(&self, shortcuts: &mut Vec<String>) {
        for item in &self.items {
            match item {
                TopLevelItem::Menu(menu) => {
                    Self::collect_menu_shortcuts(menu, shortcuts);
                }
                TopLevelItem::Button(button) => {
                    if let Some(shortcut) = &button.shortcut {
                        shortcuts.push(shortcut.clone());
                    }
                }
            }
        }
    }

    fn collect_menu_shortcuts(menu: &MenuSpec, shortcuts: &mut Vec<String>) {
        for item in &menu.items {
            match item {
                MenuItem::Button(button) => {
                    if let Some(shortcut) = &button.shortcut {
                        shortcuts.push(shortcut.clone());
                    }
                }
                MenuItem::Submenu(submenu) => {
                    Self::collect_menu_shortcuts(submenu, shortcuts);
                }
                MenuItem::Separator => {}
            }
        }
    }
}
```

**Validation:**
```bash
cargo check -p liquers-lib
cargo test -p liquers-lib ui_spec  # Run existing ui_spec tests
```

**Rollback:**
```bash
git restore liquers-lib/src/ui/widgets/ui_spec_element.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 integration section, existing ui_spec_element.rs code, UISpecElement architecture
- **Rationale:** Refactoring existing code with careful preservation of behavior - sonnet recommended

---

### Step 11: Add Integration Tests

**File:** `liquers-lib/tests/ui_shortcuts_integration.rs`

**Action:**
- Create new integration test file
- Test egui conversion round-trip
- Test YAML parsing with shortcuts (via UISpec)
- Test conflict detection with real menu specs

**Code changes:**
```rust
// NEW: Create liquers-lib/tests/ui_shortcuts_integration.rs

use liquers_lib::ui::shortcuts::{KeyboardShortcut, Modifiers, Key, find_conflicts};

#[test]
fn integration_parse_and_convert_to_egui() -> Result<(), Box<dyn std::error::Error>> {
    let shortcut: KeyboardShortcut = "Ctrl+Shift+S".parse()?;
    let egui_shortcut = shortcut.to_egui();

    // Verify egui conversion
    assert_eq!(egui_shortcut.modifiers.command, true);
    assert_eq!(egui_shortcut.modifiers.shift, true);
    assert_eq!(egui_shortcut.logical_key, egui::Key::S);

    Ok(())
}

#[test]
fn integration_yaml_with_shortcuts() -> Result<(), Box<dyn std::error::Error>> {
    let yaml = r#"
menu:
  items:
  - !button
    label: Save
    shortcut: "Ctrl+S"
    action: null
layout: !vertical
init: []
"#;

    use liquers_lib::ui::widgets::ui_spec_element::UISpec;
    let spec: UISpec = serde_yaml::from_str(yaml)?;

    // Verify YAML parsed correctly
    assert!(spec.menu.is_some());

    Ok(())
}

#[test]
fn integration_conflict_detection_with_menu() {
    // Create shortcuts that conflict
    let shortcuts = vec![
        "Ctrl+S".parse::<KeyboardShortcut>().unwrap(),
        "Cmd+S".parse::<KeyboardShortcut>().unwrap(),  // Same as Ctrl+S (semantic)
        "Alt+F4".parse::<KeyboardShortcut>().unwrap(),
    ];

    let conflicts = find_conflicts(&shortcuts);

    // Ctrl+S and Cmd+S are the same (semantic command modifier)
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].1, 2);  // Count = 2
}

#[test]
fn integration_platform_aware_display() {
    let shortcut = KeyboardShortcut::new(Modifiers::command(), Key::S);
    let display = shortcut.to_string();

    // Display varies by platform
    #[cfg(target_os = "macos")]
    assert_eq!(display, "Cmd+S");

    #[cfg(not(target_os = "macos"))]
    assert_eq!(display, "Ctrl+S");
}

#[test]
fn integration_parse_all_modifier_combinations() -> Result<(), Box<dyn std::error::Error>> {
    let test_cases = vec![
        "Ctrl+A",
        "Alt+A",
        "Shift+A",
        "Ctrl+Alt+A",
        "Ctrl+Shift+A",
        "Alt+Shift+A",
        "Ctrl+Alt+Shift+A",
    ];

    for case in test_cases {
        let shortcut: KeyboardShortcut = case.parse()?;
        // Verify it parses without error
        assert_eq!(shortcut.key, Key::A);
    }

    Ok(())
}
```

**Validation:**
```bash
cargo test -p liquers-lib --test ui_shortcuts_integration
```

**Rollback:**
```bash
rm liquers-lib/tests/ui_shortcuts_integration.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** Phase 3 integration test plan, liquers integration test patterns
- **Rationale:** Integration tests require understanding of cross-module interactions - sonnet recommended

---

### Step 12: Manual Validation

**Action:**
- Run all tests
- Check compilation with clippy
- Verify no warnings
- Test example app (ui_spec_demo.rs) with shortcuts

**Commands:**
```bash
# Run all tests
cargo test -p liquers-lib shortcuts
cargo test -p liquers-lib --test ui_shortcuts_integration

# Check with clippy
cargo clippy -p liquers-lib -- -D warnings

# Build example app
cargo build --example ui_spec_demo

# Run example app (manual test)
cargo run --example ui_spec_demo
```

**Expected results:**
- All tests pass
- No clippy warnings
- Example app runs and shortcuts work (Ctrl+Q to quit, etc.)

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Phase 4 validation commands
- **Rationale:** Running validation commands - haiku sufficient

---

### Step 13: Documentation Comments

**File:** `liquers-lib/src/ui/shortcuts.rs`

**Action:**
- Add comprehensive module-level documentation
- Document all public types, functions, and methods with `///` doc comments
- Add examples to doc comments
- Document semantic command modifier behavior clearly

**Code changes:**
```rust
// MODIFY: Add doc comments throughout shortcuts.rs

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
//! ```rust
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
//! ```rust
//! use liquers_lib::ui::shortcuts::KeyboardShortcut;
//!
//! let shortcut: KeyboardShortcut = "Ctrl+Shift+S".parse()?;
//! println!("{}", shortcut);  // "Cmd+Shift+S" on macOS, "Ctrl+Shift+S" elsewhere
//! # Ok::<(), liquers_core::error::Error>(())
//! ```
//!
//! ## Converting to egui
//! ```rust
//! use liquers_lib::ui::shortcuts::KeyboardShortcut;
//!
//! let shortcut: KeyboardShortcut = "Ctrl+Q".parse()?;
//! let egui_shortcut = shortcut.to_egui();
//! // Use with egui::Ui::input_mut(|i| i.consume_shortcut(&egui_shortcut))
//! # Ok::<(), liquers_core::error::Error>(())
//! ```
//!
//! ## Detecting conflicts
//! ```rust
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

// Add doc comments to all pub items:
// - Key enum and its methods
// - Modifiers struct and its methods
// - KeyboardShortcut struct and its methods
// - find_conflicts and validate_shortcut_strings functions
```

**Validation:**
```bash
cargo doc -p liquers-lib --no-deps --open
```

**Rollback:**
```bash
git checkout liquers-lib/src/ui/shortcuts.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture, Rust documentation best practices
- **Rationale:** Writing comprehensive documentation with examples - sonnet recommended

---

### Step 14: Update Project Documentation

**Files:**
- `liquers-lib/README.md` (if exists)
- `specs/keyboard-shortcuts/DESIGN.md` (update status)

**Action:**
- Mark Phase 4 as implemented in DESIGN.md
- Add brief mention of shortcuts module in liquers-lib README
- Update MEMORY.md with implementation notes

**Code changes:**
```markdown
# MODIFY: specs/keyboard-shortcuts/DESIGN.md
Status: Phase 4 - IMPLEMENTED ✓

# MODIFY: Add to auto memory
## Keyboard Shortcuts Library (Phase 1)
- **File**: `liquers-lib/src/ui/shortcuts.rs` (~600 lines)
- **Core types**: KeyboardShortcut, Modifiers, Key
- **Semantic command modifier**: `ctrl` field represents platform's primary key (Cmd on macOS, Ctrl elsewhere)
- **String format**: "Ctrl+S", "Cmd+Shift+A", etc.
- **Parsing**: name-based detection, case-insensitive, with key aliases
- **Error handling**: uses `liquers_core::error::Error::general_error()`
- **egui integration**: bidirectional conversions via to_egui()/from_egui()
- **Conflict detection**: find_conflicts(), validate_shortcut_strings()
- **WASM-safe**: delegates platform detection to egui
- **Serde support**: string-based serialization for YAML/JSON
- **Integration**: Used by UISpecElement (replaces old check_shortcut/parse_key methods)
- **Tests**: 20+ unit tests, 5 integration tests
```

**Validation:**
```bash
# Verify documentation is complete
git status
git diff
```

**Rollback:**
```bash
git restore specs/keyboard-shortcuts/DESIGN.md
git restore ~/.claude/projects/-home-orest-zlos-rust-liquers/memory/MEMORY.md
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** None
- **Knowledge:** Phase 1-4 documents
- **Rationale:** Simple documentation updates - haiku sufficient

---

## Testing Plan

### Unit Tests
**When to run:** After Steps 9, 10
**File paths:** `liquers-lib/src/ui/shortcuts.rs` (inline tests)
**Commands:**
```bash
cargo test -p liquers-lib shortcuts::tests
```
**Coverage:** 20+ tests covering parsing, display, serde, conversions, error handling, conflict detection

### Integration Tests
**When to run:** After Step 11
**File paths:** `liquers-lib/tests/ui_shortcuts_integration.rs`
**Commands:**
```bash
cargo test -p liquers-lib --test ui_shortcuts_integration
```
**Coverage:** 5 tests covering egui round-trip, YAML integration, cross-module conflict detection, platform-aware display

### Manual Validation
**When to run:** After Step 12
**Commands:**
```bash
cargo clippy -p liquers-lib -- -D warnings
cargo run --example ui_spec_demo
# Test shortcuts: Ctrl+Q (quit), Ctrl+S (save), etc.
```
**Expected outputs:** No warnings, example app responds to shortcuts correctly

---

## Agent Assignment Summary

| Step | Model | Skills | Rationale |
|------|-------|--------|-----------|
| 1 | haiku | rust-best-practices | Boilerplate enum definition |
| 2 | haiku | rust-best-practices | Simple struct with conversions |
| 3 | haiku | rust-best-practices | Struct and basic methods |
| 4 | sonnet | rust-best-practices | Parser logic with error handling |
| 5 | haiku | rust-best-practices | Straightforward formatting |
| 6 | haiku | rust-best-practices | Standard serde pattern |
| 7 | haiku | rust-best-practices | HashMap operations |
| 8 | haiku | rust-best-practices | Trivial module export |
| 9 | sonnet | rust-best-practices, liquers-unittest | Comprehensive test suite |
| 10 | sonnet | rust-best-practices | Refactoring existing code |
| 11 | sonnet | rust-best-practices, liquers-unittest | Integration tests |
| 12 | haiku | rust-best-practices | Running validation commands |
| 13 | sonnet | rust-best-practices | Documentation with examples |
| 14 | haiku | None | Simple doc updates |

---

## Rollback Plan

### Per-Step Rollback
Each step includes specific rollback instructions. Generally:
- **New files**: `rm <file-path>`
- **Modified files**: `git restore <file-path>` or `git checkout <file-path>`

### Full Feature Rollback
If the entire feature needs to be rolled back:
```bash
# Remove new files
rm liquers-lib/src/ui/shortcuts.rs
rm liquers-lib/tests/ui_shortcuts_integration.rs

# Restore modified files
git restore liquers-lib/src/ui/mod.rs
git restore liquers-lib/src/ui/widgets/ui_spec_element.rs
git restore specs/keyboard-shortcuts/DESIGN.md

# Verify clean state
cargo check -p liquers-lib
cargo test -p liquers-lib ui_spec
```

**Important:** If ui_spec_element.rs has been modified, the old `check_shortcut` and `parse_key` methods will be restored, and functionality will revert to the pre-shortcuts-library state.

---

## Documentation Updates

### CLAUDE.md
No updates needed - shortcuts module follows existing patterns (no new value types, no commands, no storage).

### PROJECT_OVERVIEW.md
No updates needed - this is a UI utility module, not a core architecture change.

### MEMORY.md
Add shortcuts library section (see Step 14).

### README.md
Brief mention of shortcuts module in liquers-lib section (if README exists).

---

## Execution Options

1. **Execute now** - Implement the plan step-by-step
2. **Create task list** - Convert to tasks for later execution
3. **Revise plan** - Return to Phase 4 for modifications
4. **Exit** - User will implement manually
