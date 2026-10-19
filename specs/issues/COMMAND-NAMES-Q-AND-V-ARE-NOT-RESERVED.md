---
id: COMMAND-NAMES-Q-AND-V-ARE-NOT-RESERVED
kind: issue
title: A command named q or v registers but can never run
status: draft
priority: P2
complexity: S
area: [core/commands, core/query]
design:
created: 2026-10-19
github:
---

## Problem

`CommandMetadata` validation (`liquers-core/src/command_metadata.rs`, around line 1222) reserves
only the name `ns`. The planner also treats `q` and `v` as query instructions
(`liquers-core/src/plan.rs`, around lines 1677 and 1791). A command named `q` or `v` is therefore
registered without an error or a warning, and a query `v` or `q` never calls it: from Python,
`@env.first_command def v(): return 7` followed by `env.evaluate("v")` returns `None`.

## Impact

A silently dead command: the author sees a successful registration and a `None` result, with
nothing pointing at the name. Affects every language (Rust macro, Python, JavaScript).

## Expected behaviour

Every name the planner intercepts is refused like `ns`, with a message naming it as reserved, and
the reserved list lives in one place that both the planner and the validation read.

## Discovery

Found by the Phase 5 review of `design/liquers-py/` (2026-10-19), probing names while checking
`guides/PYTHON_GUIDE.md`.
