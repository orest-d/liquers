# Phase 1: High-Level Design — template-syntax-highlighter

## Purpose

Template authors and reviewers should see a template's structure: tags, sigils, names, comments,
slots, and the queries inside query tags coloured as the query highlighter colours them, with
diagnostics marked in place. It serves the template use case of `specs/reference/VISION.md`
(context packs and reports assembled from queries), and is the token layer the template editor
(`template-editor-ui`) needs.

## Problem Example

Verified at HEAD (`8ecfd7f`): `liquers-template/src` has no highlighting code (`grep highlight`
finds nothing), and `liquers-lib/src/template/commands.rs` registers only `render`, `expand` and
`validate`. Highlighting was AC-10 of `design/liquers-mustache/`, moved out on 2026-10-17.

A template `templates/page.md`:

```
# {{title}}
{{#$ -R-key/notes/-/ns-search/field_extrct-md}}- {{name}}{{/}}
```

Today `-R/templates/page.md/-/ns-template/validate` reports `unknown command 'field_extrct'` at
2:31 and the author counts columns. Should be: `-R/templates/page.md/-/ns-template/highlight`
(validated with the command declared) returns the template as HTML in which `{{`/`}}` are
delimiters, `#$` a sigil, `title`/`name` names, `-R-key`/`notes`/`field_extrct`/`md` coloured as
query header, resource and action tokens, and `field_extrct` additionally marked as the error with
its message; `highlight-ansi` returns the same for a terminal.

## Scope and Acceptance Criteria

- **AC-1** Highlight command marks the diagnostic
  WHEN `-R/templates/page.md/-/ns-template/highlight` runs on the template above
  THEN the result is HTML whose concatenated text content is the template, `field_extrct` is in a
  span with classes `lq-tok-ActionName lq-tok-error` and the diagnostic message as `title`
- **AC-2** Tokens tile the source
  WHEN any template that parses is highlighted
  THEN the template tokens cover it exactly, in order, without gap or overlap (including after a
  set-delimiter tag `{{=<% %>=}}`, standalone lines and Unicode text)
- **AC-3** Queries in hygienic tags use query token kinds
  WHEN a hygienic query tag `{{$ cmd-{{x}}/q}}` is highlighted
  THEN `cmd` and `q` are `Query(ActionName)`, `{{x}}` is one `Slot` token, and the separators are
  `Query(Separator)`
- **AC-4** Classic bodies are slots and text
  WHEN a classic tag `{{$$ cmd-{{x}}}}` is highlighted
  THEN its body is `Slot` and `Text` tokens, not query tokens (it is parsed only after expansion)
- **AC-5** Invalid tags stay visible
  WHEN a template has a recoverable invalid tag
  THEN that tag's range is an `Invalid` token, the rest is highlighted normally
- **AC-6** Irrecoverable templates
  WHEN the template does not parse (an unclosed section)
  THEN highlighting returns the whole text as one `Text` token plus the parse diagnostic, and the
  command still succeeds
- **AC-7** ANSI style
  WHEN `highlight-ansi` runs
  THEN the result is the template text with ANSI colour codes, and stripping them gives the source
- **AC-8** egui rendering
  WHEN `liquers-lib`'s egui helper lays out the tokens of a template
  THEN the galley text equals the template text
- **AC-9** Unknown style
  WHEN `highlight-xml` runs
  THEN the command fails with an error naming the accepted styles

Non-goals: an editor (`template-editor-ui`); highlighting inside a classic body after expansion;
Markdown/HTML syntax of the text between tags.

## Core Interactions

- **Template** (`liquers-template`): a highlighter over the AST, positions only, no re-lexing of
  queries; render styles for HTML and ANSI.
- **Query** (`liquers-core`): reuses `SyntaxKind` and `tile_syntax_marks` from
  `query-editor-live-validation`.
- **Commands** (`liquers-lib`, feature `templates`): `template/highlight`.
- **UI** (`liquers-lib`, feature `egui`): a layout-job helper.

## Crate Placement

Tokens and styles in `liquers-template` (depends on core only, builds for wasm32); the command and
the egui helper in `liquers-lib/src/template/`.

## Documentation Intent

- Reference: `specs/reference/TEMPLATE_LANGUAGE.md` §4 lists `template/highlight`; a new §8
  "Highlighting" gives the token kinds.
- Guide: `specs/guides/TEMPLATE_GUIDE.md` §1 "Validate before committing" mentions `highlight`.
- Other documents: `specs/command_registry.yaml` regenerated.
- Documents to update: the source issue.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — L (two crates); adds `pub` types and a command (rules 1, 3, 4).
- **Leading issue:** **Open design - token shape:** byte-range tokens
  (`TemplateSyntaxToken { kind, start, end }`, kinds wrapping core `SyntaxKind`) instead of the
  issue's text-carrying `TemplateToken::Query(StyledQueryToken)`. Text-carrying tokens re-encode
  queries and cannot back an editor (AC-2, AC-8). Recommended: byte ranges.
- **Explanation:** every element and query piece carries a byte position in template coordinates,
  so tiling needs no second lexer; Phases 3-4 are written under the recommendations below.
- **Open questions:**
  1. **Open design - token shape:** as the leading issue.
  2. **Proposed resolution - diagnostics in the command:** `highlight` runs `validate_template`
     with the environment's registry and marks every diagnostic; no separate `Position` argument.
  3. **Proposed resolution - command shape:** `template/highlight(state, style: String = "html")`,
     styles `html` and `ansi`; text result, no `filename:` (the style decides the format).
  4. **Proposed resolution - CSS classes:** the same `lq-tok-<kind>` classes as the query editor,
     plus `lq-tok-Delimiter`, `-Sigil`, `-Name`, `-Slot`, `-Comment`, `-Invalid`.

## Design Dependencies

- `query-editor-live-validation`: **requires** its Step 1 (`SyntaxKind`, `SyntaxMark`,
  `tile_syntax_marks`). Order: after that step.
- `template-editor-ui`: **required-by** — the editor lays out these tokens.
- `liquers-mustache` (complete): origin of the source; frozen, not edited.

## Consolidated Findings

- The issue's salvaged design (`TemplateToken::Query(StyledQueryToken)`) is replaced by byte-range
  tokens: required for an editor, and the queries' positions are already in template coordinates.
- Tiling depends on the active delimiters, which a set-delimiter tag changes mid-template; the
  walk tracks them (AC-2 test over the vendored Mustache spec suite).
- The size is `L` (two crates, new command), not the `M` of the filing; the issue is re-sized.
