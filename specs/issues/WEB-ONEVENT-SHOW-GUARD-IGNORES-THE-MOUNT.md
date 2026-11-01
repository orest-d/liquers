---
id: WEB-ONEVENT-SHOW-GUARD-IGNORES-THE-MOUNT
kind: issue
title: show() from an onEvent is refused for the same handle number in another mount
status: draft
priority: P3
complexity: S
area: [web]
design:
created: 2026-10-31
github:
---
## Problem

While a page-defined element's `onEvent` runs, `liquers-web` records the element's `UIHandle` in a
thread-local (`ui_define::element_being_updated`), and `UIMount.show(query, handle)` refuses to
replace that handle, because the runner puts the element back after the handler. The record holds
only the handle, not the mount: handles are numbered per `AppState`, so with two mounts on a page,
`show(q, 1)` on mount B is refused while element 1 of mount A handles an event.

`ElementDelegate::on_event` receives the element and the event but not the `UIContext`, so the
delegate cannot tell which mount the element belongs to.

## Impact

A page with two mounts whose `onEvent` replaces an element of the other mount gets a spurious
"is being updated" error. Rare: it needs two mounts and a cross-mount `show` from inside a handler.

## Expected behaviour

The guard refuses only the element being updated in its own mount, for example by recording the
`AppState` identity with the handle (passing the `UIContext`, or its `app_state` pointer, to
`on_event`).

## Discovery

Implementing `web-ui-define` Step 6 (2026-10-31).
