# Phase 2: Solution & Architecture — command-parameter-editor

## Overview

There are three layers, each usable without the next:

1. **Core** (`liquers-core/src/plan.rs`): `set_argument_parameter`, the scalar counterpart of the
   variadic edits of `variadic-argument-slots`. It shares their private padding helper.
2. **Model** (`liquers-lib/src/ui/parameter_form.rs`, no feature gate): `ParameterForm` parses the
   text, resolves the last action's `CommandMetadata`, builds fields with an effective
   `ArgumentGUIInfo`, applies edits through the core functions, and re-encodes the query.
3. **Renderers**: an egui form (`egui/parameter_form.rs`, feature `egui`) and a web form
   (`ui/web/parameter_form.rs`, feature `webui`). They are hosted by `QueryConsoleElement`.

Rejected alternatives:
- *A standalone element plus a `lui/parameter_editor` command.* It needs a target-binding
  mechanism and a registry change, and nothing asks for it yet (Phase 1, leading issue).
- *Editing text by positions.* Parser positions mark token starts only; edits are structural.
- *Validating values in the form.* Duplicates `validate_query`; the console's live analysis shows them.
- *A `FieldWidget` enum.* It would duplicate `ArgumentGUIInfo`, which renderers match exhaustively.

## Known-Issue Preflight

| Item | Relation | Blocks? |
|---|---|---|
| `variadic-argument-slots` (design, not implemented) | provides slots, list edits and padding | yes, Step 1 waits for its Steps 1-2 |
| `query-editor-live-validation` (design, not implemented) | provides `UIContext::command_registry` (its Step 3) | yes, Step 4 waits |
| `ui-events` (design, not implemented) | provides `UserEvent`, `field_attr` and `event_attr` | only Steps 6-7 |

Namespaces involved: none. No command is added or changed.

## Interfaces

`liquers-core/src/plan.rs` (sync, pure):

```rust
/// Sets scalar argument `argument` to `value`, writing omitted earlier arguments' defaults first
/// (as `insert_variadic_element`). `None` resets it: the parameter is removed when it is the last
/// written one, otherwise its default's text is written. Errors, action unchanged: unknown,
/// injected or `multiple` argument; excess parameters; an omitted earlier argument without default.
pub fn set_argument_parameter(action: &mut ActionRequest, metadata: &CommandMetadata,
    argument: &str, value: Option<ActionParameter>) -> Result<(), Error>;
```

`liquers-lib/src/ui/parameter_form.rs`:

```rust
#[derive(Debug, Clone)]
pub struct ParameterField {
    pub argument: String,              // ArgumentInfo::name; edits address fields by it
    pub label: String,                 // label, or name when empty
    pub description: String,
    pub gui: ArgumentGUIInfo,          // effective widget (AC-5)
    pub argument_type: ArgumentType,
    pub default: CommandParameterValue,
    pub multiple: bool,
    pub values: Vec<ActionParameter>,  // written parameters: 0..=1, or the list
}

#[derive(Debug, Clone)]
pub struct ParameterForm { /* query: Query, segment: usize, action: usize */
    pub command: CommandMetadata,
    pub fields: Vec<ParameterField>,
    pub excess: Vec<ActionParameter>,  // AC-8; non-empty refuses edits
}

impl ParameterForm {
    /// Err(reason) for AC-7: parse error, no action, unknown command.
    pub fn new(text: &str, registry: &CommandMetadataRegistry) -> Result<Self, Error>;
    pub fn set(&mut self, argument: &str, value: Option<ActionParameter>) -> Result<(), Error>;
    pub fn insert(&mut self, argument: &str, index: usize, value: ActionParameter) -> Result<(), Error>;
    pub fn remove(&mut self, argument: &str, index: usize) -> Result<ActionParameter, Error>;
    pub fn move_element(&mut self, argument: &str, from: usize, to: usize) -> Result<(), Error>;
    /// The edited query, `Query::encode`.
    pub fn query_text(&self) -> String;
}

/// AC-5: `gui_info` unless it is TextField/None and the type is Boolean(Option), Integer(Option),
/// Float(Option) or Enum/GlobalEnum, which give Checkbox, IntegerField, FloatField, EnumSelector.
pub fn effective_gui(argument: &ArgumentInfo) -> ArgumentGUIInfo;
```

Each edit applies the core function, then rebuilds `fields` from new slots (no stale positions).

Action and command: the last `ActionRequest` of the last `QuerySegment::Transform`, skipping
`ns`/`q` (`is_ns`, `is_q`), as `utils::find_next_presets` does. Metadata comes from
`PlanBuilder::new(query, registry).build()` then `Plan::last_action_command`, which follows aliases.
If the plan fails, the form falls back to the presets lookup (`active_namespace`,
`build_namespace_list` and `find_command_in_namespaces` in `liquers-lib/src/utils.rs`, made
`pub(crate)`). `resolve_global_enums(registry)` is applied, so `GlobalEnum` lists its alternatives.

egui (`liquers-lib/src/egui/parameter_form.rs`):

```rust
/// Renders the form; `changed()` when an edit was applied. Edit errors show inline.
pub fn edit_parameter_form(ui: &mut egui::Ui, form: &mut ParameterForm) -> egui::Response;
```

Widgets, matched exhaustively over `ArgumentGUIInfo`: text and code fields → `TextEdit`
(single- or multiline, monospace for code, width × 8 px); integer and float fields and ranges →
`DragValue`, sliders → `Slider`; `Checkbox` / `RadioBoolean` → checkbox / two radios; the three enum
widgets → radios or a `ComboBox` (free text when `others_allowed`); `ColorString` → text plus
`color_edit_button_srgba` when it parses as hex; `DateField` → text with a `YYYYMMDD` hint; `Hide` →
nothing; `None` → text.
An unparsable number keeps the text field rather than a `DragValue`. A link value uses
`edit_query` on its query text and is written back as `ActionParameter::new_link` when that text
parses (AC-6). A `multiple` field is a list: per-element widget, ↑ ↓ ✕, and "Add" (AC-3). A reset
button (↺) is shown on each written field (AC-4).

Web (`liquers-lib/src/ui/web/parameter_form.rs`, feature `webui`):

```rust
pub fn parameter_form_html(form: &ParameterForm) -> String;
```

Every field is an `<input>` or a `<textarea>`, the elements that `ui-events` reads. Enums use
`<input list>` with a `<datalist>`, booleans a `true/false` datalist, numbers and ranges `type=number`
or `range`, colours `type=color`, dates `type=date`. Names: `field_attr("arg.<name>")`, and
`arg.<name>.<i>` for a list element. Buttons: `event_attr(Named("params-apply"))`, and
`params-add.<name>`, `params-remove.<name>.<i>`, `params-up.<name>.<i>`,
`params-reset.<name>`. Every value is escaped.

## Integration Points

- `QueryConsoleElement`:
  - new `#[serde(default)] show_parameters: bool` (AC-11)
  - new `#[serde(skip)] parameter_form: Option<Result<ParameterForm, String>>`, cached with the text
    it was built from
  - `show_toolbar`: a "Parameters" toggle; when it is on, `edit_parameter_form` runs under the
    toolbar with `ctx.command_registry()`. A changed form sets `query_text = form.query_text()`
    without submitting (AC-10). With no registry, the reason line is shown (AC-7).
  - `render_web`: when the toggle is on, `parameter_form_html` goes under the input, together with a
    toggle button (`Named("params-toggle")`).
  - `update(User(Named(..)))`: first apply the `arg.*` fields to the form, then the named list
    operation. `params-apply` also sets `query_text` and calls `submit_query` (AC-9). The arms are
    exhaustive.
- `liquers-lib/src/utils.rs`: `active_namespace` and `build_namespace_list` become `pub(crate)`.
- Module declarations in `ui/mod.rs`, `egui/mod.rs`, `ui/web/mod.rs`.

## Error Handling

Edits return `Error::general_error` for an unknown argument or an out-of-range index, and the core
functions' `missing_argument` error for padding without a default. The form shows each error inline
and leaves the query unchanged. Building the form returns the parse or lookup error, and the console
shows its message (AC-7). No new `ErrorType`.

## Relevant Commands

None new or changed. `specs/command_registry.yaml` is unchanged. The fixtures use `search/field_extract`
and `pl/is_in`.

## Documentation Architecture

`UI_INTERFACE_FSD.md`: "Parameter form" (model, widget rule, console host, web events), with a History
row and `reviewed:`. `RECIPES_AND_PLANS.md` "Argument slots": `set_argument_parameter`, with a History
row. `COMMAND_REGISTRATION_GUIDE.md`: a sentence on `gui_info` being shown, with a History row.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `plan.rs`; new `ui/parameter_form.rs`, `egui/parameter_form.rs`, `ui/web/parameter_form.rs`; `query_console_element.rs`; `utils.rs`; the three `mod.rs` |
| Crates and workflows affected | `liquers-core` (additive), `liquers-lib` (`ui`, `egui`, `webui`) |
| Existing tests likely to change | none. Console serde tests gain the default field |
| New validation | core edit tests; form unit tests; a headless egui smoke test; SSR and event tests; one Playwright case |
| Compatibility | additive. The serialized console gains a defaulted field; `liquers-py` is unaffected |
| Behaviour | an edit normalises the spelling of the query (open question 4) |
| Performance | the form is rebuilt only when the text changes (cached); a plan per rebuild |
| Security | web markup escapes all values (`escape_html`) |
| Recovery | revert per layer. Nothing persisted except a boolean |
| Certainty | high for core and model; medium for the web half, until `ui-events` lands |
