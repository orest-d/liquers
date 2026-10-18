---
id: PY-COMMAND-DECORATOR-CANNOT-DECLARE-ARGUMENT-METADATA
kind: feature
title: Python command decorators cannot declare per-argument metadata
status: draft
priority: P3
complexity: S
area: [py]
design:
created: 2026-10-18
github:
---

## Problem

`@lq.command` and `@lq.first_command` (`liquers-py/python/liquers/_commands.py` `_decorator`)
accept `name`, `namespace`, `label`, `doc`, `filename`, `volatile` and `cached`, but no
`arguments=`. The declaration pipeline merges per-argument entries over the introspected baseline
(`specs/reference/COMMAND_DECLARATION.md` §9), but a Python author has no way to supply one, so
argument labels, descriptions, `gui_info` and enum values cannot be declared from Python.

## Impact

Python commands get only derived argument labels and no GUI hints, which shows in generated UIs and
`commands_doc`. Workaround: `Environment.import_metadata` with YAML for the command, which
replaces the whole metadata entry.

## Expected behaviour

The decorators accept `arguments={"count": {...}}` and pass it in the declaration, matched by name
over the baseline as in `COMMAND_DECLARATION.md` §9; a test checks a declared label and `gui_info`
reach `command_metadata`.

## Discovery

Phase 5 review of `specs/design/liquers-py/` against `COMMAND_DECLARATION.md` §9, 2026-10-18.
