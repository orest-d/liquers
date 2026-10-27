---
id: PY-COMMAND-DECORATOR-CANNOT-DECLARE-ARGUMENT-METADATA
kind: feature
title: Python command decorators cannot declare per-argument metadata
status: draft
priority: P3
complexity: S
area: [py]
design: py-command-decorator-declaration-keywords
created: 2026-10-18
github:
---

## Problem

`@lq.command` and `@lq.first_command` (`liquers-py/python/liquers/_commands.py` `_decorator`)
accept `name`, `namespace`, `label`, `doc`, `filename`, `volatile` and `cached`, but no
`arguments=`. The declaration pipeline merges per-argument entries over the introspected baseline
(`specs/reference/COMMAND_DECLARATION.md` §9), but a Python author has no way to supply one, so
argument labels, descriptions, `gui_info` and enum values cannot be declared from Python.

## Problem Example

```python
@lq.command(arguments={"count": {"label": "Repetitions", "gui_info": {"IntegerSlider": {"min": 1, "max": 9}}}})
def repeat(text: str, count: int = 2) -> str:
    return text * count
# TypeError: decorate() got an unexpected keyword argument 'arguments'
```

Only the Python side is missing: `_register` (`liquers-py/python/liquers/_commands.py`) builds the
declaration dict from a fixed keyword list, and `PyCommandSpec::new` already passes it to
`CommandDeclaration::enhance`, which merges `arguments` entries by name over the baseline
(`merge05_argument_entry_augments_by_name`) and rejects an unknown name
(`merge06_unknown_argument_name_is_rejected`).

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
