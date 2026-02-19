# register_command! EnumArgument Implementation Plan

## Scope
Implement Issue 10 (`ENUM-ARGUMENT-TYPE`) by extending `register_command!` DSL and generated wrappers to support full `EnumArgument` metadata and validation behavior described in `spec/register_command_enum/DRAFT.md`.

## Objectives
- Add enum DSL syntax to parameter metadata.
- Generate `ArgumentType::Enum(...)` and `ArgumentType::GlobalEnum(...)`.
- Apply runtime alias expansion and enum validation before typed extraction.
- Document the new syntax and behavior in canonical specs.
- Add tests for parser, codegen, and integration behavior.

## Deliverables
- Macro/parser and codegen changes in `liquers-macro/src/lib.rs`.
- Supporting enum resolution/validation path in command argument extraction flow (wrapper-generated or core helper).
- Updated docs:
  - `specs/REGISTER_COMMAND_FSD.md`
  - `specs/COMMAND_REGISTRATION_GUIDE.md`
- Tests in `liquers-macro` and `liquers-core`.

## Phase 1: DSL and Parser Model

### 1.1 Add internal AST for enum metadata
- Introduce parser-side structures in `liquers-macro/src/lib.rs`:
  - `EnumSpec` (inline enum or `enum_ref`)
  - `EnumTypeSpec` (`string|int|int_opt|float|float_opt|bool|any`)
  - `EnumEntries` (`list` or `map`)
  - `EnumValueLiteral` (str/bool/int/float/query)
- Extend `CommandParameter::Param` with optional enum spec field.

### 1.2 Extend parameter statement parser
- Add `CommandParameterStatement` variants:
  - `Enum(EnumSpec)` for `enum: ...` and `enum(type..., others...): ...`
  - `EnumRef(String)` for `enum_ref: "..."`
- Enforce parse-time constraints:
  - cannot set both `enum` and `enum_ref`
  - duplicate aliases rejected
  - malformed typed values rejected when explicit `type:` is provided

### 1.3 Default compatibility checks (compile-time)
- Validate default (`= ...`) against enum definition when possible:
  - alias allowed
  - mapped value literal allowed
  - non-listed literal only allowed when `others: true`

## Phase 2: Metadata Codegen

### 2.1 Generate `ArgumentType`
- Update `argument_type_expression()` for enum-enabled parameters:
  - inline enum -> `ArgumentType::Enum(EnumArgument { ... })`
  - `enum_ref` -> `ArgumentType::GlobalEnum(name)`
- Preserve existing behavior for non-enum parameters.

### 2.2 Auto GUI selection for enum params
- If enum present and no explicit `gui:`:
  - <=3 alternatives -> `VerticalRadioEnum`
  - >=4 alternatives -> `EnumSelector`
- If `gui:` explicitly set, keep explicit value.

### 2.3 Enum value typing normalization
- Convert enum value literals to `CommandParameterValue` consistently:
  - list syntax defaults alias->same-value
  - map syntax uses explicit mapped literal/query
- Infer `EnumArgumentType` when omitted; reject ambiguous mixed types unless `type: any`.

## Phase 3: Runtime Extraction and Validation

### 3.1 Alias expansion path
- Ensure extraction path applies enum alias expansion before converting to requested Rust type.
- For `GlobalEnum`, resolve using command metadata registry and apply same expansion.

### 3.2 `others_allowed` semantics
- When alias not found:
  - if `others_allowed = false`, return clear error with valid aliases
  - if `others_allowed = true`, accept raw value only if compatible with `value_type`

### 3.3 Error message quality
- Include argument name and sorted list of valid aliases for unknown enum values.
- Include expected enum type for fallback type mismatch.

## Phase 4: Testing

### 4.1 Macro parser unit tests (`liquers-macro`)
- Parse success cases:
  - list enum
  - map enum
  - typed enum
  - enum with `others: true`
  - enum ref
- Parse failure cases:
  - duplicate alias
  - `enum` + `enum_ref`
  - invalid type keyword
  - invalid mapped literal for declared type

### 4.2 Codegen snapshot/text tests (`liquers-macro`)
- Assert generated tokens include expected `ArgumentType::Enum`/`GlobalEnum`.
- Assert default GUI fallback selection for enum args.

### 4.3 Integration tests (`liquers-core/tests`)
- Alias expansion returns mapped values.
- Unknown alias rejection when `others_allowed = false`.
- Fallback acceptance/rejection when `others_allowed = true`.
- Global enum resolution success/failure.
- Default value behavior for enum arguments.

## Phase 5: Documentation Updates

### 5.1 Update `specs/REGISTER_COMMAND_FSD.md`
Add or revise sections:
- **Command Parameters / Syntax**:
  - add `enum` and `enum_ref` in parameter metadata grammar
- **Supported Types**:
  - clarify enum is metadata-driven and may map to typed values
- **Parameter Metadata**:
  - new statement docs:
    - `enum: ["..."]`
    - `enum: {"alias" => ...}`
    - `enum(type: ..., others: ...): ...`
    - `enum_ref: "..."`
- **Validation behavior**:
  - alias expansion, fallback, and error rules
- **GUI defaults**:
  - enum default rendering when `gui:` omitted
- **Examples**:
  - image resize method enum
  - integer quality preset enum
  - global enum reference example

### 5.2 Update `specs/COMMAND_REGISTRATION_GUIDE.md`
Add practical examples and guidance:
- **New subsection: Enum Parameters**
  - common inline string enum
  - alias->value mapping
  - typed enum (`i32`, `bool`)
  - `others: true` use case
  - `enum_ref` use case with registry-defined global enums
- **Migration note**:
  - replacing manual `String` validation with enum metadata
- **Troubleshooting**:
  - common compile-time DSL errors
  - runtime unknown alias errors and fixes

## Proposed Work Breakdown (PR-sized)
1. PR1: Parser + metadata codegen for inline enum and enum_ref (no runtime expansion yet).
2. PR2: Runtime alias expansion/validation + integration tests.
3. PR3: Documentation updates in both spec files + final example polish.

## Acceptance Criteria
- Commands can define inline enum metadata and global enum references via DSL.
- Metadata introspection exposes correct enum alternatives/type/others flag.
- Runtime uses alias expansion and respects `others_allowed` behavior.
- Existing commands compile and run unchanged.
- `specs/REGISTER_COMMAND_FSD.md` and `specs/COMMAND_REGISTRATION_GUIDE.md` document the new syntax and examples.
- New tests pass.

## Risks and Mitigations
- Risk: Macro grammar ambiguity with existing type syntax.
  - Mitigation: keep enum syntax inside parameter metadata block only.
- Risk: Drift between metadata and runtime behavior.
  - Mitigation: add integration tests that assert both metadata and execution semantics.
- Risk: Breaking existing parsing edge-cases.
  - Mitigation: keep existing tests intact and add targeted regression tests.
