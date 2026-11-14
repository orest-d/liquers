---
id: PY-APPLICATION-CANNOT-START-FROM-A-CONFIG-FILE
kind: feature
title: A Python Liquers application has no supported way to start from a configuration file with command-line overrides
status: draft
priority: P2
complexity: M
area: [py, docs]
design: py-command-line-config
created: 2026-11-14
github:
---

## Problem

A Liquers application usually starts by loading a configuration file and overriding a few of its
settings from the command line, such as the port or the command registry file. liquers-py has no
help for this. A program has to write its own `argparse` options and read the file. It cannot
apply overrides such as the port or the command registry from Python without losing the meaning
of the file's relative paths: a `dict` passed as `config` loses `ConfigDocument::base`, and the
`Environment` keywords carry neither `server:` settings nor `command_registry`. It cannot
configure the default environment at all (`PY-DEFAULT-ENVIRONMENT-CANNOT-BE-CONFIGURED`).

The user guide's UI and server examples (chapters 11 and 12) never load a configuration file.

## Problem Example

```
python app.py --config prod.yaml --port 9000
```

There is no supported way to turn this into an environment configured by `prod.yaml` with the port
overridden, served by `lq.serve()`.

## Impact

Every application writes the same start-up code, and gets the overrides and path resolution
subtly different. Workaround: parse the arguments yourself, edit the YAML text, and pass it as
`config`. That loses the file's folder, so a relative `command_registry:` resolves against the
working directory instead.

## Expected behaviour

The Liquers options can be added to a program's own `argparse` parser. The configuration file is
named or defaults to a well-known file, the overrides are applied natively over the parsed
document, and the environment is configured. This works step by step or in one call, with a
runnable example of each. The guide's UI and server examples load a YAML configuration
(Orest, 2026-11-14).

## Discovery

Orest asked for it on 2026-11-14 at the Phase 1 gate of `py-default-environment-config`, which
this builds on.
