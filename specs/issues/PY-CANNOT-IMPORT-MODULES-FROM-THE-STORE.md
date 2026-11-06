---
id: PY-CANNOT-IMPORT-MODULES-FROM-THE-STORE
kind: feature
title: Python modules kept in a Liquers store cannot be imported, so code cannot live beside its recipes
status: draft
priority: P2
complexity: L
area: [py, core/assets]
design: liquers-py-modules
created: 2026-11-06
github:
---

## Problem

A store folder can hold data and `recipes.yaml`, but the Python commands its recipes call must be
installed on the host filesystem. `liquers-py` has no import hook over the store, no module search
path, and no way to record a module's key as a dependency of the results it computed.

**Example.** With `code/mymod.py` (`VALUE = 42`) in the store, `env.module_path =
[lq.parse_key("code")]` and `env.import_module("mymod")` raise `AttributeError`. Expected `42`
(guide Appendix A `MODULE01`).

## Impact

A project cannot be shipped as one store folder; editing a command's code in the store does not
expire the assets it computed. `MODULE` is `NS` for liquers-py. Workaround: read the bytes through
`env.store` and `exec` them by hand, with no trust boundary, caching or dependency tracking.

## Expected behaviour

`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §MODULE, tests `MODULE01`–`12`: an opt-in search path
of store prefixes (empty by default), standard-library precedence, relative resolution against the
command's working directory, a stated reload policy, module keys recorded as dependencies, typed
errors, and no deadlock from inside a running command.

## Discovery

Milestone M7 of the liquers-py plan, 2026-11-06.
