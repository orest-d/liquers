# Phase 3: Examples & Use-cases — command-parameter-editor

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | turning on `flatten` and adding a field in the egui console | the Problem Example end to end | AC-2, AC-3, AC-10 |
| 2 | Example | the browser form applied | events and submission | AC-9 |
| 3 | Test | `liquers-core` `plan.rs` `set_argument_parameter_pads_omitted_defaults` | `-md--true` | AC-2 |
| 4 | Test | `plan.rs` `set_argument_parameter_reset_removes_or_writes_default` | the reset rule | AC-4 |
| 5 | Test | `plan.rs` `set_argument_parameter_refusals_leave_action_unchanged` | multiple, injected, unknown, excess, no default | AC-8 |
| 6 | Test | `liquers-lib` `ui/parameter_form.rs` `form_lists_visible_arguments_in_order` | hidden and injected are skipped; labels | AC-1 |
| 7 | Test | `ui/parameter_form.rs` `effective_gui_follows_type_for_text_widgets` | bool, int, float, enum, declared | AC-5 |
| 8 | Test | `ui/parameter_form.rs` `form_scalar_and_list_edits_produce_planned_queries` | the texts; `validate_query` is `Ok` | AC-2, AC-3 |
| 9 | Test | `ui/parameter_form.rs` `form_enum_writes_alias` | `json` written | AC-5 |
| 10 | Test | `ui/parameter_form.rs` `form_link_parameter_round_trip` | `~X~…~E` stays a link | AC-6 |
| 11 | Test | `ui/parameter_form.rs` `form_unavailable_reasons` | parse, no action, unknown | AC-7 |
| 12 | Test | `ui/parameter_form.rs` `form_excess_is_read_only` | edits refused | AC-8 |
| 13 | Test | `egui/parameter_form.rs` `edit_parameter_form_headless_smoke` | renders every `ArgumentGUIInfo` without panic | AC-5 |
| 14 | Test | `ui/widgets/query_console_element.rs` `parameters_panel_edit_updates_text_without_submit` | no `AppMessage` | AC-10 |
| 15 | Test | `query_console_element.rs` `console_without_show_parameters_deserializes` | serde default | AC-11 |
| 16 | Test | `liquers-lib/tests/webui_ssr.rs` `console_renders_parameter_form` | field and event attributes, escaping | AC-9 |
| 17 | Test | `query_console_element.rs` `params_apply_event_sets_query_and_submits` | `User(Named("params-apply"))` | AC-9 |
| 18 | Browser | `examples-web/tests/ui_query_console_app.spec.ts` "parameter form applies" | real DOM | AC-9 |

## Example 1: egui console

The console text is `-R-key/specs/issues/-/ns-search/field_extract`, the registry comes from
`ctx.command_registry()`, and the user switches Parameters on. The form shows:

| Field | Widget | Value |
|---|---|---|
| extension | `EnumSelector` (declared): md, json, yaml, yml | (default md) |
| prefix | text field | (default "") |
| flatten | `Checkbox` (type-derived) | (default false) |
| fields | list, text elements | empty |

Ticking `flatten` calls `form.set("flatten", Some(new_string("true")))`, and the text field becomes
`…/field_extract-md--true`. "Add" on `fields` followed by typing `status` gives
`…/field_extract-md--true-status`. Nothing is evaluated until Enter. All three texts validate `Ok`
with `liquers-validate`.

```rust
let mut form = ParameterForm::new("-R-key/specs/issues/-/ns-search/field_extract", &registry)?;
form.set("flatten", Some(ActionParameter::new_string("true".into())))?;
assert_eq!(form.query_text(), "-R-key/specs/issues/-/ns-search/field_extract-md--true");
form.insert("fields", 0, ActionParameter::new_string("status".into()))?;
assert_eq!(form.query_text(), "-R-key/specs/issues/-/ns-search/field_extract-md--true-status");
```

For `ns-pl/is_in-name-a-b`, `form.move_element("values", 1, 0)` gives `ns-pl/is_in-name-b-a`
(validated), and `form.remove("values", 1)` gives `ns-pl/is_in-name-a`.

## Example 2: browser

With `ui-events`, the console markup contains
`<input data-lq-field="arg.flatten" list="…bool…" value="false">` and
`<button data-lq-event="params-apply">Apply</button>`. Editing the input to `true` and clicking Apply
sends `UserEvent { kind: Named("params-apply"), fields: [("query", …), ("arg.flatten", "true"), …] }`.
`update` applies the fields, sets `query_text` to `…-md--true` and submits it.

## Edge and Error Cases

| Case | Expected | Test |
|---|---|---|
| Text does not parse (`hello/`) | no form; the reason is the parse error | 11 |
| A resource-only query (`-R/data/x.csv`) | no form: "no action" | 11 |
| An unknown command (`hello/q/ns-lui/query_c`) | no form: "not registered" | 11 |
| A value that does not convert (`ns-pl/head-x`) | the form is shown (presets fallback); the console's analysis marks `x` | 8 |
| Padding needs a default that is missing | the edit is refused inline; the text is unchanged | 5 |
| `excess` parameters | listed read-only; edits refused | 5, 12 |
| An alias | its own arguments, not the head slots | 6 |
| `<b>` in a value | escaped in the web markup | 16 |
| A list element that is a link | moved as a unit | 10 |

## Test Plan

- Core: `cargo test -p liquers-core --lib set_argument_parameter` (tests 3-5). The metadata is built
  inline, as in the variadic tests.
- Model: `cargo test -p liquers-lib --no-default-features --lib parameter_form` (tests 6-12). The
  fixture registry has `field_extract`-shaped and `is_in`-shaped commands built inline, plus one
  command per `ArgumentGUIInfo` variant. Test 8 runs `validate_query` at Plan level on every result.
- egui: `cargo test -p liquers-lib --no-default-features --features egui --lib parameter_form`
  (test 13, `egui::Context::default().run(RawInput::default(), …)` with a `CentralPanel`), and
  `--lib query_console_element` (tests 14 and 15).
- Web: `cargo test -p liquers-lib --no-default-features --features webui --test webui_ssr` (test 16)
  and `--lib query_console_element` (test 17).
- Browser: test 18 after `cargo clean`
  (`cd liquers-lib/examples-web && npx playwright test --project ui_query_console_app`). The example
  must register a command with arguments for the form to show; the plan adds `ns-pl`-free fixture
  commands `greet(state, greeting: String = "Hello", excited: bool = false)` to the web example.
- Matrix: `bash scripts/check-build-matrix.sh`. The model builds with no features and on wasm32.
