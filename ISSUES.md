# Known Issues and Future Enhancements

## Metadata Enhancements

### Store Image Dimensions in State Metadata

**Status**: Design needed
**Category**: Architecture
**Priority**: Medium

**Description**:
Currently, image dimensions (width, height) are not stored in `State.metadata`. To retrieve dimensions, users must load the image and call dimension-related commands. For large images or when only metadata is needed, this is inefficient.

**Proposed Enhancement**:
When an image is loaded or transformed, store its dimensions in `State.metadata`. This would enable:
1. Fast dimension queries without deserializing the image
2. Metadata-based routing and caching decisions
3. Better integration with external systems that need image info

**Design Considerations**:
- **Where to store**: Add fields to `Metadata` struct or use key-value store pattern?
  - Option 1: `metadata.image_width` and `metadata.image_height` (type-specific fields)
  - Option 2: `metadata.properties["width"]` and `metadata.properties["height"]` (generic properties map)
  - Option 3: New `metadata.image_info` struct with width/height/color_type/etc.

- **When to populate**:
  - During image load commands (`from_bytes`, `from_format`, `svg_to_image`)
  - After transformations that change dimensions (`resize`, `crop`, `rotate`, etc.)
  - Lazy evaluation vs eager computation?

- **Consistency**: How to ensure metadata stays in sync with actual image?
  - What if image is modified but metadata isn't updated?
  - Should transformations always update metadata?

- **Backward compatibility**: How to handle existing assets without metadata?
  - Compute on-demand and cache?
  - Migration strategy?

- **Other metadata**: Beyond dimensions, consider:
  - Color type (RGB8, RGBA8, etc.)
  - File format (PNG, JPEG, etc.)
  - Color profile/EXIF data
  - Compression settings

**Related**:
- See `specs/IMAGE_COMMAND_LIBRARY.md` for image command library design
- See `liquers-core/src/metadata.rs` for current metadata structure
- Similar pattern might apply to Polars DataFrames (row count, column count, schema)

**Next Steps**:
1. Review existing `Metadata` structure and capabilities
2. Design metadata schema for image properties
3. Determine update strategy (eager vs lazy, consistency guarantees)
4. Implement in image command library
5. Consider generalizing pattern for other rich value types (DataFrames, etc.)

---

## Command Registration Enhancements

### Support EnumArgumentType in register_command! Macro

**Status**: Not implemented
**Category**: Macro / Command Framework
**Priority**: High
**Blocking**: Image command library implementation (IMAGE_COMMAND_LIBRARY.md)

**Description**:
The `register_command!` macro currently supports basic argument types (String, i32, u32, f32, bool, etc.) but lacks support for enum-style arguments where a parameter must be one of a predefined set of string values. This is needed for commands that accept method/format selection arguments.

**Use Cases**:
Many commands in the image library (and potentially other libraries) need to accept enum-style string arguments:

1. **Resize methods**: `resize-800-600-lanczos3`
   - Valid values: `nearest`, `triangle`, `catmullrom`, `gaussian`, `lanczos3`
   - Default: `lanczos3`

2. **Color formats**: `color_format-rgba8`
   - Valid values: `rgb8`, `rgba8`, `luma8`, `luma_alpha8`, `rgb16`, `rgba16`, `rgb32f`, `rgba32f`
   - No default (required parameter)

3. **Rotation methods**: `rotate-45-bilinear`
   - Valid values: `nearest`, `bilinear`
   - Default: `bilinear`

4. **Blur methods**: `blur-gaussian-2.5`
   - Valid values: `gaussian`, `box`, `median` (extensible)
   - Default: `gaussian`

**Current Workaround**:
Commands currently receive enum arguments as `String` and manually validate:

```rust
fn resize(state: &State<Value>, width: u32, height: u32, method: String) -> Result<Value, Error> {
    let filter = match method.as_str() {
        "nearest" => image::imageops::FilterType::Nearest,
        "triangle" => image::imageops::FilterType::Triangle,
        "catmullrom" => image::imageops::FilterType::CatmullRom,
        "gaussian" => image::imageops::FilterType::Gaussian,
        "lanczos3" => image::imageops::FilterType::Lanczos3,
        _ => return Err(Error::general_error(
            format!("Invalid resize method '{}'. Use: nearest, triangle, catmullrom, gaussian, lanczos3", method)
        )),
    };
    // ... rest of implementation
}
```

This is repetitive, error-prone, and doesn't provide command metadata about valid enum values.

**Proposed Enhancement**:
Add `EnumArgumentType` support to `register_command!` macro with the following syntax:

**Option 1: Inline enum definition**
```rust
register_command!(cr,
    fn resize(state, width: u32, height: u32,
              method: enum("nearest", "triangle", "catmullrom", "gaussian", "lanczos3") = "lanczos3"
                     (label: "Interpolation Method", gui: EnumSelector)) -> result
    label: "Resize image"
    doc: "Resize to exact dimensions with interpolation method"
    namespace: "img"
)?;
```

**Note**: Parameter metadata follows existing DSL pattern:
- `label: "..."` - Human-readable parameter label (for UI)
- `gui: <GuiInfo>` - UI rendering hint
- **No** `doc` field for individual parameters (only commands have `doc`)

**GUI defaults for enum arguments**:
- **Default**: `EnumSelector` (dropdown) for 4+ options
- **Automatic**: `VerticalRadioEnum` for 2-3 options (can be explicit)
- Can override with `gui: HorizontalRadioEnum` or other variants as needed

**Option 2: Named enum type**
```rust
// Define enum type (reusable)
enum_type!(ResizeMethod, "nearest", "triangle", "catmullrom", "gaussian", "lanczos3");

register_command!(cr,
    fn resize(state, width: u32, height: u32,
              method: ResizeMethod = "lanczos3" (label: "Interpolation Method")) -> result
    label: "Resize image"
    doc: "Resize to exact dimensions with interpolation method"
    namespace: "img"
)?;
```

**Note**: GUI would be inferred from enum definition (5 options → `EnumSelector`)

**Option 3: Rust enum mapping**
```rust
// Use actual Rust enum
#[derive(CommandEnum)]  // Custom derive macro
enum ResizeMethod {
    #[command_value("nearest")]
    Nearest,
    #[command_value("triangle")]
    Triangle,
    #[command_value("catmullrom")]
    CatmullRom,
    #[command_value("gaussian")]
    Gaussian,
    #[command_value("lanczos3")]
    Lanczos3,
}

register_command!(cr,
    fn resize(state, width: u32, height: u32,
              method: ResizeMethod = ResizeMethod::Lanczos3 (label: "Interpolation Method")) -> result
    label: "Resize image"
    doc: "Resize to exact dimensions with interpolation method"
    namespace: "img"
)?;
```

**Note**: GUI inferred from number of enum variants (5 → `EnumSelector`)

**Benefits**:
1. **Type safety**: Compile-time validation of enum values
2. **Better errors**: Framework can provide helpful error messages listing valid values
3. **Command metadata**: Enum values can be exposed via command introspection/help
4. **Less boilerplate**: No manual validation code in every command
5. **Auto-completion**: IDEs/tools can suggest valid enum values
6. **Consistency**: Uniform enum handling across all commands

**Design Considerations**:

1. **String vs Enum in Function Signature**:
   - Should command functions receive `String` (current) or custom enum type?
   - String is simpler but loses type safety
   - Enum requires conversion logic but provides compile-time checks

2. **Error Messages**:
   - Framework should generate errors like: `"Invalid value 'xyz' for argument 'method'. Valid values: nearest, triangle, catmullrom, gaussian, lanczos3"`
   - Include both argument name and valid options

3. **Metadata Storage**:
   - Store enum values in `CommandMetadata` for introspection
   - Enable help/documentation generation
   - Support for API schema generation

4. **Case Sensitivity**:
   - Should enum matching be case-sensitive or case-insensitive?
   - Recommendation: Case-insensitive for user convenience

5. **Extensibility**:
   - How to add new enum values without breaking backward compatibility?
   - Consider versioning or feature flags

6. **Default Values**:
   - Enum arguments should support defaults just like other types
   - Syntax: `method: EnumType = "default_value"`

7. **GUI Rendering**:
   - **Auto-selection based on enum size**:
     - 2-3 options: Default to `VerticalRadioEnum` (radio buttons)
     - 4+ options: Default to `EnumSelector` (dropdown)
   - **Explicit override**: Can specify `gui: HorizontalRadioEnum` or other variants
   - Examples:
     ```rust
     // 2 options: auto VerticalRadioEnum
     method: enum("nearest", "bilinear") = "bilinear" (label: "Method")

     // 2 options: explicit horizontal
     method: enum("nearest", "bilinear") = "bilinear" (label: "Method", gui: HorizontalRadioEnum)

     // 5 options: auto EnumSelector
     method: enum("nearest", "triangle", "catmullrom", "gaussian", "lanczos3") = "lanczos3" (label: "Method")

     // 5 options: override with vertical radio (not recommended for many options)
     method: enum("nearest", "triangle", "catmullrom", "gaussian", "lanczos3") = "lanczos3"
            (label: "Method", gui: VerticalRadioEnum)
     ```

8. **Enum Value Labels**:
   - **Status**: To be designed
   - **Question**: Should enum values have separate display labels?
     - Option A: Use string value as-is: `enum("nearest", "lanczos3")`
     - Option B: Support labels: `enum("nearest" (label: "Nearest Neighbor"), "lanczos3" (label: "Lanczos (High Quality)"))`
   - **Current approach**: Use string values directly for now
   - **Future**: May add label support for better UI presentation

**Parameter Metadata Integration**:

Based on existing DSL (see `REGISTER_COMMAND_FSD.md`), enum parameters follow the same metadata pattern as other types:

```rust
<name>: <Type> [injected] [= <default_value>] [(label: "...", gui: ...)]
```

For enum arguments:
```rust
method: enum("value1", "value2", "value3") = "value1" (label: "Method Label", gui: EnumSelector)
```

This generates `ArgumentInfo` with:
- `name`: "method"
- `label`: "Method Label" (or defaults to "method" if not specified)
- `default`: `CommandParameterValue::Value(Value::String("value1"))`
- `argument_type`: `ArgumentType::Enum { values: vec!["value1", "value2", "value3"] }`
- `gui_info`: `ArgumentGUIInfo::EnumSelector` (or auto-selected based on value count)
- `injected`: `false`
- `multiple`: `false`

**Implementation Approach**:

Recommended approach: **Option 1 (Inline enum)** with validation in macro

1. **Macro expansion** generates validation code
2. **Command receives String** (maintains current signature pattern)
3. **Framework validates** before calling command
4. **Metadata includes** enum values for introspection
5. **GUI info** auto-selected based on value count (2-3: `VerticalRadioEnum`, 4+: `EnumSelector`)

Example expansion:
```rust
// User writes:
register_command!(cr,
    fn resize(state,
              width: u32 (label: "Width"),
              height: u32 (label: "Height"),
              method: enum("nearest", "triangle", "catmullrom", "gaussian", "lanczos3") = "lanczos3"
                     (label: "Interpolation Method")) -> result
    label: "Resize Image"
    doc: "Resize to exact dimensions with interpolation method"
    namespace: "img"
)?;

// Macro expands to (simplified):
{
    use futures::FutureExt;

    #[allow(non_snake_case)]
    pub fn REGISTER__resize(
        registry: &mut CommandRegistry<CommandEnvironment>
    ) -> Result<&mut CommandMetadata, Error> {

        #[allow(non_snake_case)]
        fn resize__CMD_(
            state: &State<<CommandEnvironment as Environment>::Value>,
            arguments: CommandArguments<CommandEnvironment>,
            context: Context<CommandEnvironment>,
        ) -> Result<<CommandEnvironment as Environment>::Value, Error> {
            let width__par: u32 = arguments.get(0, "width")?;
            let height__par: u32 = arguments.get(1, "height")?;
            let method__par: String = arguments.get(2, "method")?;

            // Validate enum value
            let valid_methods = ["nearest", "triangle", "catmullrom", "gaussian", "lanczos3"];
            if !valid_methods.contains(&method__par.as_str()) {
                return Err(Error::general_error(format!(
                    "Invalid value '{}' for argument 'method'. Valid values: {}",
                    method__par, valid_methods.join(", ")
                )));
            }

            let res = resize(state, width__par, height__par, method__par);
            res
        }

        let mut cm = registry.register_command(
            CommandKey::new("img", "", "resize"),
            resize__CMD_
        )?;

        cm.with_label("Resize Image");
        cm.with_doc("Resize to exact dimensions with interpolation method");

        cm.arguments = vec![
            ArgumentInfo {
                name: "width".to_string(),
                label: "Width".to_string(),
                default: CommandParameterValue::None,
                argument_type: ArgumentType::Integer,
                multiple: false,
                injected: false,
                gui_info: ArgumentGUIInfo::IntegerField,
                ..Default::default()
            },
            ArgumentInfo {
                name: "height".to_string(),
                label: "Height".to_string(),
                default: CommandParameterValue::None,
                argument_type: ArgumentType::Integer,
                multiple: false,
                injected: false,
                gui_info: ArgumentGUIInfo::IntegerField,
                ..Default::default()
            },
            ArgumentInfo {
                name: "method".to_string(),
                label: "Interpolation Method".to_string(),
                default: CommandParameterValue::Value(Value::String("lanczos3".to_string())),
                argument_type: ArgumentType::Enum {
                    values: vec![
                        "nearest".to_string(),
                        "triangle".to_string(),
                        "catmullrom".to_string(),
                        "gaussian".to_string(),
                        "lanczos3".to_string(),
                    ],
                },
                multiple: false,
                injected: false,
                gui_info: ArgumentGUIInfo::EnumSelector,  // Auto-selected (5 options)
                ..Default::default()
            },
        ];

        cm.with_filename("");
        Ok(cm)
    }

    REGISTER__resize(cr)
}
```

**Key Generated Elements**:
1. **Validation code** in wrapper function (`resize__CMD_`)
2. **ArgumentType::Enum** with list of valid values
3. **Auto-selected GUI** based on value count (5 → `EnumSelector`)
4. **Default value** properly wrapped
5. **Parameter labels** from metadata
6. **Clear error messages** listing all valid values

**Testing Requirements**:

1. **Value Validation**:
   - Valid enum values accepted
   - Invalid enum values rejected with clear error message
   - Error message includes all valid values
   - Error message includes parameter name

2. **Default Values**:
   - Default values applied correctly
   - Optional enum params work without defaults
   - Required enum params enforce value presence

3. **Case Sensitivity**:
   - Case-insensitive matching works (if implemented)
   - Or explicit case-sensitive validation (current approach)

4. **Metadata Generation**:
   - `ArgumentType::Enum` correctly stores all values
   - Parameter labels correctly set
   - Defaults properly wrapped in `CommandParameterValue`

5. **GUI Auto-Selection**:
   - 2 options → `VerticalRadioEnum`
   - 3 options → `VerticalRadioEnum`
   - 4+ options → `EnumSelector`
   - Explicit GUI override works

6. **Multiple Enum Arguments**:
   - Multiple enum parameters in same command
   - Each validated independently
   - Each with correct GUI info

7. **Integration**:
   - Works with other parameter types (u32, f32, String, etc.)
   - Works with injected parameters
   - Works with context parameter
   - Works in async commands

8. **Command Registration**:
   - Command properly registered in namespace
   - Metadata accessible via command introspection
   - Help/documentation generation includes enum values

**Related**:
- See `specs/IMAGE_COMMAND_LIBRARY.md` for image commands requiring enum arguments
- See `specs/REGISTER_COMMAND_FSD.md` for current macro syntax
- See `specs/COMMAND_REGISTRATION_GUIDE.md` for command registration patterns
- See `liquers-core/src/command_metadata.rs` for metadata structure

**Examples from Image Library**:

```rust
// Example 1: Resize with 5-option enum (auto EnumSelector dropdown)
register_command!(cr,
    fn resize(state,
              width: u32 (label: "Width"),
              height: u32 (label: "Height"),
              method: enum("nearest", "triangle", "catmullrom", "gaussian", "lanczos3") = "lanczos3"
                     (label: "Interpolation Method", gui: EnumSelector)) -> result
    label: "Resize Image"
    doc: "Resize to exact dimensions with interpolation method"
    namespace: "img"
)?;

// Example 2: Color format with 6-option enum (auto EnumSelector)
register_command!(cr,
    fn color_format(state,
                    format: enum("rgb8", "rgba8", "luma8", "luma_alpha8", "rgb16", "rgba16")
                           (label: "Color Format")) -> result
    label: "Convert Color Format"
    doc: "Convert image to specified color format"
    namespace: "img"
)?;

// Example 3: Rotate with 2-option enum (auto VerticalRadioEnum)
register_command!(cr,
    fn rotate(state,
              angle: f32 (label: "Angle (degrees)"),
              method: enum("nearest", "bilinear") = "bilinear"
                     (label: "Rotation Method", gui: VerticalRadioEnum)) -> result
    label: "Rotate Image"
    doc: "Rotate image by arbitrary angle in degrees"
    namespace: "img"
)?;

// Example 4: Blur with explicit horizontal radio (override default)
register_command!(cr,
    fn blur(state,
            method: enum("gaussian", "box", "median") = "gaussian"
                   (label: "Blur Method", gui: HorizontalRadioEnum),
            sigma: f32 = 2.0 (label: "Sigma")) -> result
    label: "Blur Image"
    doc: "Apply blur filter to image"
    namespace: "img"
)?;

// Example 5: Resize by percentage (combining enum and other params)
register_command!(cr,
    fn resize_by(state,
                 percent: f32 (label: "Percentage", gui: FloatSlider(10.0, 200.0, 5.0)),
                 method: enum("nearest", "triangle", "catmullrom", "gaussian", "lanczos3") = "lanczos3"
                        (label: "Interpolation Method")) -> result
    label: "Resize by Percentage"
    doc: "Resize image by percentage (uniform scaling, e.g., 50 = half size, 200 = double)"
    namespace: "img"
)?;
```

**Key Points**:
- All parameters have `label` for UI clarity
- Enum parameters default GUI based on option count (2-3: radio, 4+: dropdown)
- Can explicitly override GUI with `gui: <GuiInfo>`
- Command-level `doc` provides overall documentation (no per-parameter `doc`)
- Labels are concise but clear for UI presentation

**Required Code Changes**:

1. **`liquers-core/src/command_metadata.rs`**:
   ```rust
   pub enum ArgumentType {
       Integer,
       Float,
       String,
       Boolean,
       Any,
       // Add new variant:
       Enum {
           values: Vec<String>,
       },
       // ... other variants
   }
   ```

2. **`liquers-macro/src/lib.rs`**:
   - Parse `enum("val1", "val2", ...)` syntax
   - Generate validation code in wrapper function
   - Create `ArgumentType::Enum` with values
   - Auto-select GUI based on value count
   - Support explicit GUI override

3. **GUI Info Handling**:
   - Already exists: `ArgumentGUIInfo::EnumSelector`, `VerticalRadioEnum`, `HorizontalRadioEnum`
   - Add auto-selection logic:
     ```rust
     let gui_info = if explicit_gui_provided {
         explicit_gui
     } else if enum_values.len() <= 3 {
         ArgumentGUIInfo::VerticalRadioEnum
     } else {
         ArgumentGUIInfo::EnumSelector
     };
     ```

4. **Validation Error Messages**:
   ```rust
   format!(
       "Invalid value '{}' for argument '{}'. Valid values: {}",
       actual_value,
       parameter_name,
       valid_values.join(", ")
   )
   ```

**Next Steps**:
1. Review current `register_command!` macro implementation in `liquers-macro/src/lib.rs`
2. Add `ArgumentType::Enum` variant to `command_metadata.rs`
3. Design and implement enum syntax parser in macro
4. Implement validation code generation
5. Implement GUI auto-selection logic
6. Add tests for enum validation and metadata
7. Update `REGISTER_COMMAND_FSD.md` with enum syntax
8. Update examples in `COMMAND_REGISTRATION_GUIDE.md`
9. Implement in image command library (`IMAGE_COMMAND_LIBRARY.md`)
10. Test with real image commands (resize, color_format, rotate, etc.)

**Summary**:

This enhancement adds first-class enum support to the `register_command!` macro, enabling:
- Type-safe parameter validation
- Better UI generation (auto-selected GUI based on option count)
- Improved error messages
- Command metadata introspection
- Reduced boilerplate code

**Syntax**:
```rust
method: enum("value1", "value2", "value3") = "default" (label: "Label", gui: EnumSelector)
```

**Auto GUI Selection**:
- 2-3 values: `VerticalRadioEnum` (radio buttons)
- 4+ values: `EnumSelector` (dropdown)
- Can override with explicit `gui: <GuiInfo>`

**Blocking**: Image command library implementation requires this feature for method/format selection in commands like `resize`, `color_format`, `rotate`, `blur`, etc.

---

## Future Issues

### Payload Injection - Command Registration Macro Enhancement

**Status:** Future enhancement
**Category:** Macro / Payload System
**Priority:** Medium
**Affects:** `liquers-macro`, `liquers-core`

**Description:**

Currently, to use the `injected` keyword with payload types or newtypes, users must manually implement `InjectedFromContext` for each type. This is verbose and error-prone due to Rust's trait coherence rules preventing blanket implementations.

**Current Limitation:**

```rust
// 1. Define payload type
#[derive(Clone)]
pub struct MyPayload {
    pub user_id: String,
    pub window_id: u64,
}

impl PayloadType for MyPayload {}

// 2. Manually implement InjectedFromContext (required!)
impl<E: Environment<Payload = MyPayload>> InjectedFromContext<E> for MyPayload {
    fn from_context(name: &str, context: Context<E>) -> Result<Self, Error> {
        context.get_payload_clone().ok_or(Error::general_error(format!(
            "No payload in context for injected parameter {}", name
        )))
    }
}

// 3. For newtypes, also manually implement InjectedFromContext
pub struct UserId(pub String);

impl ExtractFromPayload<MyPayload> for UserId {
    fn extract_from_payload(payload: &MyPayload) -> Result<Self, Error> {
        Ok(UserId(payload.user_id.clone()))
    }
}

impl InjectedFromContext<MyEnvironment> for UserId {
    fn from_context(_name: &str, context: Context<MyEnvironment>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload".to_string()))?;
        UserId::extract_from_payload(&payload)
    }
}
```

**Why This Is Necessary:**

Rust's trait coherence rules prevent having both:
- Blanket impl: `impl<E: Environment> InjectedFromContext<E> for E::Payload`
- User impl: `impl InjectedFromContext<MyEnv> for MyNewtype`

Because Rust cannot prove that `MyNewtype` is never `E::Payload` for some environment, even with the `PayloadType` marker trait.

**Proposed Solutions:**

**Option 1: Derive macro for payload types**
```rust
#[derive(Clone, PayloadType, InjectedFromContext)]
pub struct MyPayload {
    pub user_id: String,
    pub window_id: u64,
}
// Auto-generates InjectedFromContext impl
```

**Option 2: Helper macro**
```rust
impl_injected_payload!(MyPayload);
// Generates the boilerplate InjectedFromContext impl
```

**Option 3: Enhanced `register_command!` macro with field extraction**
```rust
register_command!(cr, fn my_cmd(
    state,
    user_id: String injected from payload.user_id,  // Extract field directly
    window_id: u64 injected from payload.window_id
) -> result)?;
```

This would eliminate the need for newtypes and manual `InjectedFromContext` implementations.

**Option 4: Code generation at registration time**
```rust
register_command_with_payload!(cr, fn my_cmd(
    state,
    user_id: extract String from payload.user_id,
    window_id: extract u64 from payload.window_id
) -> result)?;
```

**Recommended Approach:**

Option 3 (field extraction in macro) is most user-friendly and follows the existing `register_command!` DSL pattern. Implementation would:
1. Parse `injected from payload.field_name` syntax
2. Generate wrapper code to extract field from payload
3. Pass extracted value to command function
4. No need for newtypes or manual trait implementations

**Benefits:**
1. **Less boilerplate**: No manual trait implementations needed
2. **Type safety**: Compile-time field access validation
3. **Clearer intent**: Syntax explicitly shows field extraction
4. **Backward compatible**: Existing `injected` keyword still works for full payload

**Workaround Until Implemented:**

Users must manually implement `InjectedFromContext` for all payload types and newtypes as documented in `specs/PAYLOAD_GUIDE.md` and demonstrated in `liquers-core/tests/injection.rs`.

**Related Files:**
- `liquers-core/src/commands.rs` - Trait definitions
- `liquers-macro/src/lib.rs` - Command registration macro
- `liquers-core/tests/injection.rs` - Test examples showing manual implementation
- `specs/PAYLOAD_GUIDE.md` - User documentation
- `specs/REGISTER_COMMAND_FSD.md` - Macro syntax specification

---

### Payload Inheritance in Nested Evaluations

**Status:** Not implemented
**Category:** Context / Assets
**Priority:** Low
**Affects:** `liquers-core/src/context.rs`, `liquers-core/src/assets.rs`

**Description:**

When a command calls `context.evaluate()` to execute a nested query, the payload from the parent context is not automatically passed to the child query. This means nested queries cannot access injected parameters.

**Example:**
```rust
async fn parent_cmd(
    _state: State<Value>,
    user_id: UserId,  // Has access to payload
    context: Context<E>,
) -> Result<Value, Error> {
    // Nested query - will NOT have access to payload
    let child = context.evaluate(&parse_query("/-/child_cmd")?).await?;
    // child_cmd cannot use injected parameters!
}
```

**Why This Happens:**

`context.evaluate()` calls `asset_manager.get_asset()` which goes through the standard asset creation pipeline. This pipeline doesn't have access to the parent command's payload because:
1. Assets are shared across multiple users/contexts
2. The asset manager is designed to work without execution-specific context
3. Caching would be impossible if assets depended on ephemeral payload data

**Possible Solutions:**

1. **Add `context.evaluate_with_payload()`** - Explicitly pass payload to nested queries
   ```rust
   let child = context.evaluate_with_payload(
       &parse_query("/-/child_cmd")?,
       context.get_payload_clone()
   ).await?;
   ```

2. **Store payload in Context and thread through asset creation** - More invasive architectural change

3. **Document as intentional limitation** - Encourage users to pass data explicitly through query parameters or state

**Recommended Approach:**

Option 3 (document as limitation) is most pragmatic. Payload inheritance is conceptually problematic for caching and asset sharing. Users should pass data through:
- Query parameters: `/-/child_cmd-${value}`
- State transformation: Pass computed values via state chain

**Workaround:**

Pass data explicitly through query parameters or state instead of relying on payload inheritance.

**Related Files:**
- `liquers-core/src/context.rs` - Context implementation
- `liquers-core/src/assets.rs` - AssetManager
- `liquers-core/tests/injection.rs` - Test documenting limitation (test: `test_payload_not_inherited_in_nested_evaluation`)

---

### `register_command!` Parameter Index Misalignment with Injected Parameters

**Status:** Open (workaround available)
**Category:** Macro / Command Framework
**Priority:** High
**Affects:** `liquers-macro/src/lib.rs`

**Description:**

`extract_all_parameters()` (line ~557) uses `enumerate()` over all parameters including `Context`. `command_arguments_expression()` (line ~774) uses `filter_map` to exclude `Context` from metadata. When `Context` is not the last parameter, the extractor index `i` doesn't match the metadata/values index.

**Example:**

```rust
// BROKEN: context is not last
fn remove(state, context, target_word: String)
// Generates: arguments.get(1, "target_word")
// But metadata has target_word at index 0
```

**Fix:**

Use a separate counter for non-Context parameters in `extract_all_parameters()`:

```rust
let mut arg_index = 0;
for p in &self.parameters {
    extractors.push(p.parameter_extractor(arg_index));
    if !matches!(p, CommandParameter::Context) {
        arg_index += 1;
    }
}
```

**Workaround:** Always place `context` last in the macro DSL:

```rust
// CORRECT: context last
register_command!(cr,
    async fn remove(state, target_word: String, context) -> result
)?;
```

---

*Add new issues below this line*
