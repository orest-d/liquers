# Phase 1: High-Level Design — command-parameter-editor

## Purpose

Commands declare a label, a description, a default and a widget (`gui_info`) for each argument, but
no UI uses them. The user edits a query's arguments only as text, and so has to know the parameter
grammar. A form over the last action lets a user change an argument without knowing that grammar.
This serves the interactive-exploration use case of `specs/reference/VISION.md`, where queries are
the user interface. It is also the editor that `UI-VARIADIC-ARGUMENT-LIST-EDITOR`'s list widget
needs.

## Problem Example

Verified at HEAD (`cd6d5da`). No code reads `ArgumentInfo::gui_info`: `grep gui_info` over
`liquers-lib/src`, `liquers-web/src`, `liquers-axum/src` and `liquers-py/src` finds only
declarations. `QueryConsoleElement` (`liquers-lib/src/ui/widgets/query_console_element.rs`) has a text
field and command presets, and nothing else.

Concrete case. All the queries below were validated with `liquers-validate` and are `Ok`.
`search/field_extract` declares `extension` (an enum, default `md`, `EnumSelector`), `prefix`
(string, default `""`), `flatten` (bool, default `false`, with the default `TextField 40`) and
`fields` (string, `multiple`). The console shows `-R-key/specs/issues/-/ns-search/field_extract`.

- **Today:** to turn `flatten` on, the user has to know that the two earlier arguments must be
  written out: `…/field_extract-md--true`. Appending `-true` sets `extension`. Adding the field
  `status` then gives `…/field_extract-md--true-status`.
- **Should be:** a Parameters panel shows a selector for `extension`, a text field for `prefix`, a
  checkbox for `flatten` and an ordered list for `fields`. Ticking the checkbox writes
  `…-md--true`. Pressing "Add" and typing `status` writes `…-md--true-status`. Moving an element of
  `ns-pl/is_in-name-a-b` up gives `ns-pl/is_in-name-b-a`.

## Scope and Acceptance Criteria

- **AC-1** Fields from metadata
  WHEN the form is built for a query whose last action has registered metadata
  THEN it has one field per argument that is not injected and whose `gui_info` is not `Hide`, in
  declaration order. Each field has its label, description, default and current written values.
- **AC-2** A scalar edit pads earlier arguments
  WHEN `flatten` is set to `true` in `…/field_extract`
  THEN the query text is `…/field_extract-md--true`, and it plans
- **AC-3** A list edit stays inside its argument
  WHEN `status` is added to `fields`, or element 1 of `values` in `is_in-name-a-b` is moved to 0 or
  removed
  THEN the text is `…-md--true-status`, `is_in-name-b-a` or `is_in-name-a`
- **AC-4** Reset to default
  WHEN a field is reset
  THEN its parameter is removed if it is the last one written; otherwise the argument's default is
  written in its place
- **AC-5** The widget follows `gui_info`, and the type when `gui_info` is only the default text field
  WHEN an argument is `bool`, an integer, a float or an enum, and its `gui_info` is `TextField` or
  `None`
  THEN it is edited with a checkbox, an integer field, a float field or an enum selector. Any other
  `gui_info` is used as declared. An enum writes the chosen alternative's alias.
- **AC-6** Link parameters
  WHEN an argument's written parameter is a link `~X~…~E`
  THEN it is edited as a query, and is written back as a link
- **AC-7** No form, with a reason
  WHEN the text does not parse, has no action, names an unregistered command, or there is no
  registry
  THEN no form is shown. A one-line reason is shown instead, and the text is untouched.
- **AC-8** Excess parameters
  WHEN the action has more parameters than its arguments take
  THEN the extra parameters are listed as read-only, and edits are refused with a message
- **AC-9** Browser form
  WHEN the browser console's form is applied
  THEN `query_text` becomes the edited query and is submitted. Add, remove and move rewrite the
  form without submitting.
- **AC-10** egui console panel
  WHEN the Parameters toggle is on in the egui console
  THEN the form for the current text is shown. Each edit updates the text field and does not submit
  until Enter or Go.
- **AC-11** Old state loads
  WHEN an app state saved before this change is loaded
  THEN consoles load, with the panel off

Non-goals: picking an action other than the last one; editing the state argument, recipe argument
overrides or an alias's head parameters; a standalone parameter-editor element or `lui` command;
a date picker (`DateField` is a text field); type validation inside the form, which the console's
live analysis reports (`query-editor-live-validation`).

## Core Interactions

Query (parse, `Query::encode`); Plan (`last_action_command`, slot edits, new
`set_argument_parameter`); UI (`ParameterForm`, egui and web renderers, `QueryConsoleElement`).

## Crate Placement

`liquers-core` (`plan.rs`): the scalar edit, beside the variadic edits, because only the planner's
rules know an argument's position. `liquers-lib` (`ui/`, `egui/`, `ui/web/`): the form and the
renderers.

## Documentation Intent

- Reference: extend `UI_INTERFACE_FSD.md` with "Parameter form", and `RECIPES_AND_PLANS.md`
  "Argument slots" (from the dependency) with `set_argument_parameter`.
- Guide: `COMMAND_REGISTRATION_GUIDE.md`, one line saying that `gui_info` and `label` are now shown.
- Documents to update: `NO-COMMAND-PARAMETER-EDITOR` (closed), `UI-VARIADIC-ARGUMENT-LIST-EDITOR`
  (its step 3 is delivered here).

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. It is size `L`, adds `pub` types and functions in
  `liquers-core` and `liquers-lib` (rules 1, 3 and 4), and spans two crates (rule 6).
- **Leading issue:** **Proposed resolution - host:** a toggled Parameters panel inside
  `QueryConsoleElement`, with no new element type or `lui` command. Alternative: a standalone
  `ParameterEditorElement` bound to another element's query. This needs a new command, a binding
  mechanism and a registry change. The form model is reusable for that later.
- **Explanation:** every input exists or is designed: positions (`argument_slots`), list edits,
  command resolution (`Plan::last_action_command`), a registry in `UIContext`, and browser user
  events. Phases 3 and 4 are written under the recommendations below.
- **Open questions:**
  1. **Proposed resolution - host:** as the leading issue.
  2. **Proposed resolution - type-derived widget (AC-5):** the default `TextField(40)` cannot be told
     apart from a declared one, and a text field is never right for a `bool`. So the argument type
     picks the widget whenever `gui_info` is `TextField` or `None`.
  3. **Proposed resolution - scalar edit in core:** add
     `set_argument_parameter(action, metadata, argument, Option<ActionParameter>)`, using the same
     padding and default-text rules as `insert_variadic_element` (AC-2, AC-4).
  4. **Proposed resolution - write-back:** an edit re-encodes the whole query (`Query::encode`), so
     its spelling may normalise. Text that has not been edited is never rewritten.

## Design Dependencies

- `variadic-argument-slots` (in review): **requires**. It provides `argument_slots`, the variadic
  edits and the padding rule. This design's Step 1 extends the same code.
- `query-editor-live-validation` (in review): **requires** its Step 3 (the registry in `UIContext`
  and `AppRunner`). It also **overlaps** on editing link arguments: `edit_query_live` replaces
  `edit_query` once it exists.
- `ui-events` (in review): **requires**, for the browser half only (Steps 6 and 7): `UserEvent`,
  `field_attr`, `event_attr`, and field collection.
- `web-query-editor-live-preview`: **overlaps**. Both edit the console's `render_web`. No order.
- `query-builder` (in review): **overlaps**. It builds queries from scratch. No shared symbol.
- `argument-gui-info-default` (complete): context; it fixed the `TextField(40)` default (question 2).

## Consolidated Findings

- The order is: `variadic-argument-slots`, then Step 3 of `query-editor-live-validation`, then this
  design's egui half; the browser half waits for `ui-events`. The egui half is useful on its own.
- Command resolution must match the planner. `Plan::last_action_command` does this, and handles
  aliases (their own arguments, not their heads). When the action's parameters do not plan, the
  form falls back to the preset lookup in `liquers-lib/src/utils.rs`, so that a form is still shown
  for an invalid value.
- 410 of 423 exported `gui_info` entries are the default text field, so the AC-5 rule decides
  what users see. `DateField` stays a text field (no picker dependency).
- No serialized format changes beyond one `#[serde(default)]` field (AC-11). The registry is
  unchanged and `liquers-py` is unaffected.
