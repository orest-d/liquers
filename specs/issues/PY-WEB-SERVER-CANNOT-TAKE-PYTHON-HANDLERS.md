---
id: PY-WEB-SERVER-CANNOT-TAKE-PYTHON-HANDLERS
kind: feature
title: A Liquers server started from Python cannot route a request to a Python handler or middleware
status: draft
priority: P3
complexity: L
area: [py, axum]
design:
created: 2026-11-06
github:
---

## Problem

The `liquers-py-web-server` design serves the standard Rust routes only. The guide's second half of
`WEBSERV` (`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §WEBSERV, tests `WEBSERV06`–`08`) lets
language code attach HTTP handlers or middleware, with documented request and response types, safe
mapping of a handler exception, and no blocking of the language runtime by concurrent handlers.
Nothing in `liquers-axum` takes a handler that is not a Rust function, and no Python request or
response type exists.

## Impact

A Python user who needs a custom endpoint must put it behind a command (`/liquer/q/...`) or run a
Python web framework beside the server (`WEBAPI`, milestone M8). `WEBSERV06`–`08` stay `NS`.

## Expected behaviour

A design that decides whether handlers are commands (`COMMAND`-based, as the plan's §5 suggests),
plain callables with request and response wrappers, or are left to `WEBAPI`. It shares the
"framework-neutral handler" question with M8.

## Discovery

`liquers-py-web-server` Phase 1, 2026-11-06: the plan scopes M7's `WEBSERV` to `01`–`05`, so the
rest needs its own record.
Filed without a design on purpose: the plan defers it until command-based handlers are designed,
and the handler question is shared with `WEBAPI` (milestone M8), where it is decided.
