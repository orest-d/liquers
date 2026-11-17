"""Recipes: queries named by store keys, so that their results can be read like files.

Rust counterpart: `liquers_core::recipes`. Every name here is also available directly from `liquers`
(`liquers.Recipe is liquers.recipes.Recipe`); `import liquers as lq` and `lq.Recipe` stays
the usual style.

# Main components
- `Recipe`: one query with its key, title and settings.
- `RecipeList`: the recipes of one folder (`recipes.yaml`).
- `RecipeProviders`: the chain of recipe providers an environment consults.

# Where it is used
- `liquers.environment.Environment` reads recipes through its recipe providers.

# Related concepts
- A key with a recipe is computed on read; a key without one is read from the store.

# Examples
>>> from liquers.recipes import Recipe
>>> Recipe("hello/to_text").query
'hello/to_text'
"""

from liquers._backend import (
    Recipe,
    RecipeList,
    RecipeProviders,
)

__all__ = [
    "Recipe",
    "RecipeList",
    "RecipeProviders",
]
