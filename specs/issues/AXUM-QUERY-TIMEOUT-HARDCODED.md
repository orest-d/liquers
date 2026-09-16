---
id: AXUM-QUERY-TIMEOUT-HARDCODED
kind: issue
title: The query handler's 30-second timeout is hardcoded
status: draft
priority: P2
complexity: S
area: [axum]
design: 
created: 2026-09-16
github:
---
## Problem

`get_query_handler` (`liquers-axum/src/query/handlers.rs:41-42`) polls the asset for a result with
`tokio::time::Duration::from_secs(30)` written inline. The value is not a field on
`QueryApiBuilder`, not read from configuration, and not documented in
`reference/WEB_API_SPECIFICATION.md` §7. An embedding application cannot change it without forking
the handler.

## Impact

Any query whose evaluation legitimately exceeds thirty seconds is unreachable over `GET /q`, and
the caller gets an `ExecutionError` that reads like a failure rather than a deadline. The
workaround is real — the assets API plus its WebSocket is the documented path for work that takes
time — but it is not discoverable from the error, and thirty seconds is a guess that will be
wrong in both directions for someone.

## Expected behaviour

The timeout is a builder option with the current value as its default, and the specification says
what it is and what a caller should do when it fires. Pointing the timeout error at the assets API
would make the workaround discoverable at the moment it is needed.

## Discovery

Analysis for `AGENT-MEMORY-SERVICE`, 2026-09-16. Read at HEAD.
