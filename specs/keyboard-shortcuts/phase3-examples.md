# Phase 3: Examples & Use-cases - Keyboard Shortcuts Library

## Example Type

**User choice:** Conceptual code (focused snippets demonstrating key patterns)

## Overview of Examples and Tests

| Category | Name | Demonstrates | Test Coverage |
|----------|------|--------------|---------------|
| Example 1 | Basic Shortcut Usage | Parsing strings, platform-aware display, egui conversion | Unit: parsing, display, round-trip |
| Example 2 | Shortcut Registry & Conflict Detection | HashMap registry, duplicate detection, YAML integration | Unit: conflicts, Integration: menu validation |
| Example 3 | Platform Awareness & Edge Cases | Semantic modifiers, WASM considerations, alias parsing | Unit: modifiers, serialization |
| Unit Tests | Core Functionality | Parsing (25+ cases), display (10+ cases), modifiers, keys, errors, serialization | 40-50 test cases total |
| Integration Tests | Menu System Integration | YAML loading, validation, event handling, cross-platform behavior | 3 integration test flows |
| Corner Cases | Robustness | Memory (large registries), concurrency (thread safety), error handling, serialization edge cases | Documented mitigation strategies |

## Example 1: Basic Shortcut Usage

### Overview

This example demonstrates the primary workflow for the keyboard shortcuts library: parsing shortcut definitions from strings, displaying them in a platform-aware manner, and converting them to egui types for event handling.

### Scenario

You're building a simple text editor with a menu bar that supports keyboard shortcuts. The menu needs to:
1. Parse shortcut definitions from YAML configuration
2. Display shortcuts in menu items (respecting platform conventions)
3. Handle keyboard events using egui
4. Provide clear error messages for invalid shortcuts

### Context

When loading menu definitions from YAML or building programmatic UI, shortcut strings like "Ctrl+S" must be parsed into typed `KeyboardShortcut` values and converted to egui types for event handling. This is the most common use case for the library.

### Parsing Shortcuts from Strings

```rust
use liquers_lib::ui::shortcuts::KeyboardShortcut;

// Parse a standard shortcut string
let save_shortcut = KeyboardShortcut::parse("Ctrl+S")?;
// Result: KeyboardShortcut {
//   modifiers: Modifiers { ctrl: true, alt: false, shift: false },
//   key: Key::S
// }

// Parser is liberal with modifier names - all these are equivalent:
let sc1 = KeyboardShortcut::parse("Ctrl+S")?;
let sc2 = KeyboardShortcut::parse("Cmd+S")?;       // macOS-style name
let sc3 = KeyboardShortcut::parse("Command+S")?;   // Explicit name
// All three parse to the same semantic representation!

// Modifiers can be in any order
let shortcut = KeyboardShortcut::parse("Shift+Ctrl+S")?;
let same = KeyboardShortcut::parse("Ctrl+Shift+S")?;
assert_eq!(shortcut, same);

// Case-insensitive parsing
let shortcut = KeyboardShortcut::parse("ctrl+shift+s")?;
// Works the same as "Ctrl+Shift+S"
```

### Error Handling

```rust
use liquers_lib::ui::shortcuts::KeyboardShortcut;
use liquers_core::error::Error;

// Handle parse failures gracefully
match KeyboardShortcut::parse("Invalid+Foo") {
    Ok(shortcut) => {
        println!("Parsed: {}", shortcut);
    }
    Err(e) => {
        // Error contains descriptive message from Error::general_error()
        eprintln!("Parse error: {}", e);
        // Fallback to default shortcut or skip
    }
}

// Use `?` operator in functions that return Result
fn parse_menu_shortcuts(yaml_shortcuts: &[&str])
    -> Result<Vec<KeyboardShortcut>, Error>
{
    yaml_shortcuts
        .iter()
        .map(|s| KeyboardShortcut::parse(s))
        .collect()  // Short-circuits on first error
}
```

### Platform-Aware Display

```rust
use liquers_lib::ui::shortcuts::{KeyboardShortcut, Modifiers, Key};

// Create a shortcut programmatically
let shortcut = KeyboardShortcut::new(
    Modifiers {
        ctrl: true,
        shift: true,
        alt: false,
    },
    Key::S,
);

// Display respects platform - exactly what you want!
// On macOS:      println!("{}", shortcut) → "Cmd+Shift+S"
// On Windows:    println!("{}", shortcut) → "Ctrl+Shift+S"
// On Linux:      println!("{}", shortcut) → "Ctrl+Shift+S"
// In WASM:       println!("{}", shortcut) → "Ctrl+Shift+S" (fallback)

// This works in UI code:
let menu_label = format!("Save ({})", shortcut);
// On macOS:   "Save (Cmd+Shift+S)"
// On Windows: "Save (Ctrl+Shift+S)"
```

### Converting to egui Types

```rust
use liquers_lib::ui::shortcuts::KeyboardShortcut;

// Parse from user config
let shortcut = KeyboardShortcut::parse("Ctrl+S")?;

// Convert to egui type for use in UI
let egui_shortcut = shortcut.to_egui();
// Result: egui::KeyboardShortcut {
//   modifiers: egui::Modifiers { command: true, alt: false, shift: false, ... },
//   logical_key: egui::Key::S,
// }

// Use in egui's shortcut system
ui.input_mut(|input| {
    if input.consume_shortcut(&egui_shortcut) {
        println!("Save shortcut pressed!");
        // Perform save action
    }
});
```

### Converting Back from egui

```rust
use liquers_lib::ui::shortcuts::KeyboardShortcut;

// When receiving egui events, convert back to our type
let egui_shortcut = egui::KeyboardShortcut::new(
    egui::Modifiers { command: true, shift: true, ..Default::default() },
    egui::Key::S,
);

let our_shortcut = KeyboardShortcut::from(egui_shortcut);
println!("{}", our_shortcut);  // Platform-aware display

// Useful for displaying what the user pressed:
let user_pressed = format!("You pressed: {}", our_shortcut);
// On macOS:   "You pressed: Cmd+Shift+S"
// On Windows: "You pressed: Ctrl+Shift+S"
```

### Expected Output

```
Parsed: Ctrl+S
Platform-aware display: Cmd+S (macOS) or Ctrl+S (Windows/Linux)
Shortcut matched and consumed from input
```

### Validation

- ✅ Demonstrates core functionality (parsing, display, conversion)
- ✅ Uses realistic data/parameters from actual UI scenarios
- ✅ Shows expected output for both platforms
- ✅ Includes error handling patterns

---

## Example 2: Shortcut Registry & Conflict Detection

### Overview

This example demonstrates building a menu system that validates shortcut uniqueness, detects conflicts, and integrates keyboard navigation with the UI framework.

### Scenario

A complex application (like an IDE or editor) loads menu definitions from configuration, needs to ensure no duplicate shortcuts, and wants to provide warnings to administrators before deployment. The menu system must handle graceful degradation when shortcuts are invalid.

### Context

When building sophisticated applications with extensive menu systems, shortcut conflicts can arise as different features are added. The library provides utilities to detect and report these conflicts during configuration loading, before the application runs.

### Building a Shortcut Registry

```rust
use std::collections::HashMap;
use liquers_lib::ui::shortcuts::KeyboardShortcut;

/// Custom enum representing menu actions
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MenuAction {
    Save,
    SaveAs,
    Quit,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
}

/// A registry mapping shortcuts to menu actions
pub struct ShortcutRegistry {
    registry: HashMap<KeyboardShortcut, MenuAction>,
}

impl ShortcutRegistry {
    pub fn new() -> Self {
        Self {
            registry: HashMap::new(),
        }
    }

    /// Register a shortcut with an action
    pub fn register(
        &mut self,
        shortcut_str: &str,
        action: MenuAction,
    ) -> Result<(), String> {
        match KeyboardShortcut::parse(shortcut_str) {
            Ok(shortcut) => {
                // Check for existing mapping
                if let Some(existing) = self.registry.get(&shortcut) {
                    return Err(format!(
                        "Shortcut {} already maps to {:?}",
                        shortcut, existing
                    ));
                }
                self.registry.insert(shortcut, action);
                Ok(())
            }
            Err(e) => Err(format!("Failed to parse '{}': {}", shortcut_str, e)),
        }
    }

    /// Attempt to handle a keyboard event
    pub fn handle_event(&self, shortcut: KeyboardShortcut) -> Option<MenuAction> {
        self.registry.get(&shortcut).cloned()
    }

    /// Get all registered shortcuts
    pub fn shortcuts(&self) -> Vec<(KeyboardShortcut, MenuAction)> {
        self.registry
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect()
    }
}

// Usage example
let mut registry = ShortcutRegistry::new();
registry.register("Ctrl+S", MenuAction::Save)?;
registry.register("Ctrl+Q", MenuAction::Quit)?;
registry.register("Ctrl+Z", MenuAction::Undo)?;
registry.register("Ctrl+Shift+Z", MenuAction::Redo)?;
registry.register("Ctrl+A", MenuAction::SelectAll)?;
```

### Detecting Conflicts in a Collection

```rust
use liquers_lib::ui::shortcuts::find_conflicts;

/// Validate shortcut collection for duplicates
pub fn validate_shortcuts(shortcuts_strs: &[&str])
    -> Result<Vec<KeyboardShortcut>, Vec<String>>
{
    // Step 1: Parse all shortcut strings
    let mut parsed = Vec::new();
    let mut errors = Vec::new();

    for shortcut_str in shortcuts_strs {
        match KeyboardShortcut::parse(shortcut_str) {
            Ok(sc) => parsed.push(sc),
            Err(e) => errors.push(format!("Failed to parse '{}': {}", shortcut_str, e)),
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    // Step 2: Detect duplicate shortcuts
    let conflicts = find_conflicts(parsed.iter());
    if !conflicts.is_empty() {
        let conflict_msgs: Vec<String> = conflicts
            .iter()
            .map(|(sc, count)| format!("Shortcut '{}' appears {} times", sc, count))
            .collect();
        return Err(conflict_msgs);
    }

    Ok(parsed)
}

// Usage
let shortcuts = vec!["Ctrl+S", "Ctrl+Z", "Ctrl+S"];  // Duplicate!
match validate_shortcuts(&shortcuts) {
    Ok(_) => println!("All shortcuts valid and unique"),
    Err(errs) => {
        for err in errs {
            eprintln!("Validation error: {}", err);
        }
    }
}
```

### Validating Shortcut Strings

```rust
use liquers_lib::ui::shortcuts::{validate_shortcut_strings, KeyboardShortcut};

/// Perform comprehensive validation on a set of shortcut strings
pub fn validate_menu_shortcuts(shortcut_configs: &[(String, String)])
    -> ValidationResult
{
    let mut valid = Vec::new();
    let mut parse_errors = Vec::new();
    let mut warnings = Vec::new();

    // Step 1: Parse and validate individual shortcuts
    let shortcut_strs: Vec<&str> = shortcut_configs
        .iter()
        .map(|(s, _)| s.as_str())
        .collect();

    let invalid = validate_shortcut_strings(shortcut_strs.iter().copied());
    for (invalid_str, parse_err) in invalid {
        parse_errors.push(format!(
            "Menu shortcut '{}' is invalid: {}",
            invalid_str, parse_err
        ));
    }

    // Step 2: Collect valid parsed shortcuts
    for (shortcut_str, action_name) in shortcut_configs {
        if let Ok(shortcut) = KeyboardShortcut::parse(shortcut_str) {
            valid.push((shortcut, action_name.clone()));

            // Step 3: Issue warnings for potentially problematic shortcuts
            if shortcut.modifiers.is_empty() {
                warnings.push(format!(
                    "Shortcut '{}' has no modifiers (may conflict with text input)",
                    shortcut_str
                ));
            }
        }
    }

    // Step 4: Detect conflicts among valid shortcuts
    let valid_shortcuts: Vec<_> = valid.iter().map(|(sc, _)| sc).collect();
    let conflicts = find_conflicts(valid_shortcuts.iter());

    let conflict_warnings: Vec<String> = conflicts
        .iter()
        .map(|(sc, count)| {
            format!(
                "Shortcut '{}' is registered {} times",
                sc, count
            )
        })
        .collect();

    ValidationResult {
        valid_count: valid.len(),
        parse_errors,
        conflict_warnings,
        warnings,
    }
}

pub struct ValidationResult {
    pub valid_count: usize,
    pub parse_errors: Vec<String>,
    pub conflict_warnings: Vec<String>,
    pub warnings: Vec<String>,
}

impl ValidationResult {
    pub fn is_valid(&self) -> bool {
        self.parse_errors.is_empty() && self.conflict_warnings.is_empty()
    }
}
```

### Integration with UISpecElement Menu System

```rust
use liquers_lib::ui::shortcuts::KeyboardShortcut;

/// Menu item spec from YAML/config that includes shortcuts
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MenuItemSpec {
    pub label: String,
    pub action: String,
    #[serde(default)]
    pub shortcut: Option<KeyboardShortcut>,  // Deserializes directly from string
}

/// Menu bar spec with validation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MenuBarSpec {
    pub items: Vec<MenuItemSpec>,
}

impl MenuBarSpec {
    /// Validate all shortcuts in menu structure
    pub fn validate_shortcuts(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        // Collect all non-empty shortcuts
        let shortcuts: Vec<_> = self
            .items
            .iter()
            .filter_map(|item| item.shortcut)
            .collect();

        // Check for duplicates using library function
        let conflicts = find_conflicts(shortcuts.iter());
        for (shortcut, count) in conflicts {
            issues.push(ValidationIssue::DuplicateShortcut {
                shortcut: shortcut.to_string(),
                count,
            });
        }

        // Check for problematic patterns
        for (idx, item) in self.items.iter().enumerate() {
            if let Some(shortcut) = &item.shortcut {
                if shortcut.modifiers.is_empty() {
                    issues.push(ValidationIssue::UnmodifiedKey {
                        menu_item_index: idx,
                        label: item.label.clone(),
                        key: shortcut.key.name().to_string(),
                    });
                }
            }
        }

        issues
    }
}

#[derive(Debug, Clone)]
pub enum ValidationIssue {
    DuplicateShortcut {
        shortcut: String,
        count: usize,
    },
    UnmodifiedKey {
        menu_item_index: usize,
        label: String,
        key: String,
    },
}
```

### YAML Configuration Example

```yaml
menu:
  items:
  - !menu
    label: File
    items:
    - !button
      label: Save
      shortcut: "Ctrl+S"       # Parsed by KeyboardShortcut::deserialize
      action: save_file
    - !button
      label: Save As
      shortcut: "Ctrl+Shift+S"
      action: save_as
    - !separator
    - !button
      label: Quit
      shortcut: "Ctrl+Q"
      action: quit
  - !menu
    label: Edit
    items:
    - !button
      label: Undo
      shortcut: "Ctrl+Z"
      action: undo
    - !button
      label: Redo
      shortcut: "Ctrl+Shift+Z"
      action: redo
```

### Integration Code

```rust
// During UISpecElement initialization
impl MenuBarSpec {
    fn collect_shortcuts(&self) -> Vec<String> {
        let mut shortcuts = Vec::new();
        // Recursively collect shortcuts from menu structure
        // ... implementation details ...
        shortcuts
    }

    fn check_shortcut(&self, ui: &egui::Ui, shortcut_str: &str) -> bool {
        // Use library instead of inline parsing (replaces old ui_spec_element.rs code)
        match KeyboardShortcut::parse(shortcut_str) {
            Ok(shortcut) => {
                let egui_shortcut = shortcut.to_egui();
                ui.input_mut(|i| i.consume_shortcut(&egui_shortcut))
            }
            Err(_) => false,  // Invalid shortcut string, can't trigger
        }
    }
}

// Load and validate menu config
let yaml_config = include_str!("menu_config.yaml");
let menu_spec: MenuBarSpec = serde_yaml::from_str(yaml_config)?;

// Validate shortcuts
let issues = menu_spec.validate_shortcuts();
if issues.is_empty() {
    println!("Menu shortcuts validated successfully");
} else {
    for issue in issues {
        eprintln!("Validation issue: {:?}", issue);
    }
}

// Use shortcuts in event handling
fn handle_ui_input(ui: &egui::Ui, menu_spec: &MenuBarSpec) {
    for item in &menu_spec.items {
        if let Some(shortcut) = &item.shortcut {
            let egui_shortcut = shortcut.to_egui();
            if ui.input_mut(|i| i.consume_shortcut(&egui_shortcut)) {
                println!("Executing action: {}", item.action);
                // Dispatch action via UIContext
            }
        }
    }
}
```

### Expected Output

```
Menu shortcuts validated successfully
Executing action: file.save
Executing action: edit.undo
```

### Validation

- ✅ Demonstrates advanced use case (registry, conflict detection)
- ✅ Shows integration with existing UISpecElement system
- ✅ Includes YAML serialization/deserialization
- ✅ Provides error handling and graceful degradation

---

## Example 3: Platform Awareness & Edge Cases

### Overview

This example comprehensively demonstrates the semantic command modifier behavior, round-trip parsing/display, alias parsing, and WASM considerations.

### Semantic Command Modifier

#### The Problem: Platform-Specific Primary Command Keys

Different operating systems use different keys for the primary command modifier:
- **macOS**: Command key (⌘) - users expect "Cmd+S" for Save
- **Windows/Linux**: Control key (Ctrl) - users expect "Ctrl+S" for Save
- **Web/WASM**: Depends on runtime browser detection (not compile time)

#### The Solution: Semantic `ctrl` Field

The `ctrl` boolean field in `Modifiers` is **semantic**, not physical. It represents "the platform's primary command modifier" regardless of the underlying OS.

```rust
use liquers_lib::ui::shortcuts::{KeyboardShortcut, Modifiers, Key};

// Parse the SAME string on all platforms
let sc1 = KeyboardShortcut::parse("Ctrl+S")?;
let sc2 = KeyboardShortcut::parse("Cmd+S")?;
let sc3 = KeyboardShortcut::parse("Command+S")?;

// All three parse to identical semantic meaning
assert_eq!(sc1, sc2);
assert_eq!(sc2, sc3);

// Verify the semantic field
assert!(sc1.modifiers.ctrl);  // "ctrl: true" means "semantic command key"
assert!(!sc1.modifiers.alt);
assert!(!sc1.modifiers.shift);
```

#### Platform-Aware Display

```rust
let shortcut = KeyboardShortcut::new(
    Modifiers { ctrl: true, alt: false, shift: false },
    Key::S
);

// Display changes based on platform
#[cfg(target_os = "macos")]
assert_eq!(shortcut.to_string(), "Cmd+S");

#[cfg(any(target_os = "windows", target_os = "linux"))]
assert_eq!(shortcut.to_string(), "Ctrl+S");

// WASM always displays as "Ctrl" (compile-time detection unavailable)
// But actual shortcut matching works correctly via egui runtime detection
#[cfg(target_arch = "wasm32")]
assert_eq!(shortcut.to_string(), "Ctrl+S");
```

#### Why This Works

1. **One definition, all platforms**: Users write `"Ctrl+S"` once in config
2. **Automatic platform adaptation**: Display normalizes to platform convention
3. **Framework delegation**: egui/dioxus handle runtime platform detection for event matching
4. **WASM compatible**: No compile-time `cfg!` needed; runtime detection via browser events

```rust
// In application code (e.g., ui_spec_element.rs)
let shortcut = KeyboardShortcut::parse("Ctrl+S")?;
let egui_shortcut = shortcut.to_egui();

// On macOS native: egui::KeyboardShortcut {
//     modifiers: egui::Modifiers { command: true, .. },
//     logical_key: egui::Key::S
// }
// On Windows native: egui::KeyboardShortcut {
//     modifiers: egui::Modifiers { ctrl: true, .. },
//     logical_key: egui::Key::S
// }
// In WASM: egui detects browser OS and routes events accordingly
```

### Round-Trip Parsing and Display

#### Preservation of Semantic Meaning (Not Exact Strings)

Round-trip parsing preserves **meaning**, not necessarily the exact original string. This is intentional:

```rust
let original_string = "Ctrl+S";
let shortcut: KeyboardShortcut = original_string.parse()?;

// Serialized/displayed form
let serialized = shortcut.to_string();

// On macOS: serialized == "Cmd+S"        (different string, same meaning)
// On Windows/Linux: serialized == "Ctrl+S"  (same string)
// In WASM: serialized == "Ctrl+S"        (always Ctrl for display)

// Semantic meaning is identical
assert_eq!(shortcut.modifiers.ctrl, true);
assert_eq!(shortcut.key, Key::S);
```

#### Modifier Order Normalization

Modifiers are reordered to canonical form during round-trip:

```rust
// Three different input strings
let input1 = "Ctrl+Shift+S";
let input2 = "Shift+Ctrl+S";
let input3 = "S+Shift+Ctrl";  // Parsing order doesn't matter

let sc1: KeyboardShortcut = input1.parse()?;
let sc2: KeyboardShortcut = input2.parse()?;
let sc3: KeyboardShortcut = input3.parse()?;

// All parse to identical structure
assert_eq!(sc1, sc2);
assert_eq!(sc2, sc3);

// Display uses canonical order: Ctrl, Alt, Shift, Key
let canonical = sc1.to_string();
// macOS: "Cmd+Shift+S"
// Windows/Linux: "Ctrl+Shift+S"
// WASM: "Ctrl+Shift+S"

// Canonical form always starts with semantic command modifier (if present)
let sc_alt_only = KeyboardShortcut::parse("Alt+F4")?;
assert_eq!(sc_alt_only.to_string(), "Alt+F4");  // No command modifier
```

### Alias Parsing

#### Semantic Command Modifier Aliases

The semantic `ctrl` field accepts multiple input aliases representing the same concept:

```rust
use liquers_lib::ui::shortcuts::KeyboardShortcut;

// All these parse to IDENTICAL shortcuts
let sc1 = KeyboardShortcut::parse("Ctrl+S")?;      // Windows/Linux convention
let sc2 = KeyboardShortcut::parse("Cmd+S")?;       // macOS convention
let sc3 = KeyboardShortcut::parse("Command+S")?;   // macOS long form
let sc4 = KeyboardShortcut::parse("Meta+S")?;      // Web standard alias

assert_eq!(sc1, sc2);
assert_eq!(sc2, sc3);
assert_eq!(sc3, sc4);

// All have identical semantic meaning
assert!(sc1.modifiers.ctrl);
assert_eq!(sc1.key, Key::S);
```

#### Key Aliases

Common key names have multiple valid spellings:

```rust
// Escape key aliases
let esc1 = KeyboardShortcut::parse("Ctrl+Escape")?;
let esc2 = KeyboardShortcut::parse("Ctrl+Esc")?;
assert_eq!(esc1, esc2);

// Enter key aliases
let enter1 = KeyboardShortcut::parse("Shift+Enter")?;
let enter2 = KeyboardShortcut::parse("Shift+Return")?;
assert_eq!(enter1, enter2);

// Arrow key variants
let up1 = KeyboardShortcut::parse("Ctrl+ArrowUp")?;
let up2 = KeyboardShortcut::parse("Ctrl+Up")?;    // Short form
assert_eq!(up1, up2);
```

#### Web Format Support

Browser keyboard events use a different format. The parser accepts both:

```rust
// Human-readable format (Liquers native)
let human = KeyboardShortcut::parse("Ctrl+S")?;

// Web KeyboardEvent.code format (browser native)
// Format: "Control+KeyS", "Shift+KeyA", "Meta+Escape", etc.
let web = KeyboardShortcut::parse("Control+KeyS")?;

assert_eq!(human, web);  // Same semantic shortcut

// Mixed format (parser is liberal)
let mixed = KeyboardShortcut::parse("Control+S")?;
assert_eq!(mixed, human);

// Both can be round-tripped
let serialized_human = human.to_string();
let serialized_web = web.to_string();

// On macOS: both serialize to "Cmd+S"
// On Windows/Linux: both serialize to "Ctrl+S"
// WASM: both serialize to "Ctrl+S"
```

### WASM Considerations

#### The WASM Platform Detection Challenge

Traditional code uses `cfg!(target_os = "macos")` to detect macOS at compile time. This fails in WASM:

```rust
// ❌ WRONG for WASM
#[cfg(target_os = "macos")]
fn display_modifier() -> &'static str {
    "Cmd"  // Always wrong in WASM, even if running in Safari on macOS
}

#[cfg(not(target_os = "macos"))]
fn display_modifier() -> &'static str {
    "Ctrl"
}

// WASM compiles to `wasm32-unknown-unknown`
// cfg!(target_os = "macos") is ALWAYS false, regardless of browser/OS
```

#### The Solution: Delegate to Frameworks

Our library **does not** use compile-time platform detection. Instead:

1. **Library**: Maps semantic `ctrl` to egui's semantic `command`
2. **egui native**: Uses compile-time `cfg!` correctly
3. **egui WASM**: Uses **runtime** browser detection
4. **Browser**: Automatically provides platform-correct events

```rust
impl Modifiers {
    /// Convert to egui::Modifiers (WASM-safe)
    pub fn to_egui(&self) -> egui::Modifiers {
        egui::Modifiers {
            alt: self.alt,
            shift: self.shift,
            command: self.ctrl,  // Semantic → semantic
            ..Default::default()
        }
    }
}

// egui handles the rest:
// - Native macOS app: egui::Modifiers.command checks cfg!(target_os = "macos")
// - WASM in Safari on macOS: egui detects metaKey at runtime
// - WASM in Chrome on Windows: egui detects ctrlKey at runtime
```

#### WASM Display Format

WASM apps cannot determine the runtime OS at compile time. Therefore:

```rust
#[cfg(target_arch = "wasm32")]
impl Display for Modifiers {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // Always use "Ctrl" since we can't detect platform at compile time
        if self.ctrl {
            write!(f, "Ctrl")?;
        }
        // ... rest of display
    }
}

// Result: WASM shortcuts always display as "Ctrl+S", even on macOS
// But actual shortcut matching works correctly:
// - egui detects browser OS at runtime
// - Routes browser events (metaKey/ctrlKey) correctly
// - Shortcut matching works for all platforms
```

### Real-World Scenario: Menu Bar Configuration

Combining all edge cases in a practical example:

```rust
use liquers_lib::ui::shortcuts::KeyboardShortcut;
use std::collections::HashMap;

// YAML configuration (user-written, possibly on any platform)
let yaml_config = r#"
menu:
  file:
    - label: Save
      shortcut: "Cmd+S"      # User wrote macOS convention
    - label: Save As
      shortcut: "shift+cmd+s" # Lowercase, different format
    - label: Quit
      shortcut: "Ctrl+Q"     # User wrote Windows convention
  edit:
    - label: Undo
      shortcut: "Control+Z"  # Web format
    - label: Redo
      shortcut: "Cmd+Shift+Z" # Mixed case, macOS convention
"#;

// Parse all shortcuts
let mut shortcuts: HashMap<String, KeyboardShortcut> = HashMap::new();

let save = KeyboardShortcut::parse("Cmd+S")?;
let save_as = KeyboardShortcut::parse("shift+cmd+s")?;
let quit = KeyboardShortcut::parse("Ctrl+Q")?;
let undo = KeyboardShortcut::parse("Control+Z")?;
let redo = KeyboardShortcut::parse("Cmd+Shift+Z")?;

// Key insight: All normalize to semantic representation
assert_eq!(save.modifiers.ctrl, true);
assert_eq!(quit.modifiers.ctrl, true);
assert_eq!(undo.modifiers.ctrl, true);
assert_eq!(redo.modifiers.ctrl, true);

// Display adapts to platform
let save_display = save.to_string();
#[cfg(target_os = "macos")]
assert_eq!(save_display, "Cmd+S");
#[cfg(any(target_os = "windows", target_os = "linux"))]
assert_eq!(save_display, "Ctrl+S");

// Conflict detection works with normalized shortcuts
use liquers_lib::ui::shortcuts::find_conflicts;
let all_shortcuts = vec![&save, &quit, &undo, &redo];
let conflicts = find_conflicts(all_shortcuts.iter());

// No conflicts detected: Save, Quit, Undo, Redo are all different
assert!(conflicts.is_empty());
```

### Expected Output

```
Parsed: Ctrl+S (semantic)
Display on macOS: Cmd+S
Display on Windows/Linux: Ctrl+S
All shortcuts validated successfully
No conflicts detected
```

### Validation

- ✅ Demonstrates semantic command modifier design
- ✅ Shows round-trip behavior (meaning preserved, format normalized)
- ✅ Covers alias parsing comprehensively
- ✅ Explains WASM considerations and solutions
- ✅ Provides real-world scenario combining all edge cases

---

## Corner Cases

### 1. Memory Considerations

#### Large Shortcut Registry

**Scenario:** Registry with thousands of shortcuts (IDE with extensive keybindings)

- **Expected behavior**: HashMap scales well (O(1) lookup)
- **Storage**: Bounded by number of unique shortcuts; KeyboardShortcut is Copy (16 bytes)
- **Test approach**: Build registry with 10,000+ shortcuts, verify no slowdown
- **Mitigation**: Use `HashMap<KeyboardShortcut, Action>` (already optimized for small keys)

#### Long Shortcut Strings

**Scenario:** Very long shortcut definitions in YAML configs

- **Expected behavior**: Parsing is linear in string length; reasonable worst case is "Ctrl+Shift+Alt+ArrowUp" (~25 chars)
- **Invalid formats**: Rejected early; no memory leaks from malformed input
- **Mitigation**: Parser allocates only for error messages, not intermediate parsing

#### Allocation Failures

**Scenario:** System out of memory during shortcut parsing

- **Expected behavior**: Return `Err(Error::InvalidFormat)` or allocation failure
- **No panic**: All allocations are fallible (String, Vec allocations in error paths)
- **Mitigation**: Document expected behavior; no special handling needed (Rust default)

#### Memory Leaks

**Scenario:** Repeated shortcut parsing/validation cycles

- **Expected behavior**: No memory growth over time (all types are Copy or have clear Drop)
- **Test approach**: Run 100,000 parse cycles, verify stable memory usage
- **Mitigation**: All owned data is in error types (String), which are properly dropped

### 2. Concurrency Considerations

#### Shared Registry Across Threads

**Scenario:** Multiple threads need read access to shortcut registry

- **Expected behavior**: `KeyboardShortcut`, `Modifiers`, `Key` are `Copy` types (Send + Sync)
- **Thread safety**: HashMap can be wrapped in `Arc<Mutex<HashMap<...>>>` for mutable access
- **Read-only access**: Share `Arc<HashMap<...>>` directly (no locks for immutable reads)
- **Test approach**: Share registry across 10 threads; match shortcuts concurrently

```rust
use std::sync::Arc;
use std::collections::HashMap;

let registry = Arc::new(HashMap::new());
let registry_clone = Arc::clone(&registry);

std::thread::spawn(move || {
    // Safe: KeyboardShortcut is Copy + Send + Sync
    let action = registry_clone.get(&some_shortcut);
});
```

#### Race Conditions

**Scenario:** Multiple threads modifying registry concurrently

- **Expected behavior**: Use `Mutex` or `RwLock` for mutable access
- **No data races**: Rust's type system prevents unsynchronized access
- **Mitigation**: Document that `ShortcutRegistry` should use interior mutability if shared

#### WASM Considerations

**Scenario:** WASM environment (no threads)

- **Expected behavior**: Shortcut types work identically; no threading concerns
- **Single-threaded**: WASM runs in single-threaded event loop
- **Mitigation**: No special handling needed; types are safe for WASM

#### Deadlocks

**Scenario:** If using locks, multiple locks acquired in different orders

- **Expected behavior**: No deadlocks (only one lock per registry)
- **Design**: Registry uses single HashMap; no nested locks
- **Mitigation**: Document lock ordering if multiple registries are used

### 3. Error Handling

#### Invalid Shortcut Strings from Untrusted Input

**Scenario:** User-provided YAML with malformed shortcuts

```rust
// Malformed input handled gracefully
let invalid = vec!["", "Ctrl", "Ctrl+", "Ctrl+Foo", "Foo+S"];
let errors = validate_shortcut_strings(invalid.iter().copied());
// Returns: EmptyString, InvalidFormat, UnknownKey, UnknownModifier
```

- **Expected behavior**: All parse errors return typed `Error`
- **No panic**: Parser never panics on invalid input
- **Test approach**: Fuzz parser with random strings; verify no panics

#### Missing Modifiers/Keys

**Scenario:** Edge case shortcut definitions

- `"Ctrl+Ctrl+S"` → Parsed as "Ctrl+S" (duplicate modifiers normalized)
- `"S"` → Valid (key only, no modifiers)
- `"+"` → Invalid (`Err(InvalidFormat)`)
- `"Ctrl+"` → Invalid (`Err(InvalidFormat)`)

#### Case-Insensitive but Canonicalized

**Scenario:** Mixed-case input strings

- `"ctrl+s"` parses successfully (case-insensitive)
- Displays as platform-aware: "Cmd+S" on macOS, "Ctrl+S" elsewhere
- Test approach: Verify all case combinations parse to same result

#### Parse Error Propagation

**Scenario:** Batch parsing with some errors

```rust
let shortcuts = vec!["Ctrl+S", "Invalid", "Ctrl+A"];
let results: Vec<_> = shortcuts
    .iter()
    .map(|s| KeyboardShortcut::parse(s))
    .collect();

// First and third succeed, second fails
assert!(results[0].is_ok());
assert!(results[1].is_err());
assert!(results[2].is_ok());
```

- **Expected behavior**: Use `Result<_, Error>` for individual parse
- **Batch operations**: Use `collect()` with `?` to short-circuit, or collect all errors
- **Mitigation**: Provide both strict (fail-fast) and permissive (collect all errors) helpers

### 4. Serialization Edge Cases

#### Round-Trip with Platform Variance

**Scenario:** YAML written on Windows, read on macOS

```rust
// User writes on Windows
let original = "Ctrl+S";
let shortcut: KeyboardShortcut = serde_yaml::from_str("\"Ctrl+S\"").unwrap();

// On macOS, serialization:
let serialized = serde_yaml::to_string(&shortcut).unwrap();
// Result: "Cmd+S" (different from original but semantically identical)

// Both are correct; display format varies, meaning is preserved
```

- **Expected behavior**: Semantic meaning preserved; display format normalized
- **Test approach**: Serialize on one platform, deserialize on another (mock cfg)
- **Mitigation**: Document that round-trip preserves meaning, not exact string

#### Schema Evolution

**Scenario:** Future versions extend shortcut format

- **Current format**: Simple string `"Ctrl+S"`
- **Future format**: Could add structured YAML `{ modifiers: [ctrl, shift], key: S }`
- **Backward compatibility**: Custom serde impl can handle both formats
- **Mitigation**: Use serde's `#[serde(untagged)]` for format detection

#### Compression

**Scenario:** Shortcut data in compressed config files

- **Expected behavior**: Transparent; serde handles serialization, compression is external
- **Test approach**: Not library concern; handled by config loading layer
- **Mitigation**: None needed (serialization is format-agnostic)

#### Metadata Preservation

**Scenario:** YAML files with comments or extra fields

```yaml
shortcuts:
  save: "Ctrl+S"  # Standard save shortcut
  quit: "Ctrl+Q"
  # Legacy: was "Alt+F4" on older versions
```

- **Expected behavior**: Serde deserializes only known fields; comments preserved by YAML library
- **Test approach**: Load YAML with comments; verify shortcuts parse correctly
- **Mitigation**: Use YAML library that preserves comments if needed (e.g., `yaml-rust`)

### 5. Integration (Cross-Component Interactions)

#### With UISpecElement Menu System

**Scenario:** Replacing inline shortcut parsing in `ui_spec_element.rs`

- **Old code**: Manual `parse_key()` and `check_shortcut()` methods
- **New code**: Use `KeyboardShortcut::parse()` and `to_egui()`
- **Expected behavior**: Drop-in replacement; same functionality, cleaner code
- **Test approach**: Validate existing UISpecElement tests still pass

```rust
// Before (ui_spec_element.rs:378-416)
fn check_shortcut(&self, ui: &egui::Ui, shortcut_str: &str) -> bool {
    // Manual parsing with egui::Key::from_name()
    // ... 30+ lines of code ...
}

// After
fn check_shortcut(&self, ui: &egui::Ui, shortcut_str: &str) -> bool {
    match KeyboardShortcut::parse(shortcut_str) {
        Ok(shortcut) => {
            let egui_shortcut = shortcut.to_egui();
            ui.input_mut(|i| i.consume_shortcut(&egui_shortcut))
        }
        Err(_) => false,
    }
}
```

#### With Menu Bar Validation

**Scenario:** Validating shortcuts in MenuBarSpec (ui_spec_element.rs)

- **Old code**: Manual conflict detection with HashMap
- **New code**: Use `find_conflicts()` utility
- **Expected behavior**: Same validation, reusable across components
- **Test approach**: Compare old and new validation results

```rust
// Before
impl MenuBarSpec {
    pub fn validate_shortcuts(&self) -> Vec<(String, usize)> {
        // Manual HashMap building and counting
        // ... 20+ lines of code ...
    }
}

// After
impl MenuBarSpec {
    pub fn validate_shortcuts(&self) -> Vec<(String, usize)> {
        let shortcuts: Vec<_> = self.collect_shortcuts()
            .iter()
            .filter_map(|s| KeyboardShortcut::parse(s).ok())
            .collect();

        let conflicts = find_conflicts(shortcuts.iter());
        conflicts.into_iter()
            .map(|(sc, count)| (sc.to_string(), count))
            .collect()
    }
}
```

#### With YAML Configuration Loading

**Scenario:** Deserializing menu configs with shortcuts

- **Expected behavior**: Shortcuts deserialize directly via serde
- **Error handling**: Invalid shortcuts fail deserialization with clear error
- **Test approach**: Load valid and invalid YAML configs; verify errors are descriptive

```yaml
# Valid config
menu:
  items:
    - label: Save
      shortcut: "Ctrl+S"  # Deserializes to KeyboardShortcut

# Invalid config
menu:
  items:
    - label: BadShortcut
      shortcut: "InvalidKey"  # Deserialization fails with Error
```

#### With Future UI Frameworks (Ratatui, Dioxus)

**Scenario:** Adding conversions to/from other frameworks

- **Expected behavior**: Add new trait impls without breaking existing code
- **Design**: `to_ratatui()`, `to_dioxus()` methods added in future
- **Test approach**: Ensure KeyboardShortcut remains backward compatible
- **Mitigation**: Use feature flags for optional framework support

```rust
// Future extension (not in Phase 1)
#[cfg(feature = "ratatui")]
impl KeyboardShortcut {
    pub fn to_ratatui(&self) -> crossterm::event::KeyEvent {
        // Convert to ratatui format
    }
}

#[cfg(feature = "dioxus")]
impl KeyboardShortcut {
    pub fn to_dioxus(&self) -> dioxus::events::KeyboardData {
        // Convert to dioxus format
    }
}
```

---

## Test Plan

### Unit Tests

**File:** `liquers-lib/src/ui/shortcuts.rs` (inline `#[cfg(test)]` module)

**Test Structure:**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    // Test groups organized by functionality
    // 1. Parsing (25+ test cases)
    // 2. Display (10+ test cases)
    // 3. Modifiers (10+ test cases)
    // 4. Key (20+ test cases)
    // 5. Errors (8+ test cases)
    // 6. Serialization (7+ test cases)
    // 7. Integration edge cases (6+ test cases)
}
```

#### 1. Parsing Tests (25+ cases)

```rust
#[test]
fn parse_valid_ctrl_s() {
    let shortcut = KeyboardShortcut::parse("Ctrl+S").unwrap();
    assert_eq!(shortcut.modifiers.ctrl, true);
    assert_eq!(shortcut.key, Key::S);
}

#[test]
fn parse_case_insensitive() {
    let sc1 = KeyboardShortcut::parse("Ctrl+S").unwrap();
    let sc2 = KeyboardShortcut::parse("ctrl+s").unwrap();
    assert_eq!(sc1, sc2);
}

#[test]
fn parse_cmd_equals_ctrl() {
    let sc1 = KeyboardShortcut::parse("Ctrl+S").unwrap();
    let sc2 = KeyboardShortcut::parse("Cmd+S").unwrap();
    assert_eq!(sc1, sc2);
}

#[test]
fn parse_modifier_order_independent() {
    let sc1 = KeyboardShortcut::parse("Ctrl+Shift+S").unwrap();
    let sc2 = KeyboardShortcut::parse("Shift+Ctrl+S").unwrap();
    assert_eq!(sc1, sc2);
}

#[test]
fn parse_key_aliases() {
    let esc1 = KeyboardShortcut::parse("Ctrl+Escape").unwrap();
    let esc2 = KeyboardShortcut::parse("Ctrl+Esc").unwrap();
    assert_eq!(esc1, esc2);
}

#[test]
fn parse_empty_string_returns_error() {
    let result = KeyboardShortcut::parse("");
    assert!(result.is_err());  // Parse error expected
}

#[test]
fn parse_unknown_key_returns_error() {
    let result = KeyboardShortcut::parse("Ctrl+UnknownKey");
    assert!(result.is_err());  // Parse error expected
}
```

**Coverage:**
- ✅ Valid single modifier + key
- ✅ Multiple modifiers in any order
- ✅ Case-insensitive parsing
- ✅ Semantic command modifier aliases (Ctrl, Cmd, Command, Meta)
- ✅ Key aliases (Escape/Esc, Enter/Return)
- ✅ Web format support (Control+KeyS, Digit5)
- ✅ All error cases (EmptyString, UnknownKey, UnknownModifier, InvalidFormat)

#### 2. Display Tests (10+ cases)

```rust
#[test]
fn display_ctrl_key() {
    let shortcut = KeyboardShortcut::new(Modifiers::command(), Key::S);
    let displayed = shortcut.to_string();

    #[cfg(target_os = "macos")]
    assert_eq!(displayed, "Cmd+S");

    #[cfg(not(target_os = "macos"))]
    assert_eq!(displayed, "Ctrl+S");
}

#[test]
fn display_canonical_key_names() {
    let shortcut = KeyboardShortcut::new(Modifiers::none(), Key::Escape);
    assert_eq!(shortcut.to_string(), "Escape");  // Not "Esc"
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

    // Order: Ctrl/Cmd, Alt, Shift
    #[cfg(target_os = "macos")]
    assert!(displayed.starts_with("Cmd"));

    assert!(displayed.contains("Alt"));
    assert!(displayed.contains("Shift"));
    assert!(displayed.ends_with("S"));
}
```

**Coverage:**
- ✅ Platform-aware display (Cmd on macOS, Ctrl elsewhere)
- ✅ Canonical key names (Escape not Esc, Enter not Return)
- ✅ Canonical modifier order (Ctrl, Alt, Shift)
- ✅ Single key (no modifiers)
- ✅ Function keys, arrow keys, punctuation

#### 3. Modifiers Tests (10+ cases)

```rust
#[test]
fn modifiers_none_creates_empty() {
    let modifiers = Modifiers::none();
    assert_eq!(modifiers.ctrl, false);
    assert_eq!(modifiers.alt, false);
    assert_eq!(modifiers.shift, false);
}

#[test]
fn modifiers_is_empty() {
    let empty = Modifiers::none();
    assert!(empty.is_empty());

    let with_ctrl = Modifiers { ctrl: true, ..Modifiers::none() };
    assert!(!with_ctrl.is_empty());
}

#[test]
fn modifiers_command_creates_ctrl_true() {
    let modifiers = Modifiers::command();
    assert_eq!(modifiers.ctrl, true);
    assert_eq!(modifiers.alt, false);
    assert_eq!(modifiers.shift, false);
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

**Coverage:**
- ✅ Helper methods (none(), command(), is_empty())
- ✅ Equality and hashing
- ✅ Copy semantics
- ✅ Default trait

#### 4. Key Tests (20+ cases)

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
fn key_from_name_escape_aliases() {
    assert_eq!(Key::from_name("Escape"), Some(Key::Escape));
    assert_eq!(Key::from_name("Esc"), Some(Key::Escape));
}

#[test]
fn key_name_returns_canonical() {
    assert_eq!(Key::A.name(), "A");
    assert_eq!(Key::Escape.name(), "Escape");
}

#[test]
fn key_name_roundtrip_with_from_name() {
    let keys = vec![Key::A, Key::Escape, Key::F1, Key::ArrowUp];
    for key in keys {
        let name = key.name();
        let parsed = Key::from_name(name).expect("Should parse back");
        assert_eq!(parsed, key);
    }
}
```

**Coverage:**
- ✅ from_name() for all key types (letters, numbers, function keys, navigation, editing, punctuation)
- ✅ Case-insensitive parsing
- ✅ Key aliases (Escape/Esc, Enter/Return, etc.)
- ✅ name() returns canonical names
- ✅ Round-trip (from_name ↔ name)
- ✅ Copy semantics and equality

#### 5. Error Tests (8+ cases)

```rust
#[test]
fn error_empty_string_displays() {
    let err = Error::EmptyString;
    let msg = err.to_string();
    assert!(msg.to_lowercase().contains("empty"));
}

#[test]
fn error_unknown_key_displays_key_name() {
    let err = Error::UnknownKey("Foo".to_string());
    let msg = err.to_string();
    assert!(msg.contains("Foo"));
}

#[test]
fn error_types_are_distinct() {
    let err1 = Error::EmptyString;
    let err2 = Error::UnknownKey("A".to_string());
    assert_ne!(err1.to_string(), err2.to_string());
}

#[test]
fn error_clone() {
    let err = Error::UnknownKey("Test".to_string());
    let err_clone = err.clone();
    assert_eq!(err.to_string(), err_clone.to_string());
}
```

**Coverage:**
- ✅ All Error variants (EmptyString, UnknownKey, UnknownModifier, InvalidFormat)
- ✅ Display messages are clear and informative
- ✅ Clone and Debug traits
- ✅ Error types are distinct

#### 6. Serialization Tests (7+ cases)

```rust
#[test]
fn serialize_keyboard_shortcut_to_string() {
    let shortcut = KeyboardShortcut::parse("Ctrl+S").unwrap();
    let json = serde_json::to_string(&shortcut).unwrap();
    assert!(json.contains("Ctrl") || json.contains("Cmd"));
}

#[test]
fn deserialize_keyboard_shortcut_from_string() {
    let json = r#""Ctrl+S""#;
    let shortcut: KeyboardShortcut = serde_json::from_str(json).unwrap();
    assert_eq!(shortcut.key, Key::S);
}

#[test]
fn roundtrip_serialize_deserialize() {
    let original = KeyboardShortcut::parse("Shift+Alt+Enter").unwrap();
    let json = serde_json::to_string(&original).unwrap();
    let deserialized: KeyboardShortcut = serde_json::from_str(&json).unwrap();
    assert_eq!(original, deserialized);
}

#[test]
fn serialize_yaml_format() {
    let shortcut = KeyboardShortcut::parse("Ctrl+Q").unwrap();
    let yaml = serde_yaml::to_string(&shortcut).unwrap();
    assert!(yaml.contains("Ctrl") || yaml.contains("Cmd"));
}

#[test]
fn deserialize_invalid_shortcut_string_fails() {
    let json = r#""InvalidShortcut""#;
    let result: Result<KeyboardShortcut, _> = serde_json::from_str(json);
    assert!(result.is_err());
}
```

**Coverage:**
- ✅ Serialize to JSON/YAML
- ✅ Deserialize from JSON/YAML
- ✅ Round-trip serialization
- ✅ Invalid shortcuts fail deserialization
- ✅ Serialization in struct context

#### 7. Integration Edge Cases (6+ cases)

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
    let shortcuts = vec!["Ctrl+S", "Invalid", "Ctrl+A"];
    let results: Vec<_> = shortcuts
        .iter()
        .map(|s| KeyboardShortcut::parse(s))
        .collect();

    assert!(results[0].is_ok());
    assert!(results[1].is_err());
    assert!(results[2].is_ok());
}
```

**Coverage:**
- ✅ Large batch operations
- ✅ Mixed valid/invalid shortcuts
- ✅ Whitespace handling
- ✅ Duplicate modifiers

### Integration Tests

**File:** `liquers-lib/tests/ui_shortcuts_integration.rs`

#### Test Flow 1: Menu Spec Integration

```rust
#[tokio::test]
async fn test_menu_spec_with_shortcuts() {
    // Load YAML menu config
    let yaml = r#"
    items:
      - label: "Save"
        action: "file.save"
        shortcut: "Ctrl+S"
      - label: "Quit"
        action: "app.quit"
        shortcut: "Ctrl+Q"
    "#;

    let menu_spec: MenuBarSpec = serde_yaml::from_str(yaml).unwrap();

    // Validate shortcuts
    let issues = menu_spec.validate_shortcuts();
    assert!(issues.is_empty());

    // Simulate keyboard event handling
    // (Would need mock egui context; conceptual test)
    // assert!(menu_spec.check_shortcut(&ui, "Ctrl+S"));
}
```

#### Test Flow 2: Cross-Platform Validation

```rust
#[test]
fn test_cross_platform_serialization() {
    let shortcut = KeyboardShortcut::parse("Ctrl+S").unwrap();

    // Serialize
    let yaml = serde_yaml::to_string(&shortcut).unwrap();

    // On macOS, should contain "Cmd"
    #[cfg(target_os = "macos")]
    assert!(yaml.contains("Cmd"));

    // On Windows/Linux, should contain "Ctrl"
    #[cfg(not(target_os = "macos"))]
    assert!(yaml.contains("Ctrl"));

    // Deserialize back - should work regardless
    let deserialized: KeyboardShortcut = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(deserialized, shortcut);
}
```

#### Test Flow 3: Error Recovery

```rust
#[test]
fn test_graceful_degradation_with_invalid_shortcuts() {
    let yaml = r#"
    items:
      - label: "Save"
        action: "file.save"
        shortcut: "Ctrl+S"
      - label: "BadShortcut"
        action: "bad.action"
        shortcut: "InvalidKey"
      - label: "Quit"
        action: "app.quit"
        shortcut: "Ctrl+Q"
    "#;

    let menu_spec: MenuBarSpec = serde_yaml::from_str(yaml).unwrap();

    // Validation should report error for "InvalidKey"
    let issues = menu_spec.validate_shortcuts();
    assert!(!issues.is_empty());

    // But other valid shortcuts should still work
    // (Menu renders; invalid shortcut is skipped)
}
```

**Coverage:**
- ✅ End-to-end YAML loading and validation
- ✅ Cross-platform serialization/deserialization
- ✅ Error recovery and graceful degradation
- ✅ Integration with UISpecElement system

### Manual Validation

**Commands to run:**

```bash
# Run all unit tests
cargo test --package liquers-lib ui::shortcuts --lib

# Run integration tests
cargo test --package liquers-lib ui_shortcuts_integration --test '*'

# Run with output for debugging
cargo test --package liquers-lib ui::shortcuts --lib -- --nocapture

# Clippy checks
cargo clippy --package liquers-lib -- -D warnings

# Format check
cargo fmt --check --package liquers-lib

# Documentation tests
cargo test --package liquers-lib --doc ui::shortcuts
```

**Expected output:**

```
running 50 tests
test ui::shortcuts::tests::parse_valid_ctrl_s ... ok
test ui::shortcuts::tests::parse_case_insensitive ... ok
test ui::shortcuts::tests::display_ctrl_key ... ok
test ui::shortcuts::tests::roundtrip_serialize_deserialize ... ok
[... all tests ...]

test result: ok. 50 passed; 0 failed; 0 ignored

running 3 integration tests
test ui_shortcuts_integration::test_menu_spec_with_shortcuts ... ok
test ui_shortcuts_integration::test_cross_platform_serialization ... ok
test ui_shortcuts_integration::test_graceful_degradation_with_invalid_shortcuts ... ok

test result: ok. 3 passed; 0 failed; 0 ignored
```

**Success criteria:**
- All unit tests pass (50/50)
- All integration tests pass (3/3)
- No clippy warnings
- Code formatted correctly
- All documentation examples compile

---

## Test Coverage Summary

| Category | Test Count | Files | Coverage |
|----------|-----------|-------|----------|
| **Unit Tests** | 50+ | `liquers-lib/src/ui/shortcuts.rs` | Parsing, display, modifiers, keys, errors, serialization, edge cases |
| **Integration Tests** | 3 | `liquers-lib/tests/ui_shortcuts_integration.rs` | Menu system, cross-platform, error recovery |
| **Manual Validation** | 5 commands | CLI | Compilation, linting, formatting, documentation |

**Total test coverage:** 53+ automated tests + 5 manual validation steps

**Estimated test execution time:** ~2 seconds for unit tests, ~1 second for integration tests

---

## Review Checklist

### Examples
- ✅ 3 realistic scenarios provided
- ✅ User chose conceptual code (confirmed in Overview Table)
- ✅ Examples demonstrate core functionality (parsing, display, conversion, registry, validation)
- ✅ Examples use realistic data/parameters (YAML configs, menu systems, cross-platform scenarios)
- ✅ Expected outputs are documented

### Corner Cases
- ✅ Memory: Large registries, long strings, allocation failures, leaks
- ✅ Concurrency: Shared registries, race conditions, thread safety, WASM considerations
- ✅ Errors: Invalid input, missing components, case-insensitivity, batch parsing
- ✅ Serialization: Round-trip, platform variance, schema evolution, metadata
- ✅ Integration: UISpecElement, MenuBarSpec, YAML loading, future frameworks

### Test Coverage
- ✅ Unit tests cover happy path + error path (50+ tests)
- ✅ Integration tests cover end-to-end flows (3 tests)
- ✅ Manual validation commands provided (5 commands)
- ✅ Test coverage summary table included

### Overview Table
- ✅ Overview table present at top of document
- ✅ All examples and tests listed with purpose and coverage
- ✅ Consistent with template requirements

### Query Validation
- ⚠️ Not applicable - this is a utility library, no queries involved

---

## Next Steps

**STOP HERE.** Present Phase 3 to the user and WAIT for explicit approval.

The user must say "proceed" or "Proceed to next phase" before starting Phase 4. Any other response (feedback, questions, corrections, "looks good", "ok") is NOT approval — address the feedback and WAIT again.

After user says "proceed":
1. Start Phase 4: Implementation Plan
2. Use examples as validation criteria during implementation
3. Use test plan as quality gate before feature completion
