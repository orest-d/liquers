---
id: PY-CANNOT-START-A-WEB-SERVER
kind: feature
title: Python cannot serve its environment over the Liquers HTTP API
status: draft
priority: P2
complexity: L
area: [py, axum]
design: liquers-py-web-server
created: 2026-11-06
github:
---

## Problem

`liquers-py` does not depend on `liquers-axum` and has no server API. The standard routes exist as
builders generic over the environment (`QueryApiBuilder`, `StoreApiBuilder`, `AssetsApiBuilder`,
`RecipesApiBuilder` in `liquers-axum/src/`), but they can be assembled only from Rust
(`liquers-axum/examples/basic_server.rs`), on a Rust-built environment that has none of the
commands a Python script registered.

**Example.** `env.serve(port=0, background=True)` raises `AttributeError`. Expected: a server
handle whose address answers `GET /liquer/q/greet-Ann` with the Python command's result
(guide Appendix A `WEBSERV01`, `WEBSERV02`).

## Impact

The owner's "server" usage mode of liquers-py (plan §1, decisions of 2026-10-17) is unavailable;
`WEBSERV` is `NS` in `liquers-py/README.md`'s conformance matrix. Workaround: none from Python.

## Expected behaviour

`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §WEBSERV, tests `WEBSERV01`–`05`: foreground and
background start, readiness, typed startup errors, graceful shutdown, a stated lifetime policy
for the server, bound to the same environment as the script.

## Discovery

Milestone M7 of the liquers-py plan (`/mnt/project-files/plans/liquers-py-implementation-plan.md`
§M7), 2026-11-06: no issue recorded that liquers-py does not implement `WEBSERV`.
