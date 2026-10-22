---
id: UI-ELEMENT-NESTED-IN-CONSOLE-HAS-NO-HANDLE
kind: issue
title: A UIElement shown inside a query console is never initialised and has no handle
status: draft
priority: P3
complexity: M
area: [lib/ui]
design: console-mounts-nested-element
created: 2026-10-19
github:
---
## Problem

When a `QueryConsoleElement` (`liquers-lib/src/ui/widgets/query_console_element.rs`) receives a
snapshot whose value is a `UIElement`, `update` stores `elem.clone_boxed()` in its private
`ui_element` field and `show_content` renders it with `show_in_egui`. The nested element is never
placed in `AppState` and `UIElement::init` is never called, so its `handle()` stays `None`.

Any element that needs its handle therefore does nothing when shown inside a console. A console
nested in a console is the concrete case: `submit_query` and `request_asset_updates` send
`AppMessage::RequestAssetUpdates` only `if let Some(handle) = self.handle`, so pressing Enter in the
inner console silently does nothing. The nested element also receives no `update` messages.

## Impact

Only passive elements (those that render data they already hold) work inside a console. This
constrains every element design: `specs/design/ui-asset-snapshot-boundary/` had to load the text
editor's data at command time for this reason. Workaround: show the element in its own node
rather than through a console.

## Expected behaviour

Either the console mounts a `UIElement` result as a child node in `AppState` (so `init`, updates and
the runner apply), or the limitation is documented on `UIElement` and enforced (for example, a
visible notice when an element that needs a handle is rendered without one).

## Discovery

Found while designing `specs/design/ui-asset-snapshot-boundary/`, checking whether an element
could request its own data in `init`.

## Triage 2026-10-21

Reproduced at `8ecfd7f` (`QueryConsoleElement::update` keeps `clone_boxed()` in `ui_element`; no
`init`). Concrete case: a console evaluating `hello/q/ns-lui/query_console`; Enter in the nested
console does nothing. Designed in `specs/design/console-mounts-nested-element/` (mount as a child
node through a runner message).
