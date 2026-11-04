---
id: PY-CANNOT-RENDER-TEMPLATES
kind: feature
title: Python cannot declare template commands or render a template from a script or a command
status: draft
priority: P2
complexity: L
area: [py, lib/commands]
design: liquers-py-templates
created: 2026-11-04
github:
---

## Problem

`liquers-py` registers the `ns-template` commands (`liquers-py/src/builtins.rs`,
`register_template_commands!`), so a query can render a template. Nothing else of the guide's
`TEMPLATE` feature (`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §TEMPLATE) exists:

- no `env.template_command(name, template=…)`: a template command (an alias of `template/render`
  with the template link as head) can be declared only from Rust or by YAML metadata that
  `import_metadata` refuses, because it names no registered command;
- no `env.render` / `env.expand` / `env.validate`, so a script that wants "fill this template from
  this `dict`" must encode the template into a query by hand;
- no `context.render` / `expand` / `validate` inside a Python command;
- a Python command cannot declare a default link argument (`cwd: lq.Key = lq.link("-R-key/.")`,
  guide Appendix A `TEMPLATE11`): `_baseline` in `liquers-py/python/liquers/_commands.py` accepts
  only `None`, `str`, `int`, `float` and `bool` defaults.

**Example.** `env.expand("{{name}} has {{$ number-42}}", data={"name": "Ann"})` raises
`AttributeError`. Expected `"Ann has 42"` (guide Appendix A `TEMPLATE04`).

## Impact

Python users can render templates only through hand-built query text. `TEMPLATE` is `NS` for
liquers-py (guide §6 Python, "Templates and data nodes"); `liquers-py/README.md`'s conformance
matrix has no `TEMPLATE` row.

## Expected behaviour

The guide's §TEMPLATE, tests `TEMPLATE01`–`11`, recorded in the conformance matrix. The top-level
render needs a core helper (`TEMPLATE-TOP-LEVEL-RENDER-HAS-NO-HOST`, design `template-render-in-env`)
or an equivalent.

## Discovery

Plan update for milestone M6 of the liquers-py plan, 2026-11-04: the guide's feature table lists
`TEMPLATE`, and no issue recorded that liquers-py does not implement it.
