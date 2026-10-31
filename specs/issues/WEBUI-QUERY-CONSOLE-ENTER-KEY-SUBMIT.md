---
id: WEBUI-QUERY-CONSOLE-ENTER-KEY-SUBMIT
kind: issue
title: Enter key does not submit in the browser query console
status: closed
priority: P2
complexity: M
area: [lib/ui]
design: ui-events
created: 2026-08-08
github:
---
## Resolution

Fixed by `specs/design/ui-events/`. The console input declares `data-lq-field="query"
data-lq-event="submit"`; the web driver's keyboard router (`ui/web/keys.rs` `route_keydown`) turns
an unmodified Enter on it into a `Submit` UI event. Clicking into the input is not an activation.
Evidence: `keys.rs` `enter_in_declared_field_is_a_field_event`; Playwright `pressing Enter in the
input submits it`, `clicking into the input does not submit`.

Source: PR #10 review (chatgpt-codex-connector, 2026-07-22) — `liquers-lib/src/ui/widgets/query_console_element.rs:461`

## Problem
In the browser, Enter-key events originate on the `<input>`, and `dispatch_dom_event` looks only at
the target's closest `[data-lq-action]` ancestor. The current markup puts `data-lq-action` on the
sibling `<span>` (the "Go" button) instead of the input or one of its ancestors, so pressing Enter in
the query console returns without sending `ApplyToInput` — only clicking "Go" works.

## Problem Example
In `liquers-lib/examples-web/ui_query_console_app`, open a console from the menu (`New Console`,
query `hello/q/ns-lui/query_console/add-child`), type `about` into the query input and press Enter.
Today nothing happens: the content still shows the `hello` result. Clicking "Go" instead shows the
`about` result. Expected: Enter submits exactly as "Go" does. The Playwright case
`W1: pressing Enter in the input does not submit` in
`liquers-lib/examples-web/tests/ui_query_console_app.spec.ts` asserts today's broken behaviour.
Confirmed at HEAD on 2026-10-19 (`QueryConsoleElement::render_web` puts the action on the sibling
`<span class="lq-go">`; `dispatch_dom_event` searches only the input's ancestors).

## Fix direction
Put the action on the input (or a shared toolbar ancestor of both the input and the button), or
special-case the input element on Enter in `dispatch_dom_event`.

## Verification
Playwright: type a query, press Enter, assert the result renders (currently only a click works).
