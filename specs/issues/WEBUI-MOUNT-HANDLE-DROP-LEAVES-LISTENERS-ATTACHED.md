---
id: WEBUI-MOUNT-HANDLE-DROP-LEAVES-LISTENERS-ATTACHED
kind: issue
title: Dropping the web MountHandle leaves its event listeners attached to freed closures
status: draft
priority: P3
complexity: S
area: [lib/ui]
design:
created: 2026-10-30
github:
---

## Problem

`MountHandle` in `liquers-lib/src/ui/web/app.rs` documents "Dropping it detaches the
listeners", but it has no `Drop` impl: it only owns the `Closure`s. Dropping it frees the closures
while the `click` listener on the root and the `keydown` listener on the document (since
`ui-events`) stay registered, so the next event calls into a dropped closure and throws in
JavaScript. The runner loop spawned by `mount_web` also never stops.

## Impact

Only affects an application that unmounts and remounts the UI (none of the examples does); a
page that mounts once and keeps the handle is unaffected. Workaround: keep the handle alive
(`std::mem::forget`) for the life of the page.

## Expected behaviour

Dropping the handle removes both listeners (`remove_event_listener_with_callback` on the same
targets) and stops the runner loop, or the doc comment says the handle must outlive the page.

## Discovery

Noticed while moving the keydown listener to the document in the `ui-events` implementation
(2026-10-30).
