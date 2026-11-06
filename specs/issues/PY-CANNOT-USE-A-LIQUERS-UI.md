---
id: PY-CANNOT-USE-A-LIQUERS-UI
kind: feature
title: Python cannot hold, drive or observe a Liquers UI model on its environment
status: draft
priority: P3
complexity: L
area: [py, lib/ui]
design: liquers-py-ui
created: 2026-11-06
github:
---

## Problem

`liquers-py` links `liquers-lib` without the `egui` feature (`liquers-py/Cargo.toml`), its
environment has no UI payload (`PyEnvironment = DefaultEnvironment<Value>`,
`liquers-py/src/environment.rs`), so the `lui` commands are not registered
(`liquers-py/src/builtins.rs`), and an `ExtValue::UIElement` result has no Python form. A Python
script cannot show elements, submit queries to them or follow their status, with or without a
window (the window: `PY-CANNOT-OPEN-A-LIQUERS-UI-WINDOW`).

**Example.** `env.evaluate("hello/q/ns-lui/query_console")` fails with an unknown command, and
there is no `env.ui()`. Expected: an `lq.UIElement`, and a model that shows elements and reports
their status (guide Appendix A `UIUSE02`–`07`).

## Impact

The owner's "GUI" usage mode (plan §1) is unavailable; `UIUSE` is `NS` for liquers-py. Workaround:
write a Rust `eframe` application (`liquers-lib/examples/ui_query_console_app.rs`), which cannot
call Python commands.

## Expected behaviour

`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §UIUSE: show elements from queries, receive status
events in order, ignore stale results, dispose of subscriptions and map errors, with no window and
no egui.

## Discovery

Milestone M7 of the liquers-py plan, 2026-11-06.
