---
id: PY-NO-LANGUAGE-DEFINED-RECIPE-PROVIDER
kind: feature
title: A recipe provider cannot be written in Python, and liquers-py has no RECIPE feature
status: draft
priority: P3
complexity: M
area: [py, core/assets, core/context]
design: liquers-py-recipes
created: 2026-10-30
github:
---
## Problem

> **2026-10-31:** the sketch below is superseded. Orest decided that a configuration names a
> Python-registered *factory* (`type` is its unique name, the entry's arguments its parameters);
> see the design folder in `design:`.

`liquers-py` cannot adapt a Python object to `AsyncRecipeProvider`, so the language-integration
guide's optional `RECIPE` feature (`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §RECIPE) has no
implementation. Since `design/recipe-provider-chain-config/` a Python environment can configure the
chain (`config["recipes"]`), but only from the built-in entry types `recipes_yaml` and `manifest`.

The chain is ready for one: an integration declares an entry type through
`EnvironmentBuilder::with_recipe_provider_factory`, and its constructor can look a Python object up
in a registry the integration owns (`{type: python, object: notes}`, the named-object pattern of
that design's Phase 2). Missing are the Python protocol, the Rust adapter, the registry and the
`python` entry type, and the `RECIPE` row of `liquers-py/README.md`.

## Problem Example

```python
import liquers as lq

class Notes:                                   # the shape a Python provider would take
    def recipe_opt(self, key): ...

env = lq.Environment(config={"recipes": ["recipes_yaml", {"type": "python", "object": "notes"}]})
env.recipe_provider("notes", Notes())          # no such method; "python" is an unknown entry type
```

## Notes

Planned as part of liquers-py M4 (STORE/RECIPE) in `/mnt/project-files/plans/liquers-py-implementation-plan.md`.
Every member of a chain is asked for every key, so the adapter must answer "no recipe" cheaply and
fail only on a substantial error; a Python exception becomes a failed lookup for every key.
