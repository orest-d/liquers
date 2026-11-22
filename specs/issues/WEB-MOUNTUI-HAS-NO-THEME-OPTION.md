---
id: WEB-MOUNTUI-HAS-NO-THEME-OPTION
kind: feature
title: liquers-web's mountUI cannot choose a theme, mode or density
status: draft
priority: P3
complexity: S
area: [web, lib/ui]
created: 2026-11-21
github:
---
## Problem

`liquers-lib`'s browser mount takes the `ui:` configuration section (`MountOptions.ui`: theme,
mode, density), but `liquers-web`'s `mountUI(container, options)` always passes `ui: None`
(`liquers-web/src/ui.rs`), so a page mounted from JavaScript gets the built-in theme following
`prefers-color-scheme`, and cannot load a theme file, force light or dark, or pick the compact
density.

## Impact

A page can only restyle the editor by overriding the `--lq-*` CSS custom properties in its own
stylesheet. Workaround: that.

## Expected behaviour

`mountUI` (and `mountRenderer`) accept `{ ui: { theme?, mode?, density? } }` with the same values
as the YAML section, passed through to `MountOptions.ui`; the JavaScript guide and the stubs
document it. Language-integration feature: the UI mount (`UIUSE`).

## Discovery

`query-editor-ergonomics` Step 11, 2026-11-21, while writing the Web guide's section on editing
queries. Phase 1 judged the JavaScript integration not needed for the design; this is the one
host-level gap left.
