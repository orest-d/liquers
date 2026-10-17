---
id: LIQUERS-MUSTACHE
kind: design
title: liquers-template — a query-aware template language (Mustache syntax)
workflow: liquers-project
status: approved
phase: implementation
area: [core/query, core/value, lib/commands, lib/ui, docs]
issues: [NO-QUERY-AWARE-TEMPLATE-LANGUAGE, SIMPLE-TEMPLATE-HAS-NO-COMMAND]
merged: 2026-10-13
affects_docs: [specs/reference/TEMPLATE_LANGUAGE.md, specs/guides/TEMPLATE_GUIDE.md, specs/reference/VALUE_TYPE_SYSTEM.md, specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md, specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md, specs/reference/PROJECT_OVERVIEW.md, specs/guides/COMMAND_REGISTRATION_GUIDE.md, specs/guides/COMMAND_DESIGN_GUIDE.md, specs/reference/COMMAND_ALIASES.md]
created: 2026-10-13
---
# liquers-template — a query-aware template language (Mustache syntax)

## Phases

- [Phase 1](./phase1-high-level-design.md) — written 2026-10-13, revised twice on 2026-10-17 after review, **approved 2026-10-17**; AC-11 revised and AC-19 added after approval, the same day
- [Phase 2](./phase2-architecture.md) — written and reviewed 2026-10-17, revised the same day after review, **approved 2026-10-17**; commands revised after approval, the same day
- [Phase 3](./phase3-examples.md) — written and reviewed 2026-10-17, revised the same day, **approved 2026-10-17**
- [Phase 4](./phase4-implementation.md) — written 2026-10-17, pre-approved
- [Phase 5](./phase5-documentation.md)

## Notes

Supporting notes: [`query-placeholders.md`](./query-placeholders.md), the positions a placeholder
may fill in a hygienic query tag, how each is expanded, and how a query tag is evaluated;
[`data-node-interface.md`](./data-node-interface.md), the core data trait that lets any value be
template data, with the sync, async and dual comparison;
[`names-and-errors.md`](./names-and-errors.md), the name grammar with indexing, the error modes and
recoverable syntax errors.

**Revision 2026-10-17**, from the maintainer's review of Phase 1 (the crate and namespace names here were superseded by the Phase 2 review below). Two query-tag forms replace the
`{{x}}` / `{{{x}}}` split inside queries: hygienic `{{$ …}}` fills the AST and classic `{{$$ …}}`
expands the text. The hygienic positions are specified in full, including action names and
filenames. A shadow query parser in `liquers-mustache` replaces the public `liquers-core` parser
surface, held to the core parser by parity tests (new AC-14), so `liquers-core` is no longer changed.
Nested-brace balancing was agreed. The tags are now described one by one. The proposed namespace
changed from `tpl` to `mustache`.

Created 2026-10-13 at the user's request, from `agent-memory-mvp` Phase 1 open question 7. The
language choice — Mustache with query tags, over Jinja, Liquid and Handlebars — was argued in
[`../agent-memory-mvp/template-language-evaluation.md`](../agent-memory-mvp/template-language-evaluation.md);
the user then set the requirements this design starts from: a combination of Mustache and native
Liquers capabilities (evaluating and expanding templates, Liquers values as input); a parser that
extends the query parser with nom and `Position`; `{{…}}` inside queries for parametrized queries,
string-level or syntactic (hygienic) or both, to be discussed (open question 1); a highlighter
extending the query's; two modes, *evaluate* and *apply*; a separate crate `liquers-mustache` with the
commands in `liquers-lib`. A UI editor is out of scope and filed as `NO-TEMPLATE-EDITOR-UI`.

`SIMPLE-TEMPLATE-HAS-NO-COMMAND` (filed 2026-10-12 for `agent-memory-mvp`) is attached as a second
source: its fix — evaluation through the asset manager with dependencies — is this design's AC-2.

**Revision 2026-10-17 (second)**, from the maintainer's second review. `{{/}}` closes any section.
Evaluate and apply became a per-tag choice (`{{$ q}}` evaluates, `{{$. q}}` applies to the current
context, new open question 11), so one render command serves both. A non-scalar placeholder is a
parameter override that routes the query through `apply` (AC-16, which settles 2a). A core data
interface, `DataNode` / `AsyncDataNode`, was added (AC-15, open question 10). It brings a
`liquers-core` change back, a new module this time rather than the parser. Filed
`STORE-HANDLER-FIELD-ACCESS-HARDCODED` and `RECIPE-OVERRIDE-ONLY-LAST-ACTION`.

**Revision 2026-10-17 (Phase 2 review)**, from the maintainer. The crate is renamed
`liquers-template` and the namespace is `template`, so other template syntaxes can be added beside
Mustache. The design folder keeps its slug. Highlighting (AC-10) was withdrawn and filed with its
design as `TEMPLATE-HAS-NO-HIGHLIGHTER`. Open questions 12 and 13 were accepted. The command takes a
`cwd` argument, and the AST is specified in Phase 2 on the model of `Query`. The same day the `cwd`
argument was aligned with the command registration guide: a default link `-R-key/.`, last in the
signature (open question 14). Later the same day, names gained list indexing and a strict bracket lookup
`x[y]`, and rendering gained three error modes (strict, expand, silent) with recoverable syntax
errors kept as `Invalid` AST nodes. Both are in [`names-and-errors.md`](./names-and-errors.md), and
Phase 1 gained AC-17 and AC-18. Phase 2 was approved the same day. Its review of Phase 3 added
`TemplateHost::command_registry`, which `instantiate` needs to name an override, and filed
`REGISTRY-QUERY-DEFAULT-NOT-WRITABLE-AS-TEXT` and `SEARCH-COMMANDS-RECORD-NO-DEPENDENCY-ON-READ-FILES`.

**Revision 2026-10-17 (after Phase 2 approval)**, from the maintainer. Rendering is two commands:
`render` takes the template as its first argument and `expand` takes it as the state; `validate`
reads the state. The state and `data` are two frames of one context stack, `data` on top and a map.
A template command is an alias of `render` with the template link as its head (AC-19), documented
in the template guide and referred to from the command registration and design guides and
`COMMAND_ALIASES.md`. Phase 1 AC-11 was revised and AC-19 added; Phases 1 and 2 are re-approved at
the Phase 3 gate.

**Pre-approved after Phase 2 on 2026-10-17** (`proceed all`, which also re-approved Phases 1–3).
The decision log is in Phase 1, Design Readiness.
