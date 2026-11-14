---
id: EGUI-EXAMPLES-HAND-WRITE-THE-FRAME-LOOP
kind: issue
title: Six egui examples keep their own runtime, runner loop and repaint policy instead of the native driver
status: draft
priority: P3
complexity: S
area: [lib/egui]
design:
created: 2026-11-10
github:
---

## Problem

`specs/design/egui-native-app/` gives native hosts `NativeDriver` and `run_window`, and moves
`ui_query_console_app.rs` and `ui_spec_demo.rs` onto them. The other six egui examples in
`liquers-lib/examples/` (`egui_async_prototype.rs`, `ui_button_app.rs`, `ui_hello.rs`,
`ui_payload_app.rs`, `ui_spec_interactive.rs`, `ui_spec_simple.rs`) still write their own Tokio
runtime, `AppRunner` loop, repaint policy and `eframe::App`.

## Impact

Behaviour fixed in the driver, such as *Quit* closing the window, does not reach those examples,
and readers copy a loop the library no longer recommends.

## Expected behaviour

Each example builds a `NativeDriver` and calls `run_window`, keeping only what it demonstrates.
`egui_async_prototype.rs`, which demonstrates the loop itself, may stay as it is and say so.

## Discovery

`egui-native-app` Phase 2, 2026-11-10: moving every example would have doubled the design's diff,
and `ui_button_app.rs` is also edited by `text-editor-save` (in review).

## Triage

Blocked until `egui-native-app` lands. After that it is an automatic-fix candidate (size `S`,
examples only, no public interface change), taking `ui_button_app.rs` after `text-editor-save`.
