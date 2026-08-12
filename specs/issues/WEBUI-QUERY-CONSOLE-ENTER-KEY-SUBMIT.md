---
id: WEBUI-QUERY-CONSOLE-ENTER-KEY-SUBMIT
kind: issue
title: Enter key does not submit in the browser query console
status: accepted
priority: P2
complexity: M
area: [lib/ui]
design: ui-events
created: 2026-08-08
github:
---
Source: PR #10 review (chatgpt-codex-connector, 2026-07-22) — `liquers-lib/src/ui/widgets/query_console_element.rs:461`

## Problem
In the browser, Enter-key events originate on the `<input>`, and `dispatch_dom_event` looks only at
the target's closest `[data-lq-action]` ancestor. The current markup puts `data-lq-action` on the
sibling `<span>` (the "Go" button) instead of the input or one of its ancestors, so pressing Enter in
the query console returns without sending `ApplyToInput` — only clicking "Go" works.

## Fix direction
Put the action on the input (or a shared toolbar ancestor of both the input and the button), or
special-case the input element on Enter in `dispatch_dom_event`.

## Verification
Playwright: type a query, press Enter, assert the result renders (currently only a click works).
