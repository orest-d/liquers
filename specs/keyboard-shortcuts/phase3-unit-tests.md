# Phase 3: Unit Tests - Keyboard Shortcuts Library

Comprehensive unit test cases for the keyboard shortcuts library covering all core functionality: parsing, display, modifiers, keys, error handling, and serialization.

## Test Structure Overview

**Location:** `liquers-lib/src/ui/shortcuts.rs` (inline unit tests)

**Module structure:**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    // ... individual test functions
}
```

**Test runner:**
```bash
cargo test -p liquers-lib ui::shortcuts --lib
```

---

## Unit Tests

### Parsing Tests

#### 1. Parse Valid Single Modifier + Key

```rust
#[test]
fn parse_valid_ctrl_s() {
    let shortcut = KeyboardShortcut::parse("Ctrl+S").unwrap();
    assert_eq!(shortcut.modifiers.ctrl, true);
    assert_eq!(shortcut.modifiers.alt, false);
    assert_eq!(shortcut.modifiers.shift, false);
    assert_eq!(shortcut.key, Key::S);
}

#[test]
fn parse_valid_alt_f4() {
    let shortcut = KeyboardShortcut::parse("Alt+F4").unwrap();
    assert_eq!(shortcut.modifiers.alt, true);
    assert_eq!(shortcut.modifiers.ctrl, false);
    assert_eq!(shortcut.modifiers.shift, false);
    assert_eq!(shortcut.key, Key::F4);
}

#[test]
fn parse_valid_shift_escape() {
    let shortcut = KeyboardShortcut::parse("Shift+Escape").unwrap();
    assert_eq!(shortcut.modifiers.shift, true);
    assert_eq!(shortcut.modifiers.ctrl, false);
    assert_eq!(shortcut.modifiers.alt, false);
    assert_eq!(shortcut.key, Key::Escape);
}
```

#### 2. Parse Multiple Modifiers

```rust
#[test]
fn parse_ctrl_shift_s() {
    let shortcut = KeyboardShortcut::parse("Ctrl+Shift+S").unwrap();
    assert_eq!(shortcut.modifiers.ctrl, true);
    assert_eq!(shortcut.modifiers.shift, true);
    assert_eq!(shortcut.modifiers.alt, false);
    assert_eq!(shortcut.key, Key::S);
}

#[test]
fn parse_alt_shift_delete() {
    let shortcut = KeyboardShortcut::parse("Alt+Shift+Delete").unwrap();
    assert_eq!(shortcut.modifiers.alt, true);
    assert_eq!(shortcut.modifiers.shift, true);
    assert_eq!(shortcut.modifiers.ctrl, false);
    assert_eq!(shortcut.key, Key::Delete);
}

#[test]
fn parse_ctrl_alt_shift_enter() {
    let shortcut = KeyboardShortcut::parse("Ctrl+Alt+Shift+Enter").unwrap();
    assert_eq!(shortcut.modifiers.ctrl, true);
    assert_eq!(shortcut.modifiers.alt, true);
    assert_eq!(shortcut.modifiers.shift, true);
    assert_eq!(shortcut.key, Key::Enter);
}
```

#### 3. Parse Case-Insensitive Modifiers

```rust
#[test]
fn parse_lowercase_ctrl() {
    let shortcut1 = KeyboardShortcut::parse("Ctrl+S").unwrap();
    let shortcut2 = KeyboardShortcut::parse("ctrl+S").unwrap();
    let shortcut3 = KeyboardShortcut::parse("CTRL+S").unwrap();
    assert_eq!(shortcut1, shortcut2);
    assert_eq!(shortcut2, shortcut3);
}

#[test]
fn parse_mixed_case_modifiers() {
    let shortcut1 = KeyboardShortcut::parse("Ctrl+Shift+S").unwrap();
    let shortcut2 = KeyboardShortcut::parse("ctrl+SHIFT+s").unwrap();
    assert_eq!(shortcut1, shortcut2);
}

#[test]
fn parse_mixed_case_alt() {
    let shortcut1 = KeyboardShortcut::parse("Alt+F4").unwrap();
    let shortcut2 = KeyboardShortcut::parse("alt+F4").unwrap();
    let shortcut3 = KeyboardShortcut::parse("ALT+f4").unwrap();
    assert_eq!(shortcut1, shortcut2);
    assert_eq!(shortcut2, shortcut3);
}
```

#### 4. Parse Semantic Command Modifier Aliases

```rust
#[test]
fn parse_cmd_equals_ctrl() {
    let shortcut1 = KeyboardShortcut::parse("Ctrl+S").unwrap();
    let shortcut2 = KeyboardShortcut::parse("Cmd+S").unwrap();
    let shortcut3 = KeyboardShortcut::parse("Command+S").unwrap();

    // All should have ctrl: true
    assert_eq!(shortcut1.modifiers.ctrl, true);
    assert_eq!(shortcut2.modifiers.ctrl, true);
    assert_eq!(shortcut3.modifiers.ctrl, true);

    // All should be equal
    assert_eq!(shortcut1, shortcut2);
    assert_eq!(shortcut2, shortcut3);
}

#[test]
fn parse_cmd_uppercase_variants() {
    let shortcut1 = KeyboardShortcut::parse("CMD+S").unwrap();
    let shortcut2 = KeyboardShortcut::parse("cmd+s").unwrap();
    let shortcut3 = KeyboardShortcut::parse("Command+S").unwrap();
    let shortcut4 = KeyboardShortcut::parse("COMMAND+s").unwrap();

    assert_eq!(shortcut1, shortcut2);
    assert_eq!(shortcut2, shortcut3);
    assert_eq!(shortcut3, shortcut4);
}
```

#### 5. Parse Modifier Order Independence

```rust
#[test]
fn parse_modifier_order_independent() {
    let shortcut1 = KeyboardShortcut::parse("Ctrl+Shift+S").unwrap();
    let shortcut2 = KeyboardShortcut::parse("Shift+Ctrl+S").unwrap();
    assert_eq!(shortcut1, shortcut2);
}

#[test]
fn parse_modifier_order_alt_shift_ctrl() {
    let shortcut1 = KeyboardShortcut::parse("Ctrl+Alt+Shift+A").unwrap();
    let shortcut2 = KeyboardShortcut::parse("Alt+Shift+Ctrl+A").unwrap();
    let shortcut3 = KeyboardShortcut::parse("Shift+Ctrl+Alt+A").unwrap();

    assert_eq!(shortcut1, shortcut2);
    assert_eq!(shortcut2, shortcut3);
}
```

#### 6. Parse Key Aliases

```rust
#[test]
fn parse_key_escape_alias() {
    let shortcut1 = KeyboardShortcut::parse("Ctrl+Escape").unwrap();
    let shortcut2 = KeyboardShortcut::parse("Ctrl+Esc").unwrap();
    assert_eq!(shortcut1.key, shortcut2.key);
    assert_eq!(shortcut1.key, Key::Escape);
}

#[test]
fn parse_key_enter_alias() {
    let shortcut1 = KeyboardShortcut::parse("Shift+Enter").unwrap();
    let shortcut2 = KeyboardShortcut::parse("Shift+Return").unwrap();
    assert_eq!(shortcut1.key, shortcut2.key);
    assert_eq!(shortcut1.key, Key::Enter);
}

#[test]
fn parse_web_format_keya() {
    // Support web format: "KeyA", "Key0", etc.
    let shortcut1 = KeyboardShortcut::parse("Ctrl+KeyA").unwrap();
    let shortcut2 = KeyboardShortcut::parse("Ctrl+A").unwrap();
    assert_eq!(shortcut1.key, shortcut2.key);
    assert_eq!(shortcut1.key, Key::A);
}

#[test]
fn parse_web_format_numpad() {
    let shortcut = KeyboardShortcut::parse("Ctrl+Digit5").unwrap();
    assert_eq!(shortcut.key, Key::Num5);
}
```

#### 7. Parse Error Cases

```rust
#[test]
fn parse_empty_string_returns_error() {
    let result = KeyboardShortcut::parse("");
    assert!(matches!(result, Err(ShortcutParseError::EmptyString)));
}

#[test]
fn parse_unknown_key_returns_error() {
    let result = KeyboardShortcut::parse("Ctrl+UnknownKey");
    assert!(matches!(result, Err(ShortcutParseError::UnknownKey(_))));
}

#[test]
fn parse_unknown_modifier_returns_error() {
    let result = KeyboardShortcut::parse("Super+S");
    assert!(matches!(result, Err(ShortcutParseError::UnknownModifier(_))));
}

#[test]
fn parse_no_plus_separator_returns_error() {
    let result = KeyboardShortcut::parse("CtrlS");
    assert!(matches!(result, Err(ShortcutParseError::InvalidFormat(_))));
}

#[test]
fn parse_only_modifier_no_key_returns_error() {
    let result = KeyboardShortcut::parse("Ctrl");
    assert!(matches!(result, Err(ShortcutParseError::InvalidFormat(_))));
}

#[test]
fn parse_only_plus_returns_error() {
    let result = KeyboardShortcut::parse("+");
    assert!(matches!(result, Err(ShortcutParseError::InvalidFormat(_))));
}

#[test]
fn parse_trailing_plus_returns_error() {
    let result = KeyboardShortcut::parse("Ctrl+S+");
    assert!(matches!(result, Err(ShortcutParseError::InvalidFormat(_))));
}

#[test]
fn parse_leading_plus_returns_error() {
    let result = KeyboardShortcut::parse("+Ctrl+S");
    assert!(matches!(result, Err(ShortcutParseError::InvalidFormat(_))));
}
```

---

### Display Tests

#### 8. Display Format Correctness (Platform-Independent)

```rust
#[test]
fn display_single_key_no_modifiers() {
    let shortcut = KeyboardShortcut::new(Modifiers::none(), Key::S);
    let displayed = shortcut.to_string();
    assert_eq!(displayed, "S");
}

#[test]
fn display_ctrl_key() {
    let shortcut = KeyboardShortcut::new(Modifiers::command(), Key::S);
    let displayed = shortcut.to_string();

    // Platform-specific display: "Cmd+S" on macOS, "Ctrl+S" elsewhere
    #[cfg(target_os = "macos")]
    {
        assert_eq!(displayed, "Cmd+S");
    }
    #[cfg(not(target_os = "macos"))]
    {
        assert_eq!(displayed, "Ctrl+S");
    }
}

#[test]
fn display_alt_key() {
    let modifiers = Modifiers { alt: true, ..Modifiers::none() };
    let shortcut = KeyboardShortcut::new(modifiers, Key::F4);
    assert_eq!(shortcut.to_string(), "Alt+F4");
}

#[test]
fn display_shift_key() {
    let modifiers = Modifiers { shift: true, ..Modifiers::none() };
    let shortcut = KeyboardShortcut::new(modifiers, Key::Tab);
    assert_eq!(shortcut.to_string(), "Shift+Tab");
}

#[test]
fn display_multiple_modifiers_canonical_order() {
    let modifiers = Modifiers {
        ctrl: true,
        alt: true,
        shift: true,
    };
    let shortcut = KeyboardShortcut::new(modifiers, Key::S);
    let displayed = shortcut.to_string();

    // Order should be: Ctrl/Cmd, Alt, Shift (platform-aware)
    #[cfg(target_os = "macos")]
    {
        assert!(displayed.contains("Cmd"));
    }
    #[cfg(not(target_os = "macos"))]
    {
        assert!(displayed.contains("Ctrl"));
    }
    assert!(displayed.contains("Alt"));
    assert!(displayed.contains("Shift"));
    assert!(displayed.ends_with("S"));
}
```

#### 9. Display Canonical Key Names

```rust
#[test]
fn display_escape_not_esc() {
    let shortcut = KeyboardShortcut::new(Modifiers::none(), Key::Escape);
    assert_eq!(shortcut.to_string(), "Escape");
}

#[test]
fn display_enter_not_return() {
    let shortcut = KeyboardShortcut::new(Modifiers::none(), Key::Enter);
    assert_eq!(shortcut.to_string(), "Enter");
}

#[test]
fn display_arrow_keys() {
    let up = KeyboardShortcut::new(Modifiers::none(), Key::ArrowUp);
    let down = KeyboardShortcut::new(Modifiers::none(), Key::ArrowDown);
    let left = KeyboardShortcut::new(Modifiers::none(), Key::ArrowLeft);
    let right = KeyboardShortcut::new(Modifiers::none(), Key::ArrowRight);

    assert_eq!(up.to_string(), "ArrowUp");
    assert_eq!(down.to_string(), "ArrowDown");
    assert_eq!(left.to_string(), "ArrowLeft");
    assert_eq!(right.to_string(), "ArrowRight");
}

#[test]
fn display_function_keys() {
    let f1 = KeyboardShortcut::new(Modifiers::none(), Key::F1);
    let f12 = KeyboardShortcut::new(Modifiers::none(), Key::F12);

    assert_eq!(f1.to_string(), "F1");
    assert_eq!(f12.to_string(), "F12");
}
```

#### 10. Round-Trip Parsing and Display

```rust
#[test]
fn roundtrip_parse_display_single_key() {
    let original = "Escape";
    let shortcut: KeyboardShortcut = original.parse().unwrap();
    let displayed = shortcut.to_string();

    // Display should preserve semantic meaning
    assert_eq!(shortcut, displayed.parse().unwrap());
}

#[test]
fn roundtrip_parse_display_with_modifiers() {
    let original = "Ctrl+Shift+S";
    let shortcut: KeyboardShortcut = original.parse().unwrap();
    let displayed = shortcut.to_string();

    // Round-trip preserves meaning (not exact string on macOS)
    let reparsed: KeyboardShortcut = displayed.parse().unwrap();
    assert_eq!(shortcut, reparsed);
}

#[test]
fn roundtrip_cmd_normalizes_to_ctrl_on_nonmacos() {
    let original = "Cmd+S";
    let shortcut: KeyboardShortcut = original.parse().unwrap();
    let displayed = shortcut.to_string();

    // Both parse to same internal representation
    let cmd_parsed: KeyboardShortcut = "Cmd+S".parse().unwrap();
    let ctrl_parsed: KeyboardShortcut = "Ctrl+S".parse().unwrap();
    assert_eq!(cmd_parsed, ctrl_parsed);
}

#[test]
fn roundtrip_modifier_order_normalized() {
    let shortcut1: KeyboardShortcut = "Shift+Ctrl+A".parse().unwrap();
    let shortcut2: KeyboardShortcut = "Ctrl+Shift+A".parse().unwrap();

    // After round-trip, canonical order applied
    let displayed1 = shortcut1.to_string();
    let displayed2 = shortcut2.to_string();

    // Both normalize to same order
    assert_eq!(displayed1, displayed2);
    assert_eq!(shortcut1, shortcut2);
}
```

---

### Modifiers Tests

#### 11. Modifier Helper Methods

```rust
#[test]
fn modifiers_none_creates_empty() {
    let modifiers = Modifiers::none();
    assert_eq!(modifiers.alt, false);
    assert_eq!(modifiers.shift, false);
    assert_eq!(modifiers.ctrl, false);
}

#[test]
fn modifiers_is_empty_returns_true_for_none() {
    let modifiers = Modifiers::none();
    assert!(modifiers.is_empty());
}

#[test]
fn modifiers_is_empty_returns_false_with_ctrl() {
    let modifiers = Modifiers { ctrl: true, ..Modifiers::none() };
    assert!(!modifiers.is_empty());
}

#[test]
fn modifiers_is_empty_returns_false_with_alt() {
    let modifiers = Modifiers { alt: true, ..Modifiers::none() };
    assert!(!modifiers.is_empty());
}

#[test]
fn modifiers_is_empty_returns_false_with_shift() {
    let modifiers = Modifiers { shift: true, ..Modifiers::none() };
    assert!(!modifiers.is_empty());
}

#[test]
fn modifiers_command_creates_ctrl_true() {
    let modifiers = Modifiers::command();
    assert_eq!(modifiers.ctrl, true);
    assert_eq!(modifiers.alt, false);
    assert_eq!(modifiers.shift, false);
}

#[test]
fn modifiers_default_is_empty() {
    let modifiers = Modifiers::default();
    assert!(modifiers.is_empty());
    assert_eq!(modifiers, Modifiers::none());
}

#[test]
fn modifiers_copy_semantics() {
    let m1 = Modifiers::command();
    let m2 = m1; // Copy, not move

    // Both are valid
    assert_eq!(m1.ctrl, true);
    assert_eq!(m2.ctrl, true);
}
```

#### 12. Modifiers Equality and Hashing

```rust
#[test]
fn modifiers_equality() {
    let m1 = Modifiers { ctrl: true, alt: false, shift: false };
    let m2 = Modifiers { ctrl: true, alt: false, shift: false };
    assert_eq!(m1, m2);
}

#[test]
fn modifiers_inequality() {
    let m1 = Modifiers { ctrl: true, alt: false, shift: false };
    let m2 = Modifiers { ctrl: false, alt: true, shift: false };
    assert_ne!(m1, m2);
}

#[test]
fn modifiers_hashable_in_map() {
    use std::collections::HashMap;

    let mut map = HashMap::new();
    let m1 = Modifiers { ctrl: true, ..Modifiers::none() };
    let m2 = Modifiers { ctrl: true, ..Modifiers::none() };

    map.insert(m1, "value1");
    assert_eq!(map.get(&m2), Some(&"value1"));
}
```

---

### Key Tests

#### 13. Key::from_name Parsing

```rust
#[test]
fn key_from_name_uppercase_letters() {
    assert_eq!(Key::from_name("A"), Some(Key::A));
    assert_eq!(Key::from_name("Z"), Some(Key::Z));
}

#[test]
fn key_from_name_lowercase_letters() {
    assert_eq!(Key::from_name("a"), Some(Key::A));
    assert_eq!(Key::from_name("z"), Some(Key::Z));
}

#[test]
fn key_from_name_numbers() {
    assert_eq!(Key::from_name("0"), Some(Key::Num0));
    assert_eq!(Key::from_name("9"), Some(Key::Num9));
}

#[test]
fn key_from_name_digit_format() {
    assert_eq!(Key::from_name("Digit0"), Some(Key::Num0));
    assert_eq!(Key::from_name("Digit5"), Some(Key::Num5));
}

#[test]
fn key_from_name_function_keys() {
    assert_eq!(Key::from_name("F1"), Some(Key::F1));
    assert_eq!(Key::from_name("F12"), Some(Key::F12));
}

#[test]
fn key_from_name_escape_aliases() {
    assert_eq!(Key::from_name("Escape"), Some(Key::Escape));
    assert_eq!(Key::from_name("Esc"), Some(Key::Escape));
    assert_eq!(Key::from_name("escape"), Some(Key::Escape));
}

#[test]
fn key_from_name_enter_aliases() {
    assert_eq!(Key::from_name("Enter"), Some(Key::Enter));
    assert_eq!(Key::from_name("Return"), Some(Key::Enter));
    assert_eq!(Key::from_name("enter"), Some(Key::Enter));
}

#[test]
fn key_from_name_arrow_keys() {
    assert_eq!(Key::from_name("ArrowUp"), Some(Key::ArrowUp));
    assert_eq!(Key::from_name("ArrowDown"), Some(Key::ArrowDown));
    assert_eq!(Key::from_name("ArrowLeft"), Some(Key::ArrowLeft));
    assert_eq!(Key::from_name("ArrowRight"), Some(Key::ArrowRight));
}

#[test]
fn key_from_name_unknown_returns_none() {
    assert_eq!(Key::from_name("UnknownKey"), None);
    assert_eq!(Key::from_name("Foo"), None);
}

#[test]
fn key_from_name_case_insensitive() {
    assert_eq!(Key::from_name("ESCAPE"), Some(Key::Escape));
    assert_eq!(Key::from_name("escape"), Some(Key::Escape));
    assert_eq!(Key::from_name("Escape"), Some(Key::Escape));
}
```

#### 14. Key::name() Canonical Names

```rust
#[test]
fn key_name_returns_canonical() {
    assert_eq!(Key::A.name(), "A");
    assert_eq!(Key::S.name(), "S");
    assert_eq!(Key::Escape.name(), "Escape");
    assert_eq!(Key::Enter.name(), "Enter");
}

#[test]
fn key_name_function_keys() {
    assert_eq!(Key::F1.name(), "F1");
    assert_eq!(Key::F12.name(), "F12");
}

#[test]
fn key_name_numbers() {
    assert_eq!(Key::Num0.name(), "0");
    assert_eq!(Key::Num5.name(), "5");
    assert_eq!(Key::Num9.name(), "9");
}

#[test]
fn key_name_arrow_keys() {
    assert_eq!(Key::ArrowUp.name(), "ArrowUp");
    assert_eq!(Key::ArrowDown.name(), "ArrowDown");
    assert_eq!(Key::ArrowLeft.name(), "ArrowLeft");
    assert_eq!(Key::ArrowRight.name(), "ArrowRight");
}

#[test]
fn key_name_special_keys() {
    assert_eq!(Key::Space.name(), "Space");
    assert_eq!(Key::Tab.name(), "Tab");
    assert_eq!(Key::Backspace.name(), "Backspace");
    assert_eq!(Key::Delete.name(), "Delete");
}

#[test]
fn key_name_punctuation() {
    assert_eq!(Key::Comma.name(), "Comma");
    assert_eq!(Key::Period.name(), "Period");
    assert_eq!(Key::Slash.name(), "Slash");
}

#[test]
fn key_name_roundtrip_with_from_name() {
    let keys = vec![
        Key::A, Key::S, Key::Escape, Key::Enter,
        Key::F1, Key::ArrowUp, Key::Space, Key::Comma
    ];

    for key in keys {
        let name = key.name();
        let parsed = Key::from_name(name).expect("Should parse back");
        assert_eq!(parsed, key, "Round-trip failed for {}", name);
    }
}
```

#### 15. Key Copy Semantics and Equality

```rust
#[test]
fn key_copy_semantics() {
    let k1 = Key::S;
    let k2 = k1; // Copy, not move

    assert_eq!(k1, k2);
}

#[test]
fn key_equality() {
    assert_eq!(Key::A, Key::A);
    assert_ne!(Key::A, Key::B);
}

#[test]
fn key_hashable_in_map() {
    use std::collections::HashMap;

    let mut map = HashMap::new();
    map.insert(Key::S, "Save");
    map.insert(Key::Q, "Quit");

    assert_eq!(map.get(&Key::S), Some(&"Save"));
    assert_eq!(map.get(&Key::Q), Some(&"Quit"));
}
```

---

### Error Tests

#### 16. All ShortcutParseError Variants

```rust
#[test]
fn error_empty_string_displays() {
    let err = ShortcutParseError::EmptyString;
    let msg = err.to_string();
    assert!(msg.to_lowercase().contains("empty"));
}

#[test]
fn error_unknown_key_displays_key_name() {
    let err = ShortcutParseError::UnknownKey("Foo".to_string());
    let msg = err.to_string();
    assert!(msg.contains("Foo"));
    assert!(msg.to_lowercase().contains("unknown"));
}

#[test]
fn error_unknown_modifier_displays_modifier_name() {
    let err = ShortcutParseError::UnknownModifier("Super".to_string());
    let msg = err.to_string();
    assert!(msg.contains("Super"));
    assert!(msg.to_lowercase().contains("unknown"));
}

#[test]
fn error_invalid_format_displays_context() {
    let err = ShortcutParseError::InvalidFormat("CtrlS".to_string());
    let msg = err.to_string();
    assert!(msg.contains("CtrlS") || msg.to_lowercase().contains("format"));
}

#[test]
fn error_types_are_distinct() {
    let err1 = ShortcutParseError::EmptyString;
    let err2 = ShortcutParseError::UnknownKey("A".to_string());
    let err3 = ShortcutParseError::UnknownModifier("Ctrl".to_string());
    let err4 = ShortcutParseError::InvalidFormat("test".to_string());

    // Each error variant is different from others
    assert_ne!(err1.to_string(), err2.to_string());
    assert_ne!(err2.to_string(), err3.to_string());
    assert_ne!(err3.to_string(), err4.to_string());
}

#[test]
fn error_clone() {
    let err = ShortcutParseError::UnknownKey("Test".to_string());
    let err_clone = err.clone();
    assert_eq!(err.to_string(), err_clone.to_string());
}

#[test]
fn error_debug_impl() {
    let err = ShortcutParseError::EmptyString;
    let debug_str = format!("{:?}", err);
    assert!(debug_str.contains("EmptyString"));
}
```

---

### Serialization Tests

#### 17. Serialize and Deserialize KeyboardShortcut

```rust
#[test]
fn serialize_keyboard_shortcut_to_string() {
    use serde::Serialize;

    let shortcut = KeyboardShortcut::parse("Ctrl+S").unwrap();

    // Using JSON serialization as example
    let json = serde_json::to_string(&shortcut).unwrap();
    assert!(json.contains("Ctrl") || json.contains("Cmd")); // Platform-aware
    assert!(json.contains("S"));
}

#[test]
fn deserialize_keyboard_shortcut_from_string() {
    use serde::Deserialize;

    let json = r#""Ctrl+S""#;
    let shortcut: KeyboardShortcut = serde_json::from_str(json).unwrap();
    assert_eq!(shortcut.key, Key::S);
    assert_eq!(shortcut.modifiers.ctrl, true);
}

#[test]
fn roundtrip_serialize_deserialize() {
    use serde::{Serialize, Deserialize};

    let original = KeyboardShortcut::parse("Shift+Alt+Enter").unwrap();

    let json = serde_json::to_string(&original).unwrap();
    let deserialized: KeyboardShortcut = serde_json::from_str(&json).unwrap();

    assert_eq!(original, deserialized);
}

#[test]
fn serialize_yaml_format() {
    let shortcut = KeyboardShortcut::parse("Ctrl+Q").unwrap();

    // YAML serialization
    let yaml = serde_yaml::to_string(&shortcut).unwrap();

    // Should produce readable YAML string
    assert!(yaml.contains("Ctrl") || yaml.contains("Cmd"));
}

#[test]
fn deserialize_yaml_format() {
    let yaml = r#""Ctrl+Q""#;
    let shortcut: KeyboardShortcut = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(shortcut.key, Key::Q);
}

#[test]
fn deserialize_invalid_shortcut_string_fails() {
    let json = r#""InvalidShortcut""#;
    let result: Result<KeyboardShortcut, _> = serde_json::from_str(json);

    assert!(result.is_err());
}

#[test]
fn serialize_in_struct_context() {
    use serde::{Serialize, Deserialize};

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Config {
        quit: KeyboardShortcut,
        save: KeyboardShortcut,
    }

    let config = Config {
        quit: KeyboardShortcut::parse("Ctrl+Q").unwrap(),
        save: KeyboardShortcut::parse("Ctrl+S").unwrap(),
    };

    let json = serde_json::to_string(&config).unwrap();
    let restored: Config = serde_json::from_str(&json).unwrap();

    assert_eq!(config, restored);
}
```

---

### Integration Edge Cases

#### 18. Complex Parsing Scenarios

```rust
#[test]
fn parse_with_whitespace_tolerance() {
    // Implementations may vary; document expected behavior
    // Option 1: Trim before parsing
    let result = KeyboardShortcut::parse(" Ctrl+S ");
    // Should either succeed (trimmed) or fail (strict)
    // Document which behavior is expected
}

#[test]
fn parse_duplicate_modifiers() {
    // "Ctrl+Ctrl+S" or "Alt+Alt+S"
    let result = KeyboardShortcut::parse("Ctrl+Ctrl+S");
    // Should either normalize (single Ctrl) or fail
    // Document expected behavior
}

#[test]
fn parse_very_long_shortcut_string() {
    let long_str = "Ctrl+Alt+Shift+S+Extra+Extra+Extra";
    let result = KeyboardShortcut::parse(long_str);

    // Should fail: multiple keys or invalid format
    assert!(result.is_err());
}

#[test]
fn parse_special_characters_in_key() {
    let special = KeyboardShortcut::parse("Ctrl+@").unwrap_or_else(|_| {
        // Special chars may not be valid keys
        KeyboardShortcut::parse("Ctrl+Invalid").expect("should fail")
    });

    // Document expected behavior for special characters
}
```

#### 19. Large Batch Operations

```rust
#[test]
fn parse_many_shortcuts() {
    let shortcuts = vec![
        "Ctrl+S", "Ctrl+O", "Ctrl+N", "Ctrl+Q",
        "Alt+F4", "Shift+Tab", "Ctrl+Shift+S",
        "Escape", "Enter", "Space",
    ];

    let parsed: Result<Vec<_>, _> = shortcuts
        .iter()
        .map(|s| KeyboardShortcut::parse(s))
        .collect();

    assert!(parsed.is_ok());
    let results = parsed.unwrap();
    assert_eq!(results.len(), 10);
}

#[test]
fn parse_batch_with_some_errors() {
    let shortcuts = vec![
        "Ctrl+S",      // valid
        "Invalid",     // error
        "Ctrl+A",      // valid
    ];

    let results: Vec<_> = shortcuts
        .iter()
        .map(|s| KeyboardShortcut::parse(s))
        .collect();

    // First and third should succeed, second should fail
    assert!(results[0].is_ok());
    assert!(results[1].is_err());
    assert!(results[2].is_ok());
}
```

---

## Test Execution Plan

### Run All Shortcut Tests

```bash
cargo test -p liquers-lib ui::shortcuts --lib
```

### Run Specific Test Category

```bash
cargo test -p liquers-lib ui::shortcuts::tests::parse_ --lib
cargo test -p liquers-lib ui::shortcuts::tests::display_ --lib
cargo test -p liquers-lib ui::shortcuts::tests::error_ --lib
```

### Run with Output

```bash
cargo test -p liquers-lib ui::shortcuts --lib -- --nocapture
```

### Test Coverage Goals

| Category | Tests | Coverage |
|----------|-------|----------|
| Parsing | 7 test groups (25+ cases) | All valid formats, aliases, case-insensitivity, errors |
| Display | 3 test groups (10+ cases) | Platform-aware, canonical names, round-trip |
| Modifiers | 2 test groups (10+ cases) | Helper methods, equality, hashing |
| Key | 3 test groups (20+ cases) | from_name, name(), equality |
| Errors | 1 test group (8+ cases) | All ShortcutParseError variants |
| Serialization | 1 test group (7+ cases) | JSON, YAML, round-trip |
| Integration | 2 test groups (6+ cases) | Edge cases, batch operations |

**Total: 40-50 test cases across all categories**

---

## Notes for Phase 3 Implementation

1. **Test Organization**: Keep all tests in inline `#[cfg(test)]` module at end of `shortcuts.rs`

2. **Conditional Compilation**: Use `#[cfg(target_os = "macos")]` for platform-specific assertions in display tests

3. **Serde Features**: Ensure `serde` and `serde_json`/`serde_yaml` are available in test context (typically via dev-dependencies)

4. **Error Assertions**: Use `matches!()` macro for pattern matching on error types (cleaner than `.is_err()`)

5. **Documentation**: Each test should be self-documenting; include comments for non-obvious behavior

6. **CI Integration**: Add to GitHub Actions:
   ```bash
   cargo test -p liquers-lib ui::shortcuts --lib
   ```

---

## References

- **Phase 2 Architecture**: `/home/orest/zlos/rust/liquers/specs/keyboard-shortcuts/phase2-architecture.md`
- **Liquers Test Patterns**: `/home/orest/.claude/skills/liquers-unittest/references/test-patterns.md`
- **Project Guidelines**: `/home/orest/zlos/rust/liquers/CLAUDE.md`
- **Crate**: `liquers-lib/src/ui/shortcuts.rs`
