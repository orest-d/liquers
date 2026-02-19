# register_command! EnumArgument DSL Draft

## Status
Draft proposal for Issue 10 (`ENUM-ARGUMENT-TYPE`).

## Goal
Extend `register_command!` so command authors can define full `ArgumentType::Enum(EnumArgument)` metadata directly in macro DSL, including:
- aliases
- typed underlying values
- optional fallback for non-listed values (`others_allowed`)
- reusable global enums

This should reduce command boilerplate and make enum choices visible to UI/introspection.

## Design Principles
- Keep current parameter shape (`name: Type ...`) intact.
- Add enum behavior as explicit parameter metadata (`enum: ...`) to avoid grammar ambiguity with Rust types.
- Support full `EnumArgument` model (`name`, `values`, `value_type`, `others_allowed`).
- Preserve backward compatibility: parameters without `enum:` behave exactly as today.

## Proposed DSL

### 1. Inline string enum (common case)
```rust
register_command!(cr,
    fn resize(state,
        method: String = "lanczos3" (
            label: "Interpolation",
            enum: ["nearest", "triangle", "catmullrom", "gaussian", "lanczos3"]
        )
    ) -> result
)?;
```

Semantics:
- `argument_type = ArgumentType::Enum(EnumArgument { ... })`
- each listed string becomes alias + value (same string)
- `value_type = EnumArgumentType::String`
- `others_allowed = false`

### 2. Alias -> value mapping
```rust
register_command!(cr,
    fn rotate(state,
        mode: String = "bilinear" (
            enum: {
                "nearest" => "nearest",
                "linear"  => "bilinear",
                "hq"      => "lanczos3"
            }
        )
    ) -> result
)?;
```

Semantics:
- alias is what user types in query
- mapped value is what command receives after expansion

### 3. Typed enum values (int/float/bool/string)
```rust
register_command!(cr,
    fn quality(state,
        preset: i32 = 2 (
            enum(type: int): {
                "low" => 1,
                "med" => 2,
                "high" => 3
            }
        )
    ) -> result
)?;
```

Supported `enum(type: ...)` values:
- `string`
- `int`
- `int_opt`
- `float`
- `float_opt`
- `bool`
- `any`

Type validation:
- mapped values must match declared enum type
- if omitted, type defaults to inferred type from mapped literals

### 4. Allow non-listed values (`others_allowed`)
```rust
register_command!(cr,
    fn color(state,
        color: String (
            enum(type: string, others: true): ["red", "green", "blue"]
        )
    ) -> result
)?;
```

Semantics:
- listed aliases are expanded if matched
- otherwise raw argument is accepted if compatible with `value_type`

### 5. Global enum reference
```rust
register_command!(cr,
    fn resize(state,
        method: String = "lanczos3" (
            enum_ref: "img.resize_method"
        )
    ) -> result
)?;
```

Semantics:
- `argument_type = ArgumentType::GlobalEnum("img.resize_method".to_string())`
- resolved at runtime via `CommandMetadataRegistry::get_global_enum`

## Grammar Additions (parameter metadata)

Inside parameter metadata block:

```text
enum: ["a", "b", "c"]
enum: { "alias" => <literal_or_query>, ... }
enum(type: <enum_type>[, others: <bool>]): [ ... ]
enum(type: <enum_type>[, others: <bool>]): { ... }
enum_ref: "namespace.name"
```

`<literal_or_query>`:
- string literal
- bool literal
- int literal
- float literal
- `query "..."` (stored as `CommandParameterValue::Query`)

## Interaction With Existing Fields
- `default` remains `= ...` as today.
- `default` must be valid for enum argument:
  - either one of aliases
  - or compatible free value when `others: true`
- explicit `gui:` still allowed.
- if `gui:` omitted and enum is present, UI default is inferred:
  - <=3 alternatives: `VerticalRadioEnum`
  - >=4 alternatives: `EnumSelector`

## Macro Expansion Requirements

For enum parameters, generated metadata must include:
- `ArgumentInfo.argument_type = ArgumentType::Enum(...)` or `GlobalEnum(...)`
- `ArgumentInfo.default` unchanged (existing representation)

Runtime value extraction must:
1. read raw argument value
2. if enum/global enum exists, resolve alias using `EnumArgument::expand_alias`
3. enforce `others_allowed` + `value_type` rules
4. return converted typed value to function parameter

## Errors

Compile-time macro errors:
- malformed enum syntax
- duplicate aliases in inline enum
- mixed mapped value types without `type: any`
- `enum_ref` used together with `enum`
- `default` incompatible with enum definition

Runtime errors:
- unknown enum alias when `others: false`
- invalid fallback value for declared `value_type`
- missing global enum name

## Backward Compatibility
- no behavior changes for existing commands
- existing `String` manual validation continues to work
- enum support is opt-in per parameter

## Suggested Implementation Phases
1. Parse metadata statements `enum` and `enum_ref` in `liquers-macro`.
2. Generate `ArgumentType::Enum` / `ArgumentType::GlobalEnum` metadata.
3. Add runtime alias expansion + validation path in generated extractor code.
4. Add docs/examples in:
   - `specs/REGISTER_COMMAND_FSD.md`
   - `specs/COMMAND_REGISTRATION_GUIDE.md`
5. Add tests:
   - parser unit tests
   - expansion codegen tests
   - integration tests for alias resolution and errors

## Non-Goals (for first iteration)
- nested enum objects
- enum labels/descriptions per alternative (future: extend `EnumArgumentAlternative`)
- auto-generation of Rust enum types
