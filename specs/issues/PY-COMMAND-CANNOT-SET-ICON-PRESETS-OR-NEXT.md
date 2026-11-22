---
id: PY-COMMAND-CANNOT-SET-ICON-PRESETS-OR-NEXT
kind: feature
title: A Python @command cannot declare usage hints (the editor icon), presets or next presets
status: draft
priority: P3
complexity: S
area: [py]
created: 2026-11-21
github:
---
## Problem

`lq.command` / `Environment.command` take `name`, `namespace`, `label`, `doc`, `filename`,
`volatile` and `cached` only (`liquers-py/python/liquers/commands.pyi`). The query editor draws a
command's card from its `hints["icon"]`, its `presets` and its `next` presets
(`query-editor-ergonomics`), all of which the Command Declaration Format has (`hints`, `presets`,
`next`, `COMMAND_DECLARATION.md`), but a Python command cannot set any of them, so its card shows
the namespace's default icon or a monogram, no preset chips and no "Continue with" chips.

## Impact

Python commands look poorer in the editor than Rust ones. Workaround: a theme can give a Python
command an icon (`icons.commands`), but there is no workaround for presets or next presets.

## Expected behaviour

The decorators accept `hints={"icon": "funnel"}`, `presets=[…]` and `next=[…]` (or `icon=`), with
the meaning of the declaration format, carried into `CommandMetadata`. Language-integration
feature: §COMMAND of `LANGUAGE-INTEGRATION_GUIDE.md`.

## Discovery

Phase 5 of `query-editor-ergonomics`, 2026-11-21, reviewing which integrations can reach the new
editor features. Phase 1 judged the Python integration not needed because the window needs no
binding change; the command side was not considered.
