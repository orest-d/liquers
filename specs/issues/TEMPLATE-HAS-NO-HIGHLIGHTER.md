---
id: TEMPLATE-HAS-NO-HIGHLIGHTER
kind: feature
title: Templates cannot be rendered with syntax highlighting
status: draft
priority: P3
complexity: M
area: [lib/ui, lib/egui, lib/commands]
design:
created: 2026-10-17
github:
---

## Problem

Queries can be shown highlighted (`liquers-core/src/query.rs` `QueryRenderStyle`,
`StyledQueryToken`, `DarkAnsiQueryRenderStyle`; the egui widgets in
`liquers-lib/src/egui/widgets.rs`). Templates of `liquers-template` cannot. Highlighting was AC-10
of `design/liquers-mustache/` and was moved out of that design on 2026-10-17 at the maintainer's
request, to keep the first version small.

## Impact

Authors and reviewers read templates as plain text. A query error reported at a position is harder
to find without the query styled inside its tag. Low urgency: `ns-template/validate` reports
positioned diagnostics, and an editor (`NO-TEMPLATE-EDITOR-UI`) is a later consumer of this work.

## Expected behaviour

The design salvaged from `design/liquers-mustache/` Phase 2 (revision of 2026-10-17):

- `liquers-template` gets a `highlight(template: &Template) -> Vec<TemplateToken>` that walks the
  AST. Every element carries a `Position`, so no re-lexing is needed.
- It has its own token enum rather than a new `StyledQueryToken` variant, which would break
  exhaustive matches outside core:
  `enum TemplateToken { Query(StyledQueryToken), Slot(String), Tag(String), Name(String), Text(String), Comment(String) }`.
  The queries inside hygienic tags reuse the core `styled_tokens` of the instantiated-shape
  `QueryTemplate`, so they are styled with the same token kinds the query highlighter uses. Classic
  `{{$$ …}}` bodies are styled as `Slot` and `Text` pieces, because they are not parsed until
  expansion.
- There is a render style trait with ANSI and HTML implementations, mirroring `QueryRenderStyle`.
  An optional `Position` highlights the element at a diagnostic's position.
- `liquers-lib/src/template/egui.rs` (feature `egui`) renders `TemplateToken`s as the query widgets do.
- A command, `register_command!(cr, fn highlight(state, style: String = "html") -> result namespace: "template")`,
  returns the template as styled HTML or ANSI text.

## Discovery

Split out of `design/liquers-mustache/` Phase 2 on 2026-10-17. The maintainer asked to skip the
highlight command, remove highlighting from the design, and file it with the existing design kept.
