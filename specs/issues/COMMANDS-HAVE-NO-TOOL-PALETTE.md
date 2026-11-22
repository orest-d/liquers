---
id: COMMANDS-HAVE-NO-TOOL-PALETTE
kind: feature
title: Commands cannot be offered as a palette of tool buttons
status: draft
priority: P3
complexity: M
area: [lib/ui]
design: 
created: 2026-11-21
github:
---
## Problem

Some commands behave like tools a user picks (drawing, image operations, inserting a step), but the
UI has no palette: a grid of icon buttons, each applying a command (possibly with a preset) as the
next action or as the tool of an interactive element.

## Impact

Tool-like commands are reachable only through the command picker or by typing the query.

## Expected behaviour

A palette element listing commands chosen by a hint (`palette` or `toolbar`, which
`CommandMetadata.hints` already mentions as an example), grouped by namespace or a declared group,
drawn with the icons of `query-editor-ergonomics` AC-17, with tooltips from label and doc.
Basis: `specs/design/query-editor-ergonomics/reference-design.md` §7.

## Discovery

Split from `ICONS-CANNOT-BE-ASSETS-OR-UNICODE` on 2026-11-21, when its icon definition was absorbed
by `query-editor-ergonomics`.
