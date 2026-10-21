---
id: UI-QUERY-EDITOR-LACKS-LIVE-VALIDATION
kind: issue
title: Query editor cannot validate the current text with command metadata
status: draft
priority: P2
complexity: L
area: [lib/ui, lib/egui, web, core/plan, core/query]
design: query-editor-live-validation
created: 2026-09-04
github:
---

## Problem

The query console currently highlights a token only after an `AssetSnapshot` supplies an execution
error. It cannot validate the text currently being edited: its egui `edit_query`-style field has no
access to the command metadata registry and does not build a plan from the query.

The browser query console has a plain escaped `<input>` and no syntax-highlighting query editor at
all. Egui-specific layout code therefore cannot be the shared editor implementation.

## Impact

Users receive invalid-command, argument, and planning feedback only after submitting a query and
waiting for asset processing. Web users additionally receive no token-level syntax presentation.
The recently completed `UI-QUERY-CONSOLE-NO-ERROR-HIGHLIGHT` change can display a positioned error,
but only when another path has already produced one.

## Expected behaviour

Provide a portable query-editor model/widget that receives the current query text and command
metadata registry, builds a plan or validation result without executing the query, and exposes
syntax tokens plus an optional positioned diagnostic. The egui adapter should render that model in
an editable field; the web adapter should render equivalent token markup and diagnostic state.

The existing `StyledQuery::from_query` token model and
`query_to_layout_job_with_position` egui helper should be reused where applicable. The portable
layer must not depend on egui or HTML and should preserve the syntax-only/no-diagnostic path for
incomplete or unparseable input.

## Discovery

Review of `UI-QUERY-CONSOLE-NO-ERROR-HIGHLIGHT` found that its fix consumes only errors attached to
asset snapshots. A complete editing experience instead needs synchronous plan construction against
the command metadata registry, plus a backend-independent representation so the web query console
gains the syntax-highlighting editor it currently lacks.

## Problem example

Added 2026-10-21 (bulk design; reproduced at `8ecfd7f`). Typing `ns-pl/head-x` in the egui console
shows it coloured and silent; only after Enter does `Can't convert 'x' to integer` (byte 11)
appear. The field should underline `x` while typing. The same review found that the console's
layouter lays out a copy of the text taken before the edit, and the re-encoded query: `ns-pl/-/head`
re-encodes as `-R/ns-pl/-/head`, so the galley differs from the buffer. Both are designed in
`specs/design/query-editor-live-validation/`.
