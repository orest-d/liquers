---
id: PY-UI-CHANGE-DOES-NOT-WAKE-AN-OPEN-WINDOW
kind: issue
title: A change made from Python while a window shows waits for the window's next poll
status: draft
priority: P3
complexity: S
area: [py, lib/egui]
design:
created: 2026-11-11
github:
---

## Problem

While `UI.run()` shows a window, the model's driver is lent to the window
(`liquers-py/src/ui.rs` `DriverSlot::Lent`). `UiModel::note_change`, which `show` and `submit` call
after their change, wakes only a background task (`HeadlessHandle::waker`), so a change made from
another thread, or from a callback, is picked up at the window's next poll: up to
`WindowOptions::poll_interval` (100 ms in liquers-py) while nothing evaluates, longer where the
platform throttles a hidden window.

## Impact

A query shown from a script or a server thread appears up to a tenth of a second late in an idle
window. Nothing is lost.

## Expected behaviour

A change from Python requests a repaint of the open window at once: `run_window` could expose a
repaint handle (the `egui::Context` it already keeps in `WindowCloser`), which `note_change` calls
while the slot is `Lent`.

## Discovery

The codebase and soundness review of `liquers-py-ui-window` Phase 4 (2026-11-11), item 8.
