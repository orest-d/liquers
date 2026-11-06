---
id: EGUI-HAS-NO-REUSABLE-NATIVE-APP
kind: feature
title: liquers-lib has no reusable native egui application; every desktop app re-implements the frame loop
status: draft
priority: P2
complexity: M
area: [lib/egui, lib/ui]
design:
created: 2026-11-06
github:
---

## Problem

The browser backend has one entry point, `mount_web` (`liquers-lib/src/ui/web/app.rs`), with a
`MountHandle`. The egui backend has none: each example (`liquers-lib/examples/ui_query_console_app.rs`,
`ui_spec_demo.rs`, `ui_button_app.rs` and five more) defines its own `eframe::App`, its own Tokio
runtime, its own `AppRunner` loop, root rendering and repaint policy, and calls
`eframe::run_native` itself. No library function opens a window on a given `EnvRef` with given root
queries, reports status events, or closes on request.

## Impact

A host that is not a Rust `main` (liquers-py's `UIUSE`, a future CLI) would have to copy an
example. The copies already differ (repaint policy, lock handling), so behaviour fixed in one, such
as the stale-result rule of `web-ui-use`, is not guaranteed in the others.

## Expected behaviour

A library entry point in `liquers-lib` behind the `egui` feature, the counterpart of `mount_web`:
run a native window over an `EnvRef` and an `AppState`, with options (title, size), root queries,
a close request usable from another thread, status events drained from `AppRunner::take_events`,
and an environment replacement (`AppRunner::replace_envref`). The examples then use it.

## Discovery

`liquers-py-ui-window` Phase 1, 2026-11-06: Python's `UIUSE` needs a window it can call, and there is
none. Moving the examples onto it touches `ui_query_console_app.rs`, which `command-parameter-editor`
and `query-editor-live-validation` (in review) also edit.
