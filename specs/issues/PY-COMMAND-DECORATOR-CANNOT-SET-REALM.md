---
id: PY-COMMAND-DECORATOR-CANNOT-SET-REALM
kind: feature
title: Python command decorators cannot place a command in a realm
status: draft
priority: P3
complexity: S
area: [py]
design: py-command-decorator-declaration-keywords
created: 2026-10-18
github:
---

## Problem

`@lq.command` and `@lq.first_command` (`liquers-py/python/liquers/_commands.py`) take `namespace=`
but no `realm=`, and `Environment.unregister` and `command_metadata` look a command up by name and
namespace only, with the realm fixed to `""`. A Python command therefore always lives in the
default realm, while `register_command!` accepts `realm:`.

## Problem Example

```python
@lq.command(realm="server", namespace="tools")
def shout(text: str) -> str:
    return text.upper()
# today:    TypeError: decorate() got an unexpected keyword argument 'realm'
# expected: registered as realm "server", namespace "tools"; env.command_metadata("shout",
#           namespace="tools", realm="server") finds it
```

Re-checked at HEAD (2026-10-26): `_decorator` in `liquers-py/python/liquers/_commands.py` still
takes `name, namespace, label, doc, filename, volatile, cached, env` only.

## Impact

Python authors who need a command in a non-default realm cannot declare one. No known user needs
it yet; the omission was deliberate scope control in `design/liquers-py/`.

## Expected behaviour

The decorators accept `realm=`, passed through the declaration like `namespace`, and the lookup
methods accept a matching keyword. Alternatively, the realm stays fixed and the guide records why.

## Discovery

Intentionally omitted scope of `specs/design/liquers-py/`, recorded in Phase 5 and in the
`COMMAND` row of `liquers-py/README.md`.

Overlap triage (bulk design, 2026-10-27): designed together with
`PY-COMMAND-DECORATOR-CANNOT-DECLARE-ARGUMENT-METADATA` (leading) in
`design/py-command-decorator-declaration-keywords/`. Test that held: T3, the same keyword list in
`_decorator.decorate` / `_register`; T1, T2, T4 did not. Runner-up: `language-store-config-function`
(T2 on what `realm="store_config"` means; not attached because its Phase 4 is finished, recorded as `overlaps`, and that design's
refusal of the realm is carried as AC-7).
