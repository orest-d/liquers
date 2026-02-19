# Phase 2: Solution & Architecture - Keyboard Shortcuts

## Overview

Platform-independent keyboard shortcuts library implemented as a new module `liquers_lib::ui::shortcuts`. Core types mirror egui's KeyboardShortcut structure but with **semantic modifiers** for true cross-platform support. Provides bidirectional conversions to egui types (immediate), with future support for ratatui/dioxus via feature flags.

## Design Rationale

### Semantic "Command" Modifier

**Problem**: Different platforms use different primary command keys:
- macOS: ⌘ Command key (Cmd)
- Windows/Linux: Control key (Ctrl)

**Solution**: The `ctrl` field in `Modifiers` is **semantic**, not physical:
- Represents "the platform's primary command modifier"
- One shortcut definition (`"Ctrl+S"`) works on all platforms
- Displays appropriately: "Cmd+S" on macOS, "Ctrl+S" elsewhere
- Matches industry standard (VS Code, IntelliJ, egui)

**Benefit**: Users write platform-independent shortcuts without manual per-platform definitions.

### WASM Compatibility

**Challenge**: WASM apps compile to `wasm32-unknown-unknown` target, so `cfg!(target_os = "macos")` is always false, even when running in Safari on macOS.

**Solution**: Delegate platform detection to egui/dioxus/browser:
- Our library: Maps semantic `ctrl` to egui's semantic `command`
- egui native: Uses compile-time `cfg!` for platform detection
- egui WASM: Uses runtime browser `KeyboardEvent.metaKey` / `ctrlKey`
- Browser: Provides correct platform-aware events on macOS/Windows/Linux

**Result**: Shortcuts work correctly in WASM on all platforms without compile-time platform checks.

### Modifiers Included

**Supported (universal modifiers)**:
- `ctrl` - Semantic command modifier (Cmd on macOS, Ctrl elsewhere)
- `alt` - Alt/Option key (universal)
- `shift` - Shift key (universal)

**Not supported** (Phase 1 scope):
- **AltGr**: Primarily for character input, not shortcuts; non-existent on macOS; industry practice avoids it
- **Super/Win/Meta as distinct from Ctrl**: Rare use case; can be added in future if needed
- **Function/Hyper**: Niche modifiers; not supported by target UI frameworks

**Rationale**: Focus on universal modifiers that work across all platforms and match user expectations.

## Data Structures

### Core Struct: KeyboardShortcut

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyboardShortcut {
    pub modifiers: Modifiers,
    pub key: Key,
}
```

**Ownership rationale:**
- All fields are owned and `Copy` or small - no Arc/Box needed
- `Clone` is cheap (entire struct fits in ~16 bytes)
- `PartialEq + Eq + Hash` enable use in HashMaps for shortcut registries

**Serialization:**
- Implements `Serialize` and `Deserialize` via string representation
- Format: "Ctrl+S", "Cmd+Shift+A", "Alt+F4"
- Delegates to `FromStr` and `Display` implementations

### Modifiers Struct

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub alt: bool,
    pub shift: bool,
    pub ctrl: bool,  // SEMANTIC: "command modifier" (Cmd on macOS, Ctrl elsewhere)
}
```

**Ownership rationale:**
- Derives `Copy` (3 bytes total, well under 24-byte Copy threshold)
- Pass by value in functions (cheaper than borrowing)
- `Default` provides empty modifiers

**Platform awareness (SEMANTIC DESIGN):**
- `ctrl` is **semantic**, not physical - represents "the platform's primary command modifier"
- On macOS: `ctrl: true` means **⌘ Command key**
- On Windows/Linux: `ctrl: true` means **Ctrl key**
- This matches industry standard: same shortcut definition works everywhere

**String representation:**
- **Parsing** (liberal): "Ctrl", "Cmd", "Command" all parse to `ctrl: true`
- **Display** (platform-aware): `ctrl: true` displays as:
  - macOS: "Cmd+S"
  - Windows/Linux: "Ctrl+S"
- **Round-trip**: `"Ctrl+S".parse().to_string()` → `"Cmd+S"` on macOS, `"Ctrl+S"` elsewhere

**Serialization:**
- Part of KeyboardShortcut string representation
- Serializes using platform-appropriate display name
- No separate serialization (always serialized as part of parent shortcut)

**Design rationale:**
This semantic approach follows industry best practices (VS Code, egui, etc.) where:
- Users write ONE shortcut definition: `"Ctrl+S"`
- It works on ALL platforms automatically
- No need for platform-specific shortcut definitions
- Matches user expectations: Save is ⌘S on Mac, Ctrl+S on Windows

### Key Enum

```rust
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

    // Punctuation (common ones)
    Comma, Period, Slash, Backslash,
    Semicolon, Quote, Backtick,
    Minus, Equals,
    LeftBracket, RightBracket,

    // Special
    PrintScreen, ScrollLock, Pause,
}
```

**Variant semantics:**
- Covers common keyboard keys (alphanumeric, function, navigation, editing)
- Does **not** cover every possible key (keeps enum manageable)
- Names follow web standard KeyboardEvent.code convention where possible
- Case-insensitive parsing: "a", "A", "KeyA" all map to Key::A

**No default match arm:** All match statements on Key must be explicit.

**Ownership:**
- Derives `Copy` (enum discriminant only, no data)
- Pass by value always

### ExtValue Extensions

**Not applicable.** Shortcuts are UI metadata, not value types.

## Trait Implementations

### Trait: FromStr

**Implementor:** `KeyboardShortcut`

```rust
impl FromStr for KeyboardShortcut {
    type Err = liquers_core::error::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Parse "Ctrl+S", "Cmd+Shift+A", etc.
        // Uses Error::general_error() for failures
        // Implementation in Phase 4
    }
}
```

**Parsing rules:**
- Split on '+' delimiter
- Modifier order doesn't matter: "Ctrl+Shift+S" == "Shift+Ctrl+S"
- Case-insensitive: "ctrl" == "Ctrl" == "CTRL"
- **Name-based detection**: Each token is checked against known modifiers (Ctrl/Cmd/Alt/Shift) and keys (A-Z, F1-F12, etc.)
- Exactly one token must be a valid key; all others must be valid modifiers
- **Semantic command modifier**: "Ctrl", "Cmd", "Command", "Meta" all parse to `ctrl: true`
- Key aliases: "Esc"/"Escape", "Return"/"Enter"
- **Platform-independent**: `"Ctrl+S"` parses the same on all platforms (displays differently)

**Examples:**
- `"Ctrl+S"` → `Modifiers { ctrl: true, .. }` + `Key::S`
- `"Cmd+S"` → `Modifiers { ctrl: true, .. }` + `Key::S` (same as Ctrl)
- `"Shift+Alt+A"` → `Modifiers { shift: true, alt: true, .. }` + `Key::A`

**Bounds:** None

### Trait: Display

**Implementor:** `KeyboardShortcut`

```rust
impl Display for KeyboardShortcut {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // Format as "Ctrl+S", "Cmd+Shift+A"
        // Implementation in Phase 4
    }
}
```

**Formatting rules:**
- Modifiers first (order: Ctrl, Alt, Shift), then key
- **Platform-aware display**: `ctrl: true` displays as:
  - macOS native: "Cmd" (⌘ Command key)
  - Windows/Linux: "Ctrl" (Control key)
  - WASM: "Ctrl" (compile-time detection unavailable; acceptable fallback)
- Canonical key names: "Escape" not "Esc", "Enter" not "Return"
- Round-trip: `s.parse().to_string()` preserves **meaning** (not exact string)

**Examples:**
- macOS native: `Modifiers { ctrl: true, shift: true }` + `Key::S` → `"Cmd+Shift+S"`
- Windows/Linux: `Modifiers { ctrl: true, shift: true }` + `Key::S` → `"Ctrl+Shift+S"`
- WASM: `Modifiers { ctrl: true, shift: true }` + `Key::S` → `"Ctrl+Shift+S"` (always Ctrl)
- All platforms: `Modifiers { alt: true }` + `Key::F4` → `"Alt+F4"`

**Note**: WASM apps that need platform-aware display in UI can use JavaScript to detect platform at runtime and customize the display layer. The important part (actual shortcut matching via `to_egui()`) works correctly in WASM.

### Trait: Serialize / Deserialize

**Implementors:** `KeyboardShortcut`, `Modifiers`, `Key`

```rust
impl Serialize for KeyboardShortcut {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for KeyboardShortcut {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}
```

**Serialization format:**
- KeyboardShortcut: string representation ("Ctrl+S")
- Modifiers: not serialized independently (only as part of KeyboardShortcut)
- Key: not serialized independently (only as part of KeyboardShortcut)

**Rationale:** String format is human-readable, platform-independent, and matches existing YAML usage in ui_spec_element.rs

## Generic Parameters & Bounds

**Not applicable.** All types are concrete (no generics).

## Sync vs Async Decisions

| Function | Async? | Rationale |
|----------|--------|-----------|
| `KeyboardShortcut::from_str` | No | Pure parsing, no I/O |
| `KeyboardShortcut::to_egui` | No | Simple type conversion |
| Conflict detection helpers | No | In-memory HashMap operations |

**All functions are synchronous.** No I/O, no async contexts involved.

## Function Signatures

### Module: liquers_lib::ui::shortcuts

```rust
// ============================================================================
// Core types (defined above)
// ============================================================================

pub struct KeyboardShortcut { /* ... */ }
pub struct Modifiers { /* ... */ }
pub enum Key { /* ... */ }

// ============================================================================
// Constructor and utilities
// ============================================================================

impl KeyboardShortcut {
    /// Create a new keyboard shortcut
    pub const fn new(modifiers: Modifiers, key: Key) -> Self {
        Self { modifiers, key }
    }

    /// Parse from string, returning error details
    ///
    /// Uses `liquers_core::error::Error::general_error()` for failures.
    pub fn parse(s: &str) -> Result<Self, liquers_core::error::Error> {
        s.parse()
    }
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

    /// Command modifier (semantic: Cmd on macOS, Ctrl elsewhere)
    /// This is the same as `Modifiers { ctrl: true, ..Modifiers::none() }`
    /// Provided as a convenience for clarity.
    pub const fn command() -> Self {
        Self { ctrl: true, alt: false, shift: false }
    }
}

impl Key {
    /// Parse key from string (case-insensitive, with aliases)
    pub fn from_name(name: &str) -> Option<Self> {
        // Implementation in Phase 4
        // Handles: "A", "a", "KeyA", "Enter", "Return", "Esc", "Escape", etc.
    }

    /// Canonical name for display
    pub const fn name(&self) -> &'static str {
        // Implementation in Phase 4
        // Returns: "A", "Enter", "Escape", "ArrowUp", etc.
    }
}

// ============================================================================
// Conversion to egui types
// ============================================================================

impl KeyboardShortcut {
    /// Convert to egui::KeyboardShortcut
    pub fn to_egui(&self) -> egui::KeyboardShortcut {
        egui::KeyboardShortcut::new(self.modifiers.to_egui(), self.key.to_egui())
    }
}

impl Modifiers {
    /// Convert to egui::Modifiers (WASM-safe)
    ///
    /// Maps our semantic `ctrl` field to egui's smart `command` field.
    /// We delegate platform detection to egui, which handles it correctly
    /// for both native (compile-time cfg) and WASM (runtime browser detection).
    pub fn to_egui(&self) -> egui::Modifiers {
        egui::Modifiers {
            alt: self.alt,
            shift: self.shift,
            command: self.ctrl,  // Semantic → semantic, egui handles platform
            // Don't set ctrl/mac_cmd - let egui populate from actual events
            ..Default::default()
        }
    }
}

impl Key {
    /// Convert to egui::Key
    pub fn to_egui(&self) -> egui::Key {
        // Implementation in Phase 4
        // Maps each Key variant to corresponding egui::Key variant
    }
}

// ============================================================================
// Conversion from egui types
// ============================================================================

impl From<egui::KeyboardShortcut> for KeyboardShortcut {
    fn from(shortcut: egui::KeyboardShortcut) -> Self {
        Self {
            modifiers: Modifiers::from_egui(shortcut.modifiers),
            key: Key::from_egui(shortcut.logical_key),
        }
    }
}

impl Modifiers {
    /// Convert from egui::Modifiers
    ///
    /// Maps egui's smart `command` field to our semantic `ctrl` field.
    pub fn from_egui(m: egui::Modifiers) -> Self {
        Self {
            alt: m.alt,
            shift: m.shift,
            // Use egui's smart command field (Cmd on macOS, Ctrl elsewhere)
            ctrl: m.command,
        }
    }
}

impl Key {
    /// Convert from egui::Key
    pub fn from_egui(key: egui::Key) -> Option<Self> {
        // Implementation in Phase 4
        // Returns None for keys not in our Key enum (e.g., Numpad keys)
    }
}

// ============================================================================
// Conflict detection utilities
// ============================================================================

/// Detect duplicate shortcuts in a collection
pub fn find_conflicts<'a, I>(shortcuts: I) -> Vec<(KeyboardShortcut, usize)>
where
    I: IntoIterator<Item = &'a KeyboardShortcut>,
{
    // Implementation in Phase 4
    // Returns shortcuts that appear more than once with their counts
}

/// Helper for menu spec validation (used by ui_spec_element.rs)
///
/// Parse each string, return those that fail with error details.
pub fn validate_shortcut_strings<'a, I>(shortcuts: I) -> Vec<(String, liquers_core::error::Error)>
where
    I: IntoIterator<Item = &'a str>,
{
    // Implementation in Phase 4
    // Returns (shortcut_string, parse_error) for invalid shortcuts
}
```

**Parameter choices:**
- `&str` for string inputs (borrowed, no allocation)
- `Modifiers`, `Key` passed by value (Copy types, cheaper than borrowing)
- Return owned `KeyboardShortcut` (small struct, cheap to copy)
- Error type uses `liquers_core::error::Error` (project standard)

## Integration Points

### Crate: liquers-lib

**New file:** `liquers-lib/src/ui/shortcuts.rs`

**Exports:**
```rust
// Re-export in liquers-lib/src/ui/mod.rs
pub mod shortcuts;
pub use shortcuts::{KeyboardShortcut, Modifiers, Key, find_conflicts, validate_shortcut_strings};
```

**Modify:** `liquers-lib/src/ui/widgets/ui_spec_element.rs`

Replace `check_shortcut` and `parse_key` methods with shortcuts library:

```rust
use crate::ui::shortcuts::KeyboardShortcut;

impl UISpecElement {
    fn check_shortcut(&self, ui: &egui::Ui, shortcut_str: &str) -> bool {
        match KeyboardShortcut::parse(shortcut_str) {
            Ok(shortcut) => {
                let egui_shortcut = shortcut.to_egui();
                ui.input_mut(|i| i.consume_shortcut(&egui_shortcut))
            }
            Err(_) => false,
        }
    }
}
```

**Modify:** `liquers-lib/src/ui/widgets/ui_spec_element.rs` - validation

Replace manual conflict detection with shortcuts library:

```rust
use crate::ui::shortcuts::find_conflicts;

impl MenuBarSpec {
    pub fn validate_shortcuts(&self) -> Vec<(String, usize)> {
        // Extract shortcut strings
        let mut shortcut_strings = Vec::new();
        // ... (collect shortcuts as before)

        // Parse and detect conflicts
        let parsed: Vec<_> = shortcut_strings.iter()
            .filter_map(|s| KeyboardShortcut::parse(s).ok())
            .collect();

        let conflicts = find_conflicts(parsed.iter());
        conflicts.into_iter()
            .map(|(sc, count)| (sc.to_string(), count))
            .collect()
    }
}
```

### Dependencies

**No new dependencies needed!**

All required dependencies already present in `liquers-lib/Cargo.toml`:
```toml
[dependencies]
liquers-core = { path = "../liquers-core" }  # For Error type
egui = "0.33.0"  # Already present for conversions
serde = { version = "1.0", features = ["derive"] }  # Already present
```

**Rationale:**
- Uses `liquers_core::error::Error` (project standard, no thiserror needed)
- `egui` already present for conversions
- `serde` already present for serialization

## Relevant Commands

### New Commands

**None.** This is a utility library, not a command namespace.

### Relevant Existing Namespaces

| Namespace | Relevance | Key Commands |
|-----------|-----------|--------------|
| `lui` | UI commands that create elements with shortcuts | `ui_spec`, `add-*` |

**Note:** No new commands added. Existing `lui/ui_spec` command benefits from improved shortcut parsing (via ui_spec_element.rs integration).

## Web Endpoints (if applicable)

**Not applicable.** This is a UI utility library with no web exposure.

Future: If web UI is added, shortcut configuration could be exposed via API for customization.

## Error Handling

### Error Type: liquers_core::error::Error

Uses `liquers_core::error::Error` with typed constructors (project standard):

```rust
use liquers_core::error::{Error, ErrorType};

// Parse errors use Error::general_error()
pub fn parse(s: &str) -> Result<KeyboardShortcut, Error> {
    if s.is_empty() {
        return Err(Error::general_error("Empty shortcut string".to_string()));
    }
    // ... more parsing
    Err(Error::general_error(format!("Unknown key: {}", key_name)))
}
```

**Error scenarios:**

| Scenario | Constructor | Example |
|----------|-------------|---------|
| Empty string | `Error::general_error("Empty shortcut string")` | `"".parse::<KeyboardShortcut>()` |
| Unknown key | `Error::general_error(format!("Unknown key: {}", key))` | `"Ctrl+Foo".parse()` |
| Unknown modifier | `Error::general_error(format!("Unknown modifier: {}", mod))` | `"Foo+S".parse()` |
| Invalid format | `Error::general_error("No + separator found")` | `"CtrlS".parse()` |

### Error Propagation

```rust
use liquers_core::error::Error;

// Use ? operator for Result propagation
pub fn parse_multiple(strings: &[&str]) -> Result<Vec<KeyboardShortcut>, Error> {
    strings.iter()
        .map(|s| s.parse())
        .collect()  // Short-circuits on first error
}
```

**Consistent with liquers patterns** - all errors use `liquers_core::error::Error` with typed constructors.

## Serialization Strategy

### Serde Implementation

**String-based serialization** (human-readable, platform-independent):

```rust
// KeyboardShortcut serializes as "Ctrl+S"
#[derive(Serialize, Deserialize)]
struct Config {
    quit_shortcut: KeyboardShortcut,  // YAML: "Ctrl+Q"
    save_shortcut: KeyboardShortcut,  // YAML: "Cmd+S"
}
```

**YAML example:**
```yaml
menu:
  items:
  - !button
    label: Save
    shortcut: "Ctrl+S"  # Parsed via KeyboardShortcut::deserialize
```

**Round-trip compatibility:**
```rust
let original = "Cmd+Shift+S";
let shortcut: KeyboardShortcut = original.parse().unwrap();
let serialized = shortcut.to_string();
// serialized might normalize to "Shift+Cmd+S" (same meaning, canonical order)
```

### Web Format Support

For browser compatibility, also support parsing web KeyboardEvent.code format:

```rust
// Both formats parse to same result:
"Control+KeyS".parse::<KeyboardShortcut>()  // Web format
"Ctrl+S".parse::<KeyboardShortcut>()        // Human format
```

## Concurrency Considerations

### Thread Safety

**All types are `Send + Sync`:**
- `KeyboardShortcut` - no interior mutability, all fields are Copy
- `Modifiers` - Copy type with only bool fields
- `Key` - Copy enum with no data

**No locks needed:**
- All operations are immutable reads or owned data
- Parsing creates new instances (no shared state)
- Conflict detection builds temporary HashMap (thread-local)

**Safe to use from any thread:**
```rust
// Safe: can share shortcuts across threads
let shortcut = Arc::new(KeyboardShortcut::parse("Ctrl+S").unwrap());
let shortcut_clone = Arc::clone(&shortcut);
std::thread::spawn(move || {
    println!("{}", shortcut_clone);  // OK
});
```

## Compilation Validation

**Expected to compile:** Yes (no external runtime dependencies besides egui, serde)

**Clippy checks:**
```bash
cargo clippy --package liquers-lib --features egui -- -D warnings
```

**Expected lints:**
- None (follows Apollo best practices: no cloning, Copy for small types, &str parameters)

## References to liquers-patterns.md

- [x] Crate dependencies: liquers-lib only (correct placement)
- [x] No new commands (utility library)
- [x] Error handling uses `liquers_core::error::Error` with typed constructors (project standard)
- [x] No unwrap/expect in function signatures
- [x] All functions synchronous (no I/O)
- [x] Serialization uses serde derives
- [x] Trait implementations for std traits (FromStr, Display)
- [x] Copy types passed by value (Modifiers, Key)
- [x] &str parameters (not String)
- [x] No default match arms (explicit Key enum matching)

**Pattern alignment:** Follows UI utility pattern (like colors, fonts). Not part of query flow, asset system, or store system.
