---
id: PY-CANNOT-USE-A-LIQUERS-UI
kind: feature
title: Python cannot open, drive or observe the Liquers desktop UI on its environment
status: draft
priority: P3
complexity: XL
area: [py, lib/ui, lib/egui]
design: liquers-py-ui
created: 2026-11-06
github:
---

## Problem

`liquers-py` links `liquers-lib` without the `egui` feature (`liquers-py/Cargo.toml`), its
environment has no UI payload (`PyEnvironment = DefaultEnvironment<Value>`,
`liquers-py/src/environment.rs`), so the `lui` commands are not registered
(`liquers-py/src/builtins.rs`), and an `ExtValue::UIElement` result has no Python form. A desktop UI
on a Python environment cannot be opened at all.

**Example.** `env.evaluate("hello/q/ns-lui/query_console")` fails with an unknown command, and
there is nothing like `ui.run()` to show it. Expected: the query console opens in a window on the
script's environment, and Python can follow element status (guide Appendix A `UIUSE01`–`08`).

## Impact

The owner's "GUI" usage mode (plan §1) is unavailable; `UIUSE` is `NS` for liquers-py. Workaround:
write a Rust `eframe` application (`liquers-lib/examples/ui_query_console_app.rs`), which cannot
call Python commands.

## Expected behaviour

`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §UIUSE: start the egui backend on the main thread,
show elements from queries, receive status events in order, ignore stale results, dispose of
subscriptions, map errors, and shut down respecting thread affinity.

## Discovery

Milestone M7 of the liquers-py plan, 2026-11-06.
