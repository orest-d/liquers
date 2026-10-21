# Phase 2: Solution & Architecture — query-editor-live-validation

## Overview

Core gains a source-faithful tokenizer: `query_syntax_tokens(text, &Query)` turns the parsed
query's positions into byte ranges that tile the text. `liquers-lib/src/ui/query_editor.rs` holds a
backend-free `QueryEditorModel` that, when its text changes, parses, tokenizes and runs
`validate_query` at `Plan` level (with a registry) or `Parse` level (without), and keeps the result
until the text changes again. `UIContext` carries an optional registry; `AppRunner` fills it.
`QueryConsoleElement` owns a model; the egui adapter lays out the **buffer egui passes** from the
model's tokens; an HTML adapter emits classed spans for `render_web` and `query_to_html`.

Rejected alternatives:
- *Reuse `StyledQueryToken`.* Its text is re-encoded (`ns-pl/-/head` → `-R/ns-pl/-/head`), so it
  cannot be laid out over an edit buffer; changing it would alter every finished-query renderer.
- *Re-lex the text with a second grammar.* Two grammars drift; positions from the one parser do not.
- *Validate in the runner by message.* Async round trip per keystroke for a synchronous, cheap check.
- *Store the registry in the element.* Elements are typetag-serialized and non-generic.

## Known-Issue Preflight

| Issue | Relation | Blocks? |
|---|---|---|
| `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` | `styled_query_to_layout_job` uses `Color32::from_hex(..).unwrap()`; the new layout function uses `Color32::from_rgb` constants | no |
| `ui-events` (design) | per-keystroke browser field events are its non-goal | no (scopes AC-7) |

Namespaces involved: none (no command added or changed).

## Interfaces

`liquers-core/src/query.rs` (all targets, sync, pure):

```rust
/// Kind of a source span in query text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SyntaxKind { Separator, Header, ResourceName, ActionName, Parameter, Entity, Filename, Plain }

/// A byte range `start..end` of the source text and its kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxToken { pub kind: SyntaxKind, pub start: usize, pub end: usize }

/// Tokens tiling `text` exactly (AC-3). `query` must be `parse_query(text)`'s result: its
/// positions are the token starts.
pub fn query_syntax_tokens(text: &str, query: &Query) -> Vec<SyntaxToken>;
/// `parse_query` then `query_syntax_tokens`; on a parse error, one `Plain` token.
pub fn syntax_tokens(text: &str) -> Vec<SyntaxToken>;

/// The start of a token found by a parser; shared with `liquers-template`, whose query
/// templates carry positions in template coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxMark { pub offset: usize, pub kind: SyntaxKind }

/// Tiles `text[start..end]` from sorted `marks`, applying the query separator rules.
pub fn tile_syntax_marks(text: &str, start: usize, end: usize, marks: &[SyntaxMark]) -> Vec<SyntaxToken>;
```

Algorithm (implementation detail, fixed by tests): `query_syntax_tokens` collects marks from
segment headers, resource names, action names, parameters and link `~X~` entities (recursing into
links) and calls `tile_syntax_marks`: each mark extends to the next; trailing `/`, `-`, `/-/`
characters split off as `Separator`, a link's closing `~E` as `Entity`; text before the first mark
is `Separator`.

`liquers-lib/src/ui/query_editor.rs` (new, no feature gate):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity { Error, Warning }

#[derive(Debug, Clone)]
pub struct EditorDiagnostic {
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub start: usize,           // byte range in the text; empty text -> 0..0
    pub end: usize,
}

#[derive(Debug, Clone, Default)]
pub struct QueryAnalysis { pub tokens: Vec<SyntaxToken>, pub diagnostics: Vec<EditorDiagnostic> }

/// Pure: parse, tokenize, and validate at Plan level when `registry` is given.
pub fn analyse_query(text: &str, registry: Option<&CommandMetadataRegistry>) -> QueryAnalysis;

/// Caches the analysis of one text; recomputes only when the text or registry presence changes.
#[derive(Debug, Clone, Default)]
pub struct QueryEditorModel { /* text: String, with_registry: bool, analysis: QueryAnalysis */ }
impl QueryEditorModel {
    pub fn analysis(&mut self, text: &str, registry: Option<&CommandMetadataRegistry>) -> &QueryAnalysis;
    /// Add an execution error whose position is in `text` (AC-6); dropped on the next text change.
    pub fn set_execution_error(&mut self, text: &str, error: &Error);
}
```

Diagnostic range rule: a plan error covers the token whose `start` equals the error offset (or
the token containing it); a parse error covers offset..text end; an unknown position covers the
whole text. Warnings come from `ValidationResult::warnings`, positioned at the whole text.

`UIContext` (`ui/ui_context.rs`), a new private field `command_registry: Option<Arc<CommandMetadataRegistry>>`:

```rust
pub fn with_command_registry(self, registry: Arc<CommandMetadataRegistry>) -> Self;
pub fn command_registry(&self) -> Option<&CommandMetadataRegistry>;
```

`AppRunner` (`ui/runner.rs`): field `registry: Arc<CommandMetadataRegistry>` set in `new` from
`envref.get_command_metadata_registry().clone()`; every `UIContext::new(..)` in the runner adds
`.with_command_registry(self.registry.clone())`; `pub fn command_registry(&self) -> Arc<CommandMetadataRegistry>`.

Adapters:
- `egui/widgets.rs`: `pub fn analysis_to_layout_job(text: &str, analysis: &QueryAnalysis) -> LayoutJob`
  (colours per `SyntaxKind`, underline + red over diagnostic ranges, plain grey fallback) and
  `pub fn edit_query_live(ui, text: &mut String, model: &mut QueryEditorModel, registry: Option<&CommandMetadataRegistry>) -> egui::Response`,
  whose layouter calls `model.analysis(buf.as_str(), registry)` on **the buffer passed in** and
  shows the first diagnostic as hover text. `edit_query` keeps its signature and becomes
  `edit_query_live` with a throw-away model and no registry.
- `ui/web/html.rs` (`webui`): `pub fn analysis_to_html(text: &str, analysis: &QueryAnalysis) -> String`
  — one `<span class="lq-tok-{kind}">` per token, escaped; tokens inside a diagnostic range get
  `lq-tok-error` and the message as `title`. `ui/web/widgets.rs` `query_to_html` uses
  `analyse_query(q, None)`.

`QueryConsoleElement`: new `#[serde(skip)] editor: QueryEditorModel`; `show_toolbar` uses
`edit_query_live`-style layout with `ctx.command_registry()`; on `AssetUpdate` with an error it
calls `editor.set_execution_error(&self.query_text, e)`; `error_position` is removed (the model
owns it). `render_web` renders a highlighted preview `<div class="lq-qc-preview">` with
`analysis_to_html` under the input (the input stays a plain `<input>`).

## Integration Points

- `liquers-core/src/query.rs` (types, functions), re-exported nowhere else.
- `liquers-lib/src/ui/{query_editor.rs, mod.rs, ui_context.rs, runner.rs}`,
  `ui/widgets/query_console_element.rs`, `egui/widgets.rs`, `ui/web/{html.rs, widgets.rs, app.rs}`
  (`app.rs`: build the context after the runner and attach its registry).
- Examples constructing a render `UIContext` (`liquers-lib/examples/ui_*.rs`,
  `examples-web/*/src/lib.rs`) add `.with_command_registry(app_runner.command_registry())`;
  without it they keep parse-level behaviour, so this is not breaking.

## Error Handling

Analysis never fails: invalid text is data (`validate_query` contract). No new `Error` values.

## Relevant Commands

None new or changed; `specs/command_registry.yaml` is unchanged.

## Documentation Architecture

- `specs/reference/UI_INTERFACE_FSD.md`: new "Query editing" section; History row; `reviewed:`.
- `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md`: `SyntaxToken` / `query_syntax_tokens`
  beside the styled renderer; History row; `reviewed:`.
- `affects_docs: [UI_INTERFACE_FSD, DOC_02_QUERY_LANGUAGE_REFERENCE]`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | the files under Integration Points |
| Crates and workflows affected | `liquers-core` (additive), `liquers-lib` (`ui`, `egui`, `webui`) |
| Existing tests likely to change | `query_console_element.rs` `test_asset_update_exposes_error_position_for_egui_layout` (moves to the model); `egui/widgets.rs` `query_layout_underlines_the_positioned_token_only` keeps passing |
| New validation | tokenizer tiling over the parse corpus; model unit tests; SSR test; Playwright case |
| Compatibility | additive; `query.rs` is performance-sensitive but the new function runs only in editors |
| Performance | plan per text change: microseconds for typical queries; cached per text (AC-8) |
| Concurrency | none; pure functions on the render thread |
| Security | HTML output escapes every token (`escape_html`) |
| Recovery | revert; nothing persisted (`editor` is `serde(skip)`) |
| Certainty | high for egui and SSR; browser per-keystroke deferred |
