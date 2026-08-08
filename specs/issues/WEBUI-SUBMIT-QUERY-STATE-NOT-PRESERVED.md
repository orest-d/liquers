---
id: WEBUI-SUBMIT-QUERY-STATE-NOT-PRESERVED
kind: issue
title: Submitted query state is not preserved in the browser
status: draft
priority: P2
complexity: M
area: [lib/ui]
design: ui-events
created: 2026-08-08
github:
---
Source: PR #10 review (chatgpt-codex-connector, 2026-07-22) — `liquers-lib/src/ui/commands.rs:367`

## Problem
When the web QueryConsole's "Go" control emits `ApplyToInput`, `lui/submit` only forwards
`RequestAssetUpdates`; it bypasses `QueryConsoleElement::submit_query`, so `query_text` and history are
never updated with the live DOM input. After the result triggers a re-render, the input is rebuilt
from the old `self.query_text`, and volatile/expired refresh paths also resubmit that stale query.

## Fix direction
Update the console element's state (or carry the submitted query through the snapshot) before
requesting asset updates, so `query_text`/history reflect the live input.

## Verification
Type a new query, submit, trigger a re-render; assert the input retains the submitted query and a
volatile refresh uses it (not the previous value).
