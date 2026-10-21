# Phase 2: Solution & Architecture — template-editor-ui

## Overview

`TextEditorElement` gains a mode, a cached analysis and an optional preview. In template mode its
egui layouter uses `analyse_template` (tokens from `liquers_template::highlight`, diagnostics from
`validate_template` with `ctx.command_registry()`), laid out with `template_layout_job`. A preview
pane shows the snapshot of the preview query, requested with `RequestAssetUpdates` on the editor's
handle; after a save the dependency manager invalidates that asset and the runner pushes the new
snapshot. A new command `template/editor` builds the element in template mode.

Rejected: a separate element type (duplicates saving and serialization; leading question);
previewing the buffer via `ApplyToInput` (its result never reaches the element; question 3);
re-validating on every frame (cached by text, as `QueryEditorModel`).

## Known-Issue Preflight

| Item | Relation | Blocks? |
|---|---|---|
| `EGUI-ASSET-MANAGER-INTEGRATION` (`ui-asset-snapshot-boundary`) | creates `TextEditorElement` | requires |
| `TEXT-EDITOR-EDITS-ARE-NEVER-SAVED` (`text-editor-save`) | save path | requires |
| `TEMPLATE-HAS-NO-HIGHLIGHTER` (`template-syntax-highlighter`) | tokens | requires |
| `UI-QUERY-EDITOR-LACKS-LIVE-VALIDATION` (`query-editor-live-validation`) | registry in context, diagnostics type | requires |
| `UI-ELEMENT-NESTED-IN-CONSOLE-HAS-NO-HANDLE` | preview needs a handle | no (degrades) |

Namespaces: `template` (one command).

## Interfaces

`liquers-lib/src/ui/widgets/text_editor_element.rs` (additions):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EditorSyntax { #[default] Plain, Template }

// new fields on TextEditorElement
pub syntax: EditorSyntax,                 // serialized, #[serde(default)]
pub preview_input: String,                // serialized, #[serde(default)]; "" = evaluate mode
#[serde(skip)] preview: Option<AssetSnapshot>,
#[serde(skip)] preview_requested: Option<String>,   // last query sent
#[cfg(feature = "templates")] #[serde(skip)] template_model: crate::template::editor::TemplateEditorModel,

impl TextEditorElement {
    pub fn with_syntax(self, syntax: EditorSyntax) -> Self;
    /// `""` -> `<key>/-/ns-template/expand`; else `<input>/ns-template/render-~X~<key>~E`
    /// (`Query`s built with `Query::encode`, the key as `-R/<key>`).
    pub fn preview_query(&self) -> Option<String>;   // None in Plain mode
}
```

`init` and a change of `preview_input` send `RequestAssetUpdates { handle, query: preview_query }`
when the handle exists; `update(AssetUpdate)` stores the snapshot as `preview`.

`liquers-lib/src/template/editor.rs` (feature `templates`, new):

```rust
#[derive(Debug, Clone, Default)]
pub struct TemplateAnalysis { pub tokens: Vec<TemplateSyntaxToken>, pub diagnostics: Vec<EditorDiagnostic> }
pub fn analyse_template(text: &str, registry: Option<&CommandMetadataRegistry>) -> TemplateAnalysis;
#[derive(Debug, Clone, Default)]
pub struct TemplateEditorModel { /* cached text + analysis, as QueryEditorModel */ }
impl TemplateEditorModel {
    pub fn analysis(&mut self, text: &str, registry: Option<&CommandMetadataRegistry>) -> &TemplateAnalysis;
}
```

`Diagnostic` (`liquers-template`) maps to `EditorDiagnostic` (`query-editor-live-validation`):
range = the token containing `position.offset`; `Severity` to `DiagnosticSeverity`.

Command (`liquers-lib/src/template/commands.rs`):

```rust
pub async fn editor<E: Environment<Value = Value>>(state: &State<Value>, context: Context<E>) -> Result<Value, Error>;
register_command!($cr, async fn editor(state, context) -> result
    namespace: "template" label: "Template editor"
    doc: "Edit the template stored under the key in the state, with live diagnostics and a preview")?;
```

Body: as `text_editor` after `ui-asset-snapshot-boundary` (key from state, text via
`context.get_dependency_state`), then `TextEditorElement::new(key, text).with_syntax(EditorSyntax::Template)`.

egui: template mode lays out `template_layout_job(buf, &a.tokens, diagnostics)`; a horizontal
preview-input row uses `edit_query_live` (AC-8); the preview pane renders `preview.value` with
`UIValueExtension::show`, or `display_error` for `preview.error`. Web: `render_web` renders
`render_tokens(text, …, &HtmlTemplateStyle)` read-only plus the preview value via `value_to_html`.

## Integration Points

- `ui/widgets/text_editor_element.rs` (fields, init, update, egui, web).
- `template/{editor.rs, commands.rs, mod.rs}`; `register_template_commands!`.
- `specs/command_registry.yaml` regenerated.
- Features: element compiles without `templates` (mode `Template` then lays out as plain text and
  `preview_query` still works, since it is only text); `template_model` field gated.

## Error Handling

Analysis never fails. The command's errors are `text_editor`'s (missing key, non-text value). A
preview error arrives as `AssetSnapshot.error` and is displayed (AC-7). No new `Error` values.

## Relevant Commands

New `template/editor`. Uses `template/expand`, `template/render`. `text_editor` unchanged.

## Documentation Architecture

`TEMPLATE_LANGUAGE.md` §4, `UI_INTERFACE_FSD.md` "Editors", `TEMPLATE_GUIDE.md` §1 — each with a
History row and `reviewed:`. `affects_docs: [TEMPLATE_LANGUAGE, UI_INTERFACE_FSD, TEMPLATE_GUIDE]`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | as Integration Points |
| Crates and workflows affected | `liquers-lib` only (`ui`, `template`, `egui`); four prerequisite designs |
| Existing tests likely to change | `TextEditorElement` serde test (new `#[serde(default)]` fields); `registry_export` |
| New validation | analysis unit tests; runner integration for preview; SSR |
| Compatibility | serialized editors without the new fields read with defaults |
| Concurrency | preview monitoring is the runner's existing per-handle map |
| Performance | template validation per text change, cached; linear in size |
| Security | `render_web` escapes through `HtmlTemplateStyle` |
| Features | `cfg(feature = "templates")` on the model field and analysis; `scripts/check-build-matrix.sh` |
| Recovery | revert; serialized states keep loading (defaults) |
| Certainty | medium: depends on four designs landing as specified |
