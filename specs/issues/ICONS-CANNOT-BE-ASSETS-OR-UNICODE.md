---
id: ICONS-CANNOT-BE-ASSETS-OR-UNICODE
kind: feature
title: An icon can only be named from an icon set; no SVG or PNG asset, Unicode fallback or tool palette
status: closed
priority: P3
complexity: L
area: [lib/ui]
design: query-editor-ergonomics
created: 2026-11-21
github:
---
## Problem

The only way to give a command an icon is the free command hint `hint icon: "<name>"`
(`CommandMetadata.hints`, `liquers-macro` `hint`), a bare string the `query-editor-ergonomics`
design reads as a Phosphor icon name. There is no way to use an SVG or PNG asset (needed for
non-standard icons and tool palettes), no Unicode fallback (needed for a planned terminal UI, which
can show neither SVG nor icon fonts), and no notion of a palette of commands picked as tools.

## Impact

Commands that behave like tools (drawing, image operations) cannot carry their own artwork, and a
TUI would have no icon to draw. Applications built on Liquers cannot brand their commands. No
workaround beyond the monogram fallback.

## Expected behaviour

One icon reference type, used for commands, namespaces (`NAMESPACES-HAVE-NO-METADATA`) and palette
entries, with three kinds: an asset (SVG or PNG by key or query), an icon-set name (Phosphor by
default), and a Unicode character. A definition may carry several; each renderer draws the first it
can (the TUI prefers Unicode, the GUIs an asset or set icon), then the monogram. Open points: the
string syntax of a reference, whether an asset icon is a query or a key, SVG tinting, PNG sizes,
and a registration-time check that a set name exists. A palette (a grid of icon buttons, each
applying a command, possibly with a preset) builds on it. Basis:
`specs/design/query-editor-ergonomics/reference-design.md` §7.

## Discovery

Orest's notes of 2026-11-21 on the query editor design: icons must be definable by users, tools
may be picked from a palette, and a future TUI needs Unicode icons. Recorded as a basis for a later
design, not a requirement of `query-editor-ergonomics`.

## Design

2026-11-21: the icon definition is absorbed by `query-editor-ergonomics` (Phase 1 AC-17, Phase 2
`ui/theme/icon.rs`): variants (set reference, SVG, raster; inline, embedded in code or by key) with
an always-available Unicode text. The tool palette is split off as `COMMANDS-HAVE-NO-TOOL-PALETTE`.

## Resolution

2026-11-21, `query-editor-ergonomics` Steps 2, 5, 7 and 9: `ui::theme::Icon` is a list of
variants (an icon-set name, a theme library icon, an SVG or raster inline, embedded in code or by
store key) with a Unicode text that a resolved icon always has; egui draws glyphs, SVG (resvg) and
rasters, the browser draws Phosphor CSS with a Unicode fallback and data-URI images. The string
syntax is the table of `ui/theme/icon.rs`; an asset icon is a key. Namespace icons come from the
theme (`icons.namespaces`, `icons.defaults`) until `NAMESPACES-HAVE-NO-METADATA`. The palette is
`COMMANDS-HAVE-NO-TOOL-PALETTE`. Evidence: tests 17, 17a, 19 and 26 of the design.
