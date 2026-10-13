---
id: NO-TEMPLATE-EDITOR-UI
kind: feature
title: Templates and queries have no editor with highlighting and diagnostics
status: draft
priority: P3
complexity: L
area: [lib/ui, lib/egui]
design: 
created: 2026-10-13
github:
---
## Problem

There is no editor in the Liquers UI (egui or web) for templates — nor for queries beyond the query
console — that highlights syntax and shows parse errors and warnings at their positions while
typing. The query highlighter (`liquers-core/src/query.rs` `QueryRenderStyle`, `StyledQueryToken`,
used by `liquers-lib/src/egui/widgets.rs`) renders a finished query; it is not an editing surface.

## Impact

Once templates exist (`NO-QUERY-AWARE-TEMPLATE-LANGUAGE`), authors write them in an external editor
and learn about errors only when rendering. Low urgency: templates can be validated from the
command line or by a command.

## Expected behaviour

A template (and query) editor widget for egui and the web UI, using the template highlighter and
the parser's positioned diagnostics, with a preview of the rendered result in *evaluate* or *apply*
mode. Deliberately out of scope of `design/liquers-mustache/`.

## Discovery

Split out by the user on 2026-10-13 from the template-language request: a UI editor is out of scope
for now and recorded separately.
