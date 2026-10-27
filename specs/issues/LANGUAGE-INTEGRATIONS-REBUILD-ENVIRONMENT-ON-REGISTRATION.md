---
id: LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION
kind: issue
title: liquers-web and liquers-py rebuild the whole environment for each late registration
status: draft
priority: P3
complexity: M
area: [web, py]
design:
created: 2026-10-20
github:
---

## Problem

After the first evaluation, a `command`, `first_command`, `unregister` or `import_metadata` call in
`liquers-py` marks the environment for a rebuild (`reference/PYTHON_INTEGRATION.md` §Rebuild), and
`liquers-web` rebuilds on a late registration batch (`liquers-web/README.md`, limitation row).
Every rebuild discards the in-memory asset cache and keeps the replaced environment on a retired
list.

## Problem Example

```python
env = lq.Environment()
env.evaluate("-R/data/big.csv/-/slow_summary")   # computed, held in the asset cache
@env.command
def extra(text: str) -> str: return text
env.evaluate("-R/data/big.csv/-/slow_summary")   # computed again: the rebuild dropped the cache
```

Re-checked at HEAD (2026-10-26): `design/post-init-command-registration/` is `in_review` and
unimplemented (no `EnvRef::update_commands`), so both integrations still rebuild.

## Impact

Results that were not stored are recomputed after every late registration, and retired environments
accumulate. Correct but wasteful; no functional workaround besides registering everything before
the first evaluation.

## Expected behaviour

Once `design/post-init-command-registration/` lands (`EnvRef::update_commands`), both integrations
apply late registrations, removals and metadata imports through it instead of rebuilding, keeping
their replay tests green. Depends on that design's decision on the trait shape.

## Discovery

Split out of `POST-INIT-COMMAND-REGISTRATION` during the 2026-10-20 bulk design, which designs the
core mechanism and leaves the integrations' switch to this issue.

Note (2026-10-22): since `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` was fixed, an environment no longer
keeps itself alive, so in liquers-py a retired environment is freed by `shutdown()` (which empties
the retired list) rather than never. It is still held until then, although in-flight work now pins
its own environment and the retired list is no longer needed for that.

