---
id: WEBUI-SUBMIT-QUERY-STATE-NOT-PRESERVED
kind: issue
title: Submitted query state is not preserved in the browser
status: closed
priority: P2
complexity: M
area: [lib/ui]
design: ui-events
created: 2026-08-08
github:
---
## Resolution

Fixed by `specs/design/ui-events/`. The console no longer submits through `UiAction::Apply` /
`lui/submit`: Go (and Enter) send `UIEvent { kind: Submit, fields: [query] }` to the console,
whose `update` sets `query_text`, pushes history and requests the submitted query, so the
re-rendered input, history and volatile/expiry refresh all use it. Evidence:
`query_console_element.rs` `test_ui_submit_sets_query_history_and_requests_updates`,
`test_expired_after_ui_submit_requests_submitted_query`; `tests/ui_invalidation.rs`
`ui_event_reaches_element_and_records_repaint`; Playwright `clicking Go submits the typed query and
the console keeps it`.

Source: PR #10 review (chatgpt-codex-connector, 2026-07-22) — `liquers-lib/src/ui/commands.rs:367`

## Problem
When the web QueryConsole's "Go" control emits `ApplyToInput`, `lui/submit` only forwards
`RequestAssetUpdates`; it bypasses `QueryConsoleElement::submit_query`, so `query_text` and history are
never updated with the live DOM input. After the result triggers a re-render, the input is rebuilt
from the old `self.query_text`, and volatile/expired refresh paths also resubmit that stale query.

## Problem Example
In `liquers-lib/examples-web/ui_query_console_app`, open a console (initial query `hello`), type
`about`, click "Go". The content shows the `about` result, but when the console re-renders the
input reverts to `hello`, the history holds only `hello`, and a volatile or expired refresh would
re-request `hello`. Expected: the input shows `about`, history ends with `about`, and refreshes
request `about`. The Playwright case `W2: the input reverts to the previous query after submitting`
asserts today's broken behaviour. Confirmed at HEAD on 2026-10-19 (`submit` in
`liquers-lib/src/ui/commands.rs` sends only `AppMessage::RequestAssetUpdates`; nothing updates
`QueryConsoleElement::query_text`).

## Fix direction
Update the console element's state (or carry the submitted query through the snapshot) before
requesting asset updates, so `query_text`/history reflect the live input.

## Verification
Type a new query, submit, trigger a re-render; assert the input retains the submitted query and a
volatile refresh uses it (not the previous value).
