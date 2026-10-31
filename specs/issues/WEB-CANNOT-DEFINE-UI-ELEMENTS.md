---
id: WEB-CANNOT-DEFINE-UI-ELEMENTS
kind: feature
title: A page cannot define a UI element type or a UI backend in JavaScript
status: draft
priority: P3
complexity: L
area: [web, lib/ui]
design: web-ui-define
created: 2026-10-27
github:
---
## Problem

Every `UIElement` (`liquers-lib/src/ui/element.rs`) is a Rust type registered with `typetag` at
compile time, and the only renderer in the browser is `liquers-lib`'s `mount_web` driver. A page
cannot add an element type (a gauge, a chart on a canvas, a form built with its own framework), and
cannot render the element tree itself (React, Vue, a custom DOM). The `UIDEF` feature of
`guides/LANGUAGE-INTEGRATION_GUIDE.md` is "not started (deferred)" in `liquers-web/README.md`.

## Impact

A JavaScript command that wants to show something richer than `value_to_html` produces must return
HTML as a string value, which loses the element's handle, state, events and serialization. A page
built on a JavaScript UI framework cannot use Liquers' element tree at all.

## Expected behaviour

A page declares element types (render, events, optional DOM lifecycle) that behave as elements
inside the web backend, and can run the element tree with a renderer of its own. Depends on
`WEB-CANNOT-USE-A-LIQUERS-UI`. See `specs/design/web-ui-define/`.

## Discovery

Starting the UI features of the language integration guide for liquers-web (2026-10-27).
