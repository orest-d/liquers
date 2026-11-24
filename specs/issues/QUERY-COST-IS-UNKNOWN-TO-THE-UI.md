---
id: QUERY-COST-IS-UNKNOWN-TO-THE-UI
kind: feature
title: The UI cannot tell an expensive query from a cheap one, so an element's update mode cannot follow it
status: draft
priority: P3
complexity: M
area: [lib/ui, core/commands]
created: 2026-11-24
github:
---
## Problem

`specs/design/query-update-modes/` gives the query console and the editors an update mode (On
demand, Live preview, Apply immediately), chosen by a `mode` argument with a fixed default. An
expensive query should open On demand and a cheap one in Apply immediately, but nothing says which
is which: command metadata has no cost hint, and the plan does not estimate one. A UI spec author
has to know and pass the mode by hand.

## Problem Example

A UI spec opens `lui/query_console` on a report whose recipe trains a model for a minute. It starts
in Apply immediately, so each panel edit starts a minute-long evaluation, until the person notices
the status bar's suggestion and switches to On demand.

## Expected behaviour

A command can declare that it is expensive (for example a `hint cost: "high"` in
`register_command!`), the plan of a query can report whether any of its steps is, and the console
and editors use that for their default mode when no `mode` argument is given. Related:
`CORE-ONE-JOB-QUEUE-FOR-ALL-COMMANDS` (placing commands by cost).
