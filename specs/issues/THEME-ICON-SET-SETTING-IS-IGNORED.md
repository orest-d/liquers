---
id: THEME-ICON-SET-SETTING-IS-IGNORED
kind: issue
title: A theme's icons.icon-set is declared but a bare icon name always means Phosphor
status: closed
priority: P3
complexity: S
area: [lib/ui]
design: query-editor-ergonomics
created: 2026-11-22
github:
---
## Problem

`IconTheme.icon_set` (`icons.icon-set` in a theme, `liquers-lib/src/ui/theme/icon.rs`) is
documented as "the set a bare name refers to", but nothing reads it: `IconVariant::parse` maps a
bare name and `ph:` to the constant `PHOSPHOR`, and both renderers draw only `phosphor` set icons
(`egui::theme::draw_icon`, `ui::web::editor::icon_html`). Setting `icon-set: lucide` changes
nothing, and an icon of another set (`lucide:filter`) is always skipped.

## Impact

A theme cannot switch the icon set, and the setting suggests it can. Workaround: write icons as
SVG variants, or keep Phosphor.

## Expected behaviour

Either remove the key (it is part of the theme format, so keep it accepted) or make it work: a
bare name resolved against `icons.icon-set` at composition, and a renderer hook for icon sets
other than Phosphor (a font or an SVG sprite per set). Until then `reference/THEME.md` §2.3 says
it is not read.

## Discovery

Writing `specs/reference/THEME.md`, 2026-11-22.

## Resolution

2026-11-22, `query-editor-ergonomics` (at Orest's request: keep the setting, Phosphor by default
so it need not be given). A bare name parses to `IconVariant::Set { set: None, .. }`, and
`Theme::icon_for` fills the set in from `icons.icon-set` (`IconTheme::resolve_set`); `ph:` stays
Phosphor. The built-in theme gives `phosphor`, the field defaults to it, and a value that is not a
set name fails composition. The built-in namespace table names its icons `ph:` explicitly.
Evidence: `ui::theme::tests::bare_icon_names_follow_the_icon_set`; `reference/THEME.md` §2.2–2.4.
Drawing a set other than Phosphor is split off as `ICON-SETS-OTHER-THAN-PHOSPHOR-ARE-NOT-DRAWN`.
