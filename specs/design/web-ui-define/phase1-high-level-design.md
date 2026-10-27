# Phase 1: High-Level Design — web-ui-define

## Purpose

Let a page add to the Liquers UI in JavaScript: define an element type that behaves like a built-in
element inside the web backend, and run the element tree with a renderer of its own (React, Vue or
plain DOM). This is the `UIDEF` feature of `guides/LANGUAGE-INTEGRATION_GUIDE.md` for the Browser
JavaScript profile. It builds on `web-ui-use`, which makes the existing UI usable from a page.

## Problem Example

```js
liquers.defineElement({
  type: "gauge",
  render: (props) => `<div class="gauge"><span>${props.value}%</span>
                        <button data-lq-event="click">+</button></div>`,
  onEvent: (props, event) => ({ ...props, value: props.value + 1 }),
});
liquers.registerCommand({ name: "gauge", run: (v) => liquers.element("gauge", { value: v }) });
await liquers.mountUI(app, { query: "gauge-50" });
```

**Today.** There is no `defineElement` and no `liquers.element`. Every `UIElement`
(`liquers-lib/src/ui/element.rs`) is a Rust type registered with `typetag` at compile time. The
command above can only return a plain object, which crosses as JSON and is shown as data by
`value_to_html`. There is no way to replace the renderer either: `mount_web` is the only driver.

**Should be.** The mount shows `50%` and a button. Clicking it calls this gauge's `onEvent`, the
element now holds `{ value: 51 }`, and only its node is re-rendered. A second gauge on the page is
untouched.

## Scope and Acceptance Criteria

- **AC-1** Register and render a JavaScript element (`UIDEF01`)
  WHEN a page defines type `gauge` and a query's value is `liquers.element("gauge", { value: 50 })`
  THEN the mount renders the gauge's markup inside a node with that element's handle
- **AC-2** Properties round-trip (`UIDEF02`)
  WHEN an event handler returns new properties, or the element tree is serialized and restored
  THEN the element renders and reports the new properties, unchanged by the round trip
- **AC-3** An event reaches the right instance (`UIDEF03`)
  WHEN the user clicks inside one of two gauges
  THEN only that gauge's `onEvent` runs, with its own properties and handle
- **AC-4** It works in every claimed backend (`UIDEF04`)
  WHEN the same element is rendered by the browser driver and by `render_app_ssr`
  THEN both produce the same markup; egui is not claimed
- **AC-5** An unknown type falls back (`UIDEF05`)
  WHEN an element names a type that is not defined (for example a tree restored before the page
  defined it)
  THEN it renders a placeholder naming the type, and renders properly once the type is defined
- **AC-6** Disposal releases callbacks (`UIDEF06`)
  WHEN the page undefines a type, or disposes the last mount using it
  THEN its retained JavaScript functions are released and the handle count returns to its earlier
  value
- **AC-7** A JavaScript renderer shows a standard element (`UIDEF07`)
  WHEN a page mounts with `{ renderer }` instead of a container, and the tree holds a built-in
  element such as `markdown`
  THEN the renderer receives the tree and each element's markup or value, re-renders on change
  notifications, and can send actions back
- **AC-8** Callbacks never run under the UI lock (`UIDEF08`)
  WHEN a `render` or `onEvent` callback submits a query, or an element's evaluation finishes while a
  callback is running
  THEN the work is applied on the next driver tick, with no `already borrowed` panic or lost update

Non-goals: egui elements written in JavaScript; hot reload beyond re-rendering existing instances;
accessibility beyond the page's own markup.

## Core Interactions

- **UI** (`liquers-lib/src/ui/`): one Rust element type stands for every JavaScript-defined type and
  delegates rendering and events to a registry of page callbacks; the driver gains a headless mode
  that publishes the tree and changes instead of writing the DOM.
- **Value types**: a JavaScript-defined element is an `ExtValue::UIElement` like any other, so it
  can be stored, passed between commands and mounted.
- **Bindings**: `defineElement`, `undefineElement`, `element(type, props)`, and the `renderer` option
  of `mountUI`.

## Crate Placement

`liquers-lib/src/ui/` for the language-neutral parts: the delegating element type's serialized form
(type name plus JSON properties) and the headless driver, which a Python or other integration can
reuse. `liquers-web` for the JavaScript callback registry, the retained functions and the exported
API. No `liquers-core` change is expected.

## Documentation Intent

- Reference: extend `reference/JAVASCRIPT_INTEGRATION.md` (element definition, renderer protocol).
- Guide: extend `guides/JAVASCRIPT_GUIDE.md` with "Define an element" and "Render the tree yourself".
- Other documents: the `UIDEF` row of `liquers-web/README.md`.
- Documents to update: `guides/LANGUAGE-INTEGRATION_GUIDE.md` §6 Browser JavaScript.

## Design Dependencies

- **requires** `web-ui-use`: mounting, the payload type, the stoppable driver and the event
  subscription this design extends.
- **requires (native events)** `ui-events` (in review, not implemented): its `UserEvent` and
  `data-lq-event` markup are how a click reaches an element's `update`. Without it an element can
  only use `data-lq-action` queries.
- **overlaps** `console-mounts-nested-element`: a JavaScript element returned into a console needs
  the console to mount it as a child node to receive events.
- **soft** `JS-COMMAND-CANNOT-ACCESS-CONTEXT`: lets an `onEvent` handler evaluate queries through a
  context rather than through module-level calls.

## Decisions

All recommendations accepted by the maintainer on 2026-10-27. No open questions remain.

1. **The element contract.** An element type has a pure `render(props) → HTML string` and an
   `onEvent(props, event) → props`. A widget that owns a canvas or a third-party component can add an
   optional `mount(container, props)` / `unmount()` pair. Nothing uses a virtual-tree format.
2. **Native events.** This design waits for `ui-events` and uses its `UserEvent`, so Rust and
   JavaScript elements share one event vocabulary.
3. **A JavaScript renderer (AC-7).** It stays in this design as the last milestone.
4. **Properties must be JSON.** Properties that cannot be serialized as JSON are refused when the
   element is created.

## References

- `guides/LANGUAGE-INTEGRATION_GUIDE.md` §UIDEF and Appendix A `UIDEF`
- `specs/design/web-ui-use/`, `specs/design/ui-events/`, `specs/design/webui-fixes/` (change
  records and `data-lq-children`)
- `liquers-lib/src/ui/element.rs` (`UIElement`, `render_web`, `show_in_web`)
