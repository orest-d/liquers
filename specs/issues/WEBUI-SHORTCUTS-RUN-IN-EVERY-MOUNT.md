---
id: WEBUI-SHORTCUTS-RUN-IN-EVERY-MOUNT
kind: issue
title: With two web UI mounts on one page, an unfocused menu shortcut runs in both
status: draft
priority: P3
complexity: S
area: [lib/ui, web]
design:
created: 2026-10-31
github:
---

## Problem

Since `ui-events` (PR #225), `mount_web` attaches its `keydown` listener to the document rather
than to the mount's root (`liquers-lib/src/ui/web/app.rs`), so a menu shortcut works when nothing
is focused. Each mount adds its own listener. With two mounts on one page (which `web-ui-use`'s
`mountUI` makes possible), a shortcut pressed with no focus, or with focus outside both roots,
reaches both mounts, and each runs its own menu's matching action.

## Impact

Only a page with more than one mount whose menus share a shortcut. A single mount, and every
`data-lq-event` key, is unaffected: a field event only fires for a node inside the mount's root.

## Expected behaviour

An unfocused shortcut runs in one mount: for example the last one the user interacted with, or
the one a page marks as owning global shortcuts. A focused node inside a root routes only to that
mount.

## Discovery

Noticed while aligning `web-ui-use` with the `ui-events` implementation (2026-10-31).
