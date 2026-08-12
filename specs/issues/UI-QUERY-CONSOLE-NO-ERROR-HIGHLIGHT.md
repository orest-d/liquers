---
id: UI-QUERY-CONSOLE-NO-ERROR-HIGHLIGHT
kind: issue
title: Query console never highlights the erroring token despite complete plumbing
status: draft
priority: P2
complexity: S
area: [lib/ui, core/query]
design: 
created: 2026-08-12
github:
---
## Problem

Every `Error` carries a `position: Position`, and the query-styling machinery can turn a `Position`
into an underlined token. The two are never connected, so the interactive query console reports
errors only as a line of red text below the field and leaves the query itself unmarked.

The highlight path is complete and unused:

| Step | Location |
|---|---|
| Styling takes a position | `StyledQuery::from_query(x, position: &Position)` — `liquers-core/src/query.rs:253` |
| Token becomes a highlight when the position matches | `to_highlight_if_matching` — `query.rs:319` |
| Highlight renders red and underlined | `liquers-lib/src/egui/widgets.rs:411` |

The break is at the entry point. `query_to_layout_job` (`liquers-lib/src/egui/widgets.rs:426`)
builds its `StyledQuery` with `StyledQuery::from(query)`, and that conversion
(`query.rs:259-271`) hardcodes `Position::unknown()`. No caller can pass a position, so
`StyledQueryToken::Highlight` is never produced by the console and the `.underline()` arm at
`widgets.rs:411` is dead code in practice.

`QueryConsoleElement` already holds the error it would need — `error: Option<Error>`
(`liquers-lib/src/ui/widgets/query_console_element.rs:51`), populated from the asset snapshot at
`:389` and rendered as a plain `colored_label` at `:316`.

## Expected behaviour

When the console holds an error whose position is known, the corresponding token in the query text
field is highlighted. A `Position::unknown()` error keeps the current message-only behaviour.

## Fix direction

Give `query_to_layout_job` a position parameter (or add a `query_to_layout_job_highlighted`
alongside it, leaving the existing signature for callers with nothing to highlight), thread
`self.error.as_ref().map(|e| &e.position)` from `QueryConsoleElement::render`, and pass it to
`StyledQuery::from_query` instead of the `From<Query>` conversion.

Scope is the egui console. The HTML rendering path (`error_html`, `query_console_element.rs:484`)
has the same information available and could follow, but is not covered by this issue.

## Discovery

Found while designing `PLAN-EXCESS-ACTION-PARAMETERS-DROPPED`, whose value depends on this: raising
a positioned error for an excess action parameter is what lets an editor point at the parameter, and
that payoff is only realised once the console consumes the position it is handed.
