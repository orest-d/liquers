---
id: NO-QUERY-AWARE-TEMPLATE-LANGUAGE
kind: feature
title: No template language understands Liquers queries and values
status: closed
priority: P2
complexity: XL
area: [core/query, lib/commands]
design: liquers-mustache
created: 2026-10-13
github:
---
## Problem

Liquers has one template mechanism, `simple_template` (`liquers-core/src/parse.rs`
`parse_simple_template`, `interpreter.rs` `evaluate_simple_template`): text with `$query$` spans. It
has no variables, sections or partials, collides with ordinary text (`$` occurs 341 times in 85
Markdown files of `specs/`), and no command exposes it (`SIMPLE-TEMPLATE-HAS-NO-COMMAND`).
An off-the-shelf engine (Mustache, minijinja) fills the other gaps but treats a query as an opaque
string: it cannot report a malformed query at its position in the template, cannot evaluate it
through the asset layer, and cannot use a Liquers value as its data.

What is missing is a template language that **is part of Liquers**:

- Mustache's data model and tags (interpolation, sections, inverted sections, partials, comments,
  set-delimiter), so templates are familiar, logic-less and checkable against the Mustache spec;
- query tags whose contents are parsed by the query parser — a template's parser **extends** the
  query parser (nom, `Position`), so errors and warnings carry template line and column;
- **parametrized queries**: a query may itself contain `{{…}}` expansions — whether the whole query
  is expanded as a string, or only certain syntactic positions (a parameter value, a resource
  segment), hygienically, is open;
- two modes over one AST — **evaluate** (run every statement and query; the template is a
  document) and **apply** (the template is a function applied to a `Value`, which becomes the data);
- syntax highlighting extending the query's (`QueryRenderStyle`, `StyledQueryToken`).

## Impact

Context packs and generated indexes in `agent-memory-mvp`, issue and design templates, reports and
any document assembled from several queries have to be written as bespoke commands. Template errors
surface only at run time and without positions. The predecessor project used `mustache` + `expand`
(ranees-liquers) and lost it when commands moved to `register_command!`.

## Expected behaviour

A new crate `liquers-template` (on `liquers-core`; named `liquers-mustache` until 2026-10-17) with the parser, AST, highlighter and both
evaluation modes, and `liquers-lib` commands to render, apply and validate templates. Design:
`specs/design/liquers-mustache/`. A template editor is a separate issue (`NO-TEMPLATE-EDITOR-UI`).

## Discovery

Raised by the user on 2026-10-13, answering `agent-memory-mvp` Phase 1 open question 7: the
template system must understand queries, so it should be reimplemented, and the language to
reimplement chosen for suitability (`design/agent-memory-mvp/template-language-evaluation.md`).

## Resolution (2026-10-17)

Implemented by `design/liquers-mustache/` (PR #139): the `liquers-template` crate (Mustache with `{{$ q}}` / `{{$$ q}}` query tags, sections, partials, apply forms, error modes) and the `template/render`, `template/expand` and `template/validate` commands. Reference: `specs/reference/TEMPLATE_LANGUAGE.md`; guide: `specs/guides/TEMPLATE_GUIDE.md`.
