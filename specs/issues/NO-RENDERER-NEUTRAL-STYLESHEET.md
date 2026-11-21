---
id: NO-RENDERER-NEUTRAL-STYLESHEET
kind: feature
title: Styles cannot be written once as selector rules and resolved for egui, the browser and a terminal UI
status: draft
priority: P3
complexity: L
area: [lib/ui]
design: 
created: 2026-11-21
github:
---
## Problem

Liquers draws UI in egui and in the browser, and a terminal UI is planned. Only the browser has a
style language (CSS). The `query-editor-ergonomics` design gives egui and the browser one token
theme with overrides keyed by namespace and command (its AC-14), but there is no way to write a
rule once, such as "a focused card of namespace `pl`" or "an error field in Compact density", and
have every renderer resolve it.

## Impact

Themes beyond the editor's PoC must be written per renderer, and a TUI would need a third
mechanism. Users cannot restyle one kind of element without code. Workaround: the keyed overrides
of `query-editor-ergonomics`.

## Expected behaviour

A renderer-neutral **style query**: a widget describes itself as a short path of typed elements
with attributes and states (`editor > card[ns=pl][command=gt]:focus > field[type=int]`), asks the
theme to resolve it, and gets **tokens** (colours, spacing, font role, icon), which each renderer
maps to its own primitives: egui `Visuals`/`Frame`, CSS for the browser (or plain CSS there),
terminal styles for a TUI. No DOM is needed: in egui the path is built while drawing, and
results are cached by path.

Findings of 2026-11-21 (`specs/design/query-editor-ergonomics/reference-design.md` §4.2): no
universal library fits as is. Building blocks: servo's `selectors` (generic selector matching
over any type implementing its `Element` trait; MPL-2.0, very widely used) and `cssparser`
(MPL-2.0); the cascade (ordering by origin and specificity, `var()`) would be ours. Rejected as
foundations: `stylo` (a full browser engine computing browser properties), `egui_css` (WIP,
egui 0.29), `ratatui-style` (ratatui-only output), `style-cascade` (tied to its own accessibility
DOM, two weeks old). Python Textual's CSS for terminal UIs is a reference for the TUI side.

## Discovery

Orest asked on 2026-11-21, while settling the theme of `query-editor-ergonomics`, whether a
universal CSS or CSS-like library could resolve styles for egui and a TUI.
