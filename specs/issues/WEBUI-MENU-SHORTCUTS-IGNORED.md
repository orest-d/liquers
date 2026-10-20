---
id: WEBUI-MENU-SHORTCUTS-IGNORED
kind: issue
title: UISpec menu keyboard shortcuts do nothing in the browser
status: draft
priority: P2
complexity: M
area: [lib/ui]
design: ui-events
created: 2026-10-19
github:
---

## Problem

A `UISpec` menu item may carry a `shortcut` (`Ctrl+N`). The egui backend honours it:
`UISpecElement::check_shortcut` (`liquers-lib/src/ui/widgets/ui_spec_element.rs`) parses it with
`KeyboardShortcut::parse` and consumes it with `consume_shortcut`. The web backend ignores it:
`render_top_level_item_web` / `render_menu_item_web` emit only the `data-lq-action`, never the
shortcut, and the delegated `keydown` listener in `liquers-lib/src/ui/web/app.rs`
`dispatch_dom_event` returns for every key except Enter. Nothing in the browser path consults
`liquers-lib/src/ui/shortcuts.rs`.

Example: `liquers-lib/examples-web/ui_query_console_app` declares `New Console` with
`shortcut: Ctrl+N`. Natively, Ctrl+N adds a console. In the browser the same key press does nothing
(or reaches the browser), although the label shows the shortcut is meant to work.

## Impact

Every browser app built from a `UISpec` loses its keyboard accelerators silently; the menu still
works by mouse. No error is reported, so a spec author cannot tell.

## Expected behaviour

A shortcut declared in a `UISpec` menu runs its menu action in the browser as it does in egui, using
the shared `KeyboardShortcut` matcher, without overriding native key behaviour (text editing, Tab,
IME composition, a field's own Enter action). Shortcuts the browser reserves and never delivers to
the page (Chrome's Ctrl+N/T/W) cannot be honoured; that limit should be documented.

## Discovery

Found during the `webui-fixes` review (2026-07) and recorded only as "W5" in prose in
`specs/archive/2026-08-08-issues.md` and `specs/design/ui-events/`; it had no issue file. Filed
on 2026-10-19 while resuming `ui-events` under the bulk-design procedure, after confirming at HEAD
that the web renderer emits no shortcut and the keydown handler matches none.

Overlap: attached to `ui-events` (tests T2, shared contract: keyboard precedence between a field's
own action, shortcuts and the browser; and T4: `ui-events` already listed this as W5). No
runner-up; no other open design touches the web keydown path.
