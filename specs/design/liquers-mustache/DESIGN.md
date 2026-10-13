---
id: LIQUERS-MUSTACHE
kind: design
title: liquers-mustache — a query-aware template language
workflow: liquers-project
status: in_review
phase: high-level
area: [core/query, lib/commands, lib/ui, docs]
issues: [NO-QUERY-AWARE-TEMPLATE-LANGUAGE, SIMPLE-TEMPLATE-HAS-NO-COMMAND]
merged: 2026-10-13
affects_docs: []
created: 2026-10-13
---
# liquers-mustache — a query-aware template language

## Phases

- [Phase 1](./phase1-high-level-design.md) — written 2026-10-13, awaiting approval
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

## Notes

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
