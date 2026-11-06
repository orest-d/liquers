---
id: PY-CANNOT-OPEN-A-LIQUERS-UI-WINDOW
kind: feature
title: Python cannot open the Liquers desktop UI in a window
status: draft
priority: P3
complexity: L
area: [py, lib/egui, build]
design: liquers-py-ui-window
created: 2026-11-06
github:
---

## Problem

`liquers-py` links `liquers-lib` without the `egui` feature (`liquers-py/Cargo.toml`), so no
published wheel contains the egui backend, and there is no call that opens a window over a Python
environment. `liquers-lib` itself has no reusable native window to call
(`EGUI-HAS-NO-REUSABLE-NATIVE-APP`).

**Example.** A script that has an `lq.UI` model (`liquers-py-ui`) cannot show it: there is no
`ui.run()`. Expected: the model's elements in an egui window on the main thread (guide Appendix A
`UIUSE01`, `UIUSE08`).

## Impact

The owner's "GUI" and "GUI plus server" usage modes are unavailable. Workaround: a Rust `eframe`
program (`liquers-lib/examples/ui_query_console_app.rs`), which cannot call Python commands.

## Expected behaviour

`ui.run()` on the main thread, `ui.close()` from any thread, callbacks and Ctrl-C handled between
frames, and a decision on whether the wheels carry egui.

## Discovery

`liquers-py-ui` Phase 1 review, 2026-11-06: the window half was split from the model so the wheel
decision does not hold up the rest of `UIUSE`.
