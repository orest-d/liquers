# Phase 1: High-Level Design — web-ui-define

## Design Readiness

Decision log of the pre-approved run (`proceed all`, 2026-10-30).

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. It adds exported JavaScript functions and classes and a
  public `liquers-lib` element type (rules 3 and 4); complexity `L`.
- **Leading issue:** **Blocking for Step 6 only, `ui-events`:** `onEvent` needs `ui-events`'
  `UserEvent` (decision 2). That design is in review with readiness `needs-decision`: its leading
  question is one `UpdateMessage::User` variant with a closed `UserEventKind`, which it recommends,
  versus one variant per event kind. Until it is approved and implemented, Steps 1–5 can ship and
  `UIDEF03` stays partial.
- **Explanation:** Phases 2–4 specify a working solution under the four Phase 1 decisions. The
  event half waits on another design's approval.
- **Open questions:**
  1. **Blocking (Step 6), `ui-events` approval:** as the leading issue.
  2. **Proposed resolution, delegate registry per thread:** a definition is visible on the thread
     that registered it. In the browser that is the one thread, and it is how `UIDEF08`'s thread
     affinity is met.
  3. **Proposed resolution, `mount` lifecycle:** `mount` runs again whenever the element's markup
     is replaced, after the previous cleanup. A canvas widget redraws on each props change, rather
     than receiving an `update` call.
  4. **Proposed resolution, page renderer entry point:** `mountRenderer(renderer, options)` is a
     separate function, not an overload of `mountUI`. It keeps `mountUI`'s container check typed.
  5. **Proposed resolution, errors in callbacks:** a throwing `render`, `onEvent` or `mount` is
     shown in the element and written to `console.error`. It is not a status event, because
     status events come from the runner, which never sees these calls.
  6. **Proposed resolution, markup trust:** `render`'s string is inserted as HTML, as Rust
     `render_web` markup is. The page escapes its own data; the guide says so.

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
  WHEN the page undefines a type, or disposes a mount
  THEN the type's functions (on undefine), or the mount's `mount` cleanups and renderer (on dispose),
  are released, and the handle count returns to its earlier value
- **AC-7** A JavaScript renderer shows a standard element (`UIDEF07`)
  WHEN a page calls `mountRenderer(renderer, options)` instead of `mountUI`, and the tree holds a built-in
  element such as `markdown`
  THEN the renderer receives the tree and each element's markup or value, re-renders on change
  notifications, and can send actions back
- **AC-8** Callbacks cannot corrupt the UI (`UIDEF08`)
  WHEN a `render` or `onEvent` callback calls back into the mount, or an element's evaluation
  finishes while a callback is running
  THEN calls that need the UI lock from inside `render` (which runs under it) or that target the
  element being updated are refused with a `LiquersError`, every other call is applied on the next
  tick, and nothing panics or loses an update

Non-goals: egui elements written in JavaScript; hot reload beyond re-rendering existing instances;
accessibility beyond the page's own markup.

## Core Interactions

- **UI** (`liquers-lib/src/ui/`): one Rust element type stands for every JavaScript-defined type and
  delegates rendering and events to a registry of page callbacks; the driver gains a headless mode
  that publishes the tree and changes instead of writing the DOM.
- **Value types**: a JavaScript-defined element is an `ExtValue::UIElement` like any other, so it
  can be stored, passed between commands and mounted.
- **Bindings**: `defineElement`, `undefineElement`, `element(type, props)`, and `mountRenderer`.

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

## Open Questions

None. Every recommendation was accepted by the maintainer on 2026-10-27, and is recorded below as a decision.

1. **The element contract.** An element type has a pure `render(props) → HTML string` and an
   `onEvent(props, event) → props`. A widget that owns a canvas or a third-party component can add an
   optional `mount(container, props)` / `unmount()` pair. Nothing uses a virtual-tree format.
2. **Native events.** This design waits for `ui-events` and uses its `UserEvent`, so Rust and
   JavaScript elements share one event vocabulary.
3. **A JavaScript renderer (AC-7).** It stays in this design as the last milestone.
4. **Properties must be JSON.** Properties that cannot be serialized as JSON are refused when the
   element is created.

## Scope Changes

- **2026-10-30, Phase 2 review (pre-approved).** AC-6 now names the two things that release
  functions, undefining a type and disposing a mount, because the registry is per thread, not per
  mount. AC-8 now says what happens inside `render`, which runs under the UI lock. The page
  renderer has its own entry point, `mountRenderer`. Size unchanged; Phases 1–4 updated.

## References

- `guides/LANGUAGE-INTEGRATION_GUIDE.md` §UIDEF and Appendix A `UIDEF`
- `specs/design/web-ui-use/`, `specs/design/ui-events/`, `specs/design/webui-fixes/` (change
  records and `data-lq-children`)
- `liquers-lib/src/ui/element.rs` (`UIElement`, `render_web`, `show_in_web`)
