# Phase 2: Solution & Architecture — template-syntax-highlighter

## Overview

`liquers_template::highlight::highlight(text, &Template) -> Vec<TemplateSyntaxToken>` walks the
AST. Each element's `position.offset` starts its range; the tag's delimiters are found from the
active open/close strings (updated by every `SetDelimiterTag`); query pieces of a hygienic body
become `SyntaxMark`s tiled by core's `tile_syntax_marks`, with each `Slot` overlaid as one token.
`render_tokens(text, &tokens, &diagnostics, &style)` produces HTML or ANSI. `liquers-lib` registers
`template/highlight` and an egui layout helper.

Rejected alternatives:
- *Text-carrying tokens with `StyledQueryToken`* (the issue's sketch): re-encoded text; not
  editor-safe (leading question).
- *A new `StyledQueryToken::Slot` variant:* breaks exhaustive matches outside core (noted in the
  issue).
- *Re-lexing the template text:* duplicates the parser's delimiter and standalone rules.
- *Shadow text with slots replaced, re-parsed by core:* unnecessary, positions already exist.

## Known-Issue Preflight

| Issue | Relation | Blocks? |
|---|---|---|
| `UI-QUERY-EDITOR-LACKS-LIVE-VALIDATION` (design `query-editor-live-validation`) | provides core tokens | requires Step 1 |
| `NO-TEMPLATE-EDITOR-UI` | consumer | no |

Namespaces: `template` (one command added).

## Interfaces

`liquers-template/src/highlight.rs` (new; sync, pure; all targets):

```rust
use liquers_core::query::{SyntaxKind, SyntaxToken};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TemplateSyntaxKind {
    Text, Delimiter, Sigil, Name, Comment, Slot, Invalid,
    Query(SyntaxKind),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateSyntaxToken { pub kind: TemplateSyntaxKind, pub start: usize, pub end: usize }

/// Tokens tiling `text`; `template` must be `parse_template(text, …)`'s result.
pub fn highlight(text: &str, template: &Template) -> Vec<TemplateSyntaxToken>;
/// Parse, then `highlight`; an irrecoverable parse error gives one `Text` token (AC-6).
pub fn highlight_text(text: &str, syntax: TemplateSyntax) -> Vec<TemplateSyntaxToken>;

pub trait TemplateRenderStyle {
    fn begin(&self, kind: TemplateSyntaxKind, diagnostic: Option<&Diagnostic>) -> String;
    fn end(&self, kind: TemplateSyntaxKind, diagnostic: Option<&Diagnostic>) -> String;
    fn text(&self, source: &str) -> String;           // HTML escapes; ANSI passes through
}
pub struct HtmlTemplateStyle;                         // <span class="lq-tok-…" title="…">
pub struct AnsiTemplateStyle;                         // colours of DarkAnsiQueryRenderStyle
pub fn render_tokens<S: TemplateRenderStyle>(text: &str, tokens: &[TemplateSyntaxToken],
    diagnostics: &[Diagnostic], style: &S) -> String;
```

A token is "in" a diagnostic when it contains the diagnostic's byte offset; the first such token
gets the error class/colour (`Diagnostic::position.offset` is template bytes).

Re-exports in `liquers-template/src/lib.rs`: `highlight`, `highlight_text`, `render_tokens`,
`TemplateSyntaxKind`, `TemplateSyntaxToken`, `TemplateRenderStyle`, `HtmlTemplateStyle`,
`AnsiTemplateStyle`.

Command (`liquers-lib/src/template/commands.rs`, inside `register_template_commands!`):

```rust
pub async fn highlight<E: Environment<Value = Value>>(state: State<Value>, style: String,
    context: Context<E>) -> Result<Value, Error>;
register_command!($cr,
    async fn highlight(state, style: String = "html", context) -> result
    namespace: "template"
    label: "Highlight template"
    doc: "The state as a template, syntax-highlighted as `html` or `ansi`, diagnostics marked"
)?;
```

Async only because the registry comes from `context.get_envref()` as in `validate`; nothing awaits
evaluation. egui (`liquers-lib/src/template/egui.rs`, `cfg(all(feature = "templates", feature = "egui"))`):
`pub fn template_layout_job(text: &str, tokens: &[TemplateSyntaxToken], diagnostics: &[Diagnostic]) -> egui::text::LayoutJob`,
sharing the query colours with `egui/widgets.rs` through a `pub(crate) fn syntax_kind_color(SyntaxKind) -> Color32`
(added there by `query-editor-live-validation` Step 4, or here if this lands first).

## Integration Points

- `liquers-template/src/{highlight.rs, lib.rs}`; walk matches `Element`, `SectionHead`,
  `PartialTarget`, `QueryBody`, `ClassicPiece`, `ActionParameterTemplate`, `Piece`,
  `QuerySegmentTemplate` exhaustively.
- `liquers-lib/src/template/{commands.rs, egui.rs, mod.rs}`.
- `specs/command_registry.yaml` (new command; CHANGELOG line).

## Error Handling

`highlight` never fails on template content (AC-6). The command fails only for a non-text state
(`try_into_string` error, as `validate`) or an unknown style:
`Error::general_error(format!("unknown style '{style}': expected html or ansi"))` (AC-9).

## Relevant Commands

New: `template/highlight`. Existing namespace `template` (`render`, `expand`, `validate`) unchanged.

## Documentation Architecture

`specs/reference/TEMPLATE_LANGUAGE.md` (§4 command list, new §8, History, `reviewed:`);
`specs/guides/TEMPLATE_GUIDE.md` §1 (History, `reviewed:`).
`affects_docs: [TEMPLATE_LANGUAGE, TEMPLATE_GUIDE]`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | as Integration Points |
| Crates and workflows affected | `liquers-template` (additive), `liquers-lib` `templates`/`egui` |
| Existing tests likely to change | `registry_export` (registry regenerated); none in `liquers-template` |
| New validation | tiling over the vendored spec suite and `tests/render.rs` templates; command tests |
| Compatibility | additive; one new command |
| Data / performance | linear in template size |
| Security | HTML style escapes every slice; `title` attribute escaped |
| Features | `egui.rs` gated on both features; `scripts/check-build-matrix.sh`; wasm32 for `liquers-template` |
| Recovery | revert; nothing persisted |
| Certainty | high once core tiling exists; delimiter tracking is the delicate part (AC-2) |
