---
id: ICON-SETS-OTHER-THAN-PHOSPHOR-ARE-NOT-DRAWN
kind: feature
title: Renderers draw only Phosphor set icons; a theme cannot supply another icon set
status: draft
priority: P3
complexity: M
area: [lib/ui, lib/egui]
design: query-editor-ergonomics
created: 2026-11-22
github:
---
## Problem

A theme can name its default icon set (`icons.icon-set`), and a bare icon name resolves against
it, but `egui::theme::draw_icon` and `ui::web::editor::icon_html` draw only `phosphor` set icons
(the egui Phosphor font, the Phosphor web stylesheet). An icon of any other set (`lucide:filter`,
or a bare name under `icon-set: lucide`) is skipped, and the renderer falls through to the next
variant or the Unicode text.

## Impact

Switching the icon set only makes sense once a renderer can draw the new set. Workaround: inline
SVG variants or a `lib:` library of SVGs.

## Expected behaviour

A way to supply a set: a font plus a name-to-codepoint table for egui, a stylesheet or SVG sprite
for the browser, registered by set name (perhaps named in the theme). `is_known_set_icon` would
then ask the registry instead of comparing with `PHOSPHOR`.

## Discovery

Closing `THEME-ICON-SET-SETTING-IS-IGNORED`, 2026-11-22.
