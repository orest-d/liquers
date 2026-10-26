---
id: UI-DISPLAY-CONVERSION
kind: design
title: A value view converts a value it cannot show to a table or image type by an automatic edge
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [lib/ui, lib/value]
issues: [UI-WIDGET-DOES-NOT-CONVERT-ITS-VALUE]
created: 2026-10-26
---
# A value view converts a value it cannot show to a table or image type by an automatic edge

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. Split off from `automatic-value-conversion` (its question 2); it
requires `value-conversion-registry`, which is designed but not implemented.

## Phase 1: High-Level Design

### Purpose

A value view (the element the UI mounts for any query result that is not itself a `UIElement`)
shows a table only when the value already is a `polars.DataFrame` or a `RecordView`. A command
that returns the same data as a list of dictionaries — the natural result of a Python command or
a JSON source — is shown as the bare label `Array`. Letting the view ask the conversion registry
for a type it can show gives such results a table without each command converting for display
(VISION priority 2, records and DataFrames; "Trust the result": the conversion is display-only
and labelled).

### Problem Example

```
fetch_people
```

where `fetch_people` returns `[{"name": "Ann", "age": 31}, {"name": "Bo", "age": 27}]` (an
`Array`). Verified at HEAD (`cd6d5da`): the browser backend (`liquers-lib/src/ui/web/html.rs`
`simple_to_html`) renders `<span class="lq-value lq-array">Array</span>`, and the egui backend
(`liquers-lib/src/egui/mod.rs` `UIValueExtension::show`) an italic `Array` label — not even the
JSON text. Should be: the view finds `Array → RecordView` (`Structural`, automatic, from
`value-conversion-registry`), converts a copy for display, shows a two-row table with the caption
`Array shown as RecordView`, and leaves the asset's value an `Array`.

### Scope and Acceptance Criteria

- **AC-1** A list of objects is shown as a table
  WHEN an `AssetViewElement` for `fetch_people` (feature `records`) is rendered by the web backend
  THEN the HTML holds a `<table>` with `Ann` and `Bo`, preceded by the caption `Array shown as RecordView`
- **AC-2** The asset is not changed
  WHEN the view converted the value for display
  THEN the asset's value is still the `Array`, its type identifier is `Array`, and its log has no conversion entry
- **AC-3** A natively shown value is not converted
  WHEN the value is a `RecordView`, `polars.DataFrame`, `Image`, `UIElement`, `Text` or a scalar
  THEN the view has no display conversion and renders exactly as before
- **AC-4** A refused or failing edge falls back
  WHEN the value is `Text` holding CSV (`Text → RecordView` is `Fallible`), or an `Array` of numbers (the `Structural` edge fails on it)
  THEN the view renders as before, with no caption and no error shown
- **AC-5** The query console converts too
  WHEN a `QueryConsoleElement` receives a snapshot whose value is the `fetch_people` array
  THEN its data view renders the same table and caption
- **AC-6** A recomputed value is re-converted
  WHEN a displayed asset is recomputed to a new array
  THEN the next snapshot's display value is the new table, and a recomputation to `Text` drops it

Non-goals: converting inside `StateViewElement` (built without an environment; no in-tree caller
builds one from an asset); a purpose vocabulary (question 1); widgets that *declare* a wanted
type (no widget does today); transitive conversion; lossy or fallible edges for display.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a `pub` field to `AssetSnapshot` and a new `pub`
  function and struct in `liquers-lib` (rules 3, 4), and depends on an unimplemented design (rule 5).
- **Leading issue:** **Open design - purposes** (question 1).
- **Explanation:** the registry interface (`can_convert`, `convert`, `ConversionPolicy::Automatic`,
  `ConversionClass::is_automatic`) is fixed by `value-conversion-registry` Phase 2, and the view's
  value reaches it only where the runner holds an `EnvRef`, so Phases 3-4 are written against
  that interface under the recommended answers.
- **Open questions:**
  1. **Open design - purposes:** no purpose concept. The UI keeps an ordered list of the type
     identifiers its backends render richly — `RecordView`, `polars.DataFrame`, `Image`, each
     present only under its feature — and asks the registry for an automatic edge to the first
     reachable one. Recommended: neither this consumer nor HTTP negotiation
     (`http-accept-negotiation`) needs a registered `table` identifier, so
     `type-conversion-draft.md` open question 1 stays open until a consumer must name "a table"
     in a declaration. Alternative: register `table` as an identifier nothing is stored as; it
     would need edges *to* it from every table type and a renderer *for* it, for no gain here.
  2. **Proposed resolution - the conversion is labelled:** a caption `<from> shown as <to>`
     above the rendering (web: `<div class="lq-display-note">`; egui: a small italic label), so a
     user never mistakes the display type for the asset's type.
  3. **Proposed resolution - where it runs:** where a snapshot or view value is built, in async
     runner code that holds the environment (`AppRunner::build_snapshot`, the view's value
     update), never in `render_web` / `show_in_egui`, which have no environment and run per frame.
  4. **Proposed resolution - failure is silent:** a failed or missing edge leaves today's
     rendering and records nothing on the asset (the asset is finished; display must not write
     to it).

### Design Dependencies

- `value-conversion-registry` — **requires**: `Environment::get_conversion_registry`,
  `ConversionRegistry::{can_convert, convert}`, `ConversionInput`, `ConversionPolicy::Automatic`,
  and the `Array/Object → RecordView` (`Structural`) edge.
- `ui-asset-snapshot-boundary` — **overlaps** (order): it removes
  `AssetViewElement::from_asset_ref` and feeds the view through `AssetSnapshot`. Both edit
  `AppRunner::poll_evaluating_nodes`; T3 was judged not to hold because the only shared edit is
  one call where a value is mounted, and merging would make that design wait on the registry.
  Phase 4 step 3 has a variant for either order.
- `automatic-value-conversion` — **overlaps**: the sibling site (command state and links); same
  registry and policy, no shared code.
- `http-accept-negotiation` — **overlaps**: the third consumer; question 1 is answered the same
  way there.
- `console-mounts-nested-element` — **overlaps**, weak: edits the console's data view for nested
  elements; a `UIElement` is never converted, so the two branches do not meet.

### Consolidated Findings

- Both backends render `Array` and `Object` as a bare label today; the issue's "JSON text" was
  inaccurate and is corrected in its body.
- With the registry's proposed edges, only `Array` and `Object` (and any `Foreign` value an
  integration gives an automatic edge, e.g. a pandas frame) gain a display. `Text`/`Bytes →
  RecordView` and `Bytes → Image` are `Fallible`, so they stay as they are (AC-4).
- Render functions have no environment; the conversion therefore travels as a second value next
  to the real one (`AssetSnapshot::display_value`, a view field), computed once per value change.

## Phase 2: Architecture

### Solution

A new module `liquers-lib/src/ui/display.rs` decides and performs the display conversion. The
runner computes it wherever it hands a value to a view, and stores it beside the value. Each view
renders the display value when present, with the caption, and the real value otherwise.

Rejected: converting inside `value_to_html` / `UIValueExtension::show` (no environment; runs
every frame); a process-global registry (environments extend it); replacing the snapshot's
`value` (widgets would see a type the asset does not have, against AC-2).

Known-issue preflight: no open issue names `build_snapshot`, `AssetSnapshot` or `value_to_html`
besides `UI-ELEMENT-NESTED-IN-CONSOLE-HAS-NO-HANDLE` (`console-mounts-nested-element`, weak) and
`EGUI-ASSET-MANAGER-INTEGRATION` (`ui-asset-snapshot-boundary`, order only).

### Changes

```rust
// liquers-lib/src/ui/display.rs (new; `pub mod display;` in ui/mod.rs)
/// A value converted only to be shown: the asset keeps its own value.
#[derive(Clone, Debug)]
pub struct DisplayValue { pub value: Arc<Value>, pub from: String, pub to: String }
impl DisplayValue { pub fn caption(&self) -> String; } // "Array shown as RecordView"

/// The identifiers the backends render richly, in preference order (feature-gated pushes).
fn display_targets() -> Vec<&'static str>;              // ["RecordView", "polars.DataFrame", "Image"]

/// `None` when `value` is already a display target or a `UIElement`, when no automatic edge
/// reaches a target, or when every reachable edge fails on this value. Synchronous.
pub fn display_value<E: Environment<Value = Value>>(envref: &EnvRef<E>, value: &Value) -> Option<DisplayValue>;
```

Body: `from = value.identifier()`; for each target `to`: `registry.can_convert(&from, to)` is
`Some(class)` with `class.is_automatic()` → `registry.convert(&ConversionInput { value, data_format:
None }, to, ConversionPolicy::Automatic)`; `Ok((v, _))` → `Some(DisplayValue { value: Arc::new(v), … })`;
`Err(_)` → next target. Exhaustive matches only over `ConversionClass` through `is_automatic`.

- `ui/message.rs` `AssetSnapshot`: `pub display_value: Option<DisplayValue>` (new field; every
  literal construction, six in `query_console_element.rs` tests and one in `runner.rs`, adds it).
- `ui/runner.rs`: `build_snapshot(envref, asset_ref)` (takes the runner's `envref`) fills
  `display_value` from the snapshot value; the error snapshot sets `None`.
- `ui/element.rs` `AssetViewElement`: a shared `display: Arc<RwLock<Option<DisplayValue>>>`
  (`#[serde(skip)]`) set wherever `value` is set — at HEAD in `from_asset_ref` (initial value and
  the background loop, which gets the env from `asset_ref.try_get_envref()`), after
  `ui-asset-snapshot-boundary` in `update(AssetUpdate)` from `snapshot.display_value`; cleared by
  `set_value` / `set_error`. `render_web` and `show_in_egui` in `Value` mode render
  `caption + display.value` when present.
- `ui/widgets/query_console_element.rs`: keeps `display_value` from the snapshot beside `value`;
  `show_content` and `render_web` prefer it, with the caption.
- `ui/web/html.rs`: `pub fn display_note_html(caption: &str) -> String` (escaped).

Errors: none surfaced (question 4). Sync: `display_value` is sync, called from async runner code.
Commands: none; `specs/command_registry.yaml` unchanged. Documents:
`specs/reference/UI_INTERFACE_FSD.md` (a "Display conversion" paragraph under value views, History
row, `reviewed:`).

### Risks

| Assessment | Finding |
|---|---|
| Existing tests | `AssetSnapshot { … }` literals gain a field; no assertion changes |
| Compatibility | Additive field on a `pub` struct (a breaking literal for an external constructor; none known). Display only: no stored or served format changes |
| Performance | One conversion per value change, on the runner task; a large array converts once, not per frame. `can_convert` is a map lookup for every other value |
| Feature matrix | Targets and tests gated by `records` / `polars` / `image-support`; web rendering by `webui`; `scripts/check-build-matrix.sh` |
| Recovery | Revert; the field defaults to `None` and rendering falls back |
| Certainty | Medium-high: depends on the registry's interface as written; re-check when it lands |

## Phase 3: Examples and Tests

### Examples

The Problem Example is AC-1. Setup: `DefaultEnvironment<Value, SimpleUIPayload>` built with the
default conversion registry (features `records`, `webui`), and

```rust
fn fetch_people() -> Result<Value, Error> {
    Value::try_from_json_value(&serde_json::json!([{"name": "Ann", "age": 31}, {"name": "Bo", "age": 27}]))
}
register_command!(cr, fn fetch_people() -> result)?;
```

`fetch_people` validates (`liquers-validate --command fetch_people -- 'fetch_people'`: `Ok`, one
`Action` step). Evaluated through an `AppRunner` pending node, the mounted view's
`render_web` contains `lq-display-note` with `Array shown as RecordView` and a `<table>` whose
cells include `Ann`; `asset.get().await?.data` is still an `Array`.

### Tests

Unit, `liquers-lib/src/ui/display.rs` `mod tests` (`#[cfg(feature = "records")]` where an edge is needed):
- `array_of_objects_displays_as_record_view` — `display_value` returns `to == "RecordView"`, two rows. AC-1.
- `display_targets_and_elements_are_not_converted` — `RecordView`, `Image` (feature `image-support`), `Text`, `I64`, a `UIElement`: `None`. AC-3.
- `fallible_and_failing_edges_fall_back` — `Text("a,b\n1,2")`: `None`; `Array [1, 2, 3]`: `None`. AC-4.

Integration, `liquers-lib/tests/ui_display_conversion.rs` (`#![cfg(all(feature = "records", feature = "webui"))]`):
- `view_renders_converted_table_with_caption` — runner mounts `fetch_people`; asserts the HTML. AC-1.
- `asset_keeps_its_value_and_log` — the asset's type identifier is `Array`; no log entry mentions `RecordView`. AC-2.
- `console_snapshot_carries_display_value` — `QueryConsoleElement` submits `fetch_people`; its `render_web` holds the table and caption. AC-5.
- `recomputed_value_is_reconverted` — a command whose output alternates (array, then `Text`) via a test counter, expired between runs; the second snapshot's `display_value` is `None` and the first's is `Some`. AC-6.

Run: `cargo test -p liquers-lib --features webui --lib --tests ui::display ui_display_conversion`,
then `cargo test -p liquers-lib --lib --tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Re-check `value-conversion-registry`'s landed signatures (`Environment::get_conversion_registry`, `ConversionRegistry::{can_convert, convert}`, `ConversionInput`, `ConversionPolicy`, `ConversionClass::is_automatic`); adjust Phase 2's sketch and record any difference here — `cargo check -p liquers-lib`. Depends on that design.
- [ ] 2. `liquers-lib/src/ui/display.rs` — `DisplayValue`, `display_targets`, `display_value`, unit tests; `pub mod display` in `ui/mod.rs` — `cargo test -p liquers-lib --lib ui::display`. Rollback: delete the module.
- [ ] 3. `ui/message.rs` `AssetSnapshot::display_value`; `ui/runner.rs` `build_snapshot` (pass `envref`) and the error snapshot; the test literals in `query_console_element.rs`. Then the view: if `ui-asset-snapshot-boundary` has landed, `AssetViewElement::update` and the runner's mount copy `display_value`; otherwise `from_asset_ref` computes it for the initial value and in its loop — `cargo test -p liquers-lib --lib --tests`. Rollback: revert the step.
- [ ] 4. Rendering: `html.rs` `display_note_html`; `AssetViewElement::render_web` / `show_in_egui`; `QueryConsoleElement::render_web` / `show_content` — depends on 3 — `cargo test -p liquers-lib --features webui --lib --tests`.
- [ ] 5. Integration tests `liquers-lib/tests/ui_display_conversion.rs` — pass.
- [ ] 6. Docs: `specs/reference/UI_INTERFACE_FSD.md` (History row, `reviewed:`); close `UI-WIDGET-DOES-NOT-CONVERT-ITS-VALUE` with a resolution note — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-lib --lib --tests`; the same with `--features webui`; `bash
scripts/check-build-matrix.sh` (a minimal build has no display targets but must compile; wasm32
rows for `webui`); `python3 scripts/docs_index.py --check`; diff review: no change to
`specs/command_registry.yaml`, nothing outside `liquers-lib/src/ui/`, `liquers-lib/tests/` and the
reference document.
