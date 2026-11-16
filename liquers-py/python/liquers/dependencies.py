"""Dependencies between assets: what a value was computed from, so it can expire when that changes.

Rust counterpart: `liquers_core::dependencies` (and `DependencyKey`, `DependencyRecord` of `liquers_core::metadata`). Every name here is also available directly from `liquers`
(`liquers.DependencyKey is liquers.dependencies.DependencyKey`); `import liquers as lq` and `lq.DependencyKey` stays
the usual style.

# Main components
- `DependencyKey`: what an asset can depend on (a key, a query, a command, a recipe).
- `DependencyRelation`: how it depends (state argument, parameter link, recipe, …).
- `DependencyRecord`, `PlanDependency`: one recorded dependency, with its version or relation.

# Where it is used
- `liquers.metadata.MetadataRecord` lists its dependencies.
- `liquers.plan.Plan` lists the dependencies a plan declares.

# Related concepts
- Inspecting the dependency manager itself is not available yet (`PY-CANNOT-INSPECT-THE-DEPENDENCY-MANAGER`).

# Examples
>>> from liquers.dependencies import DependencyRelation
>>> DependencyRelation.recipe().encode()
'Recipe'
"""

from liquers._backend import (
    DependencyKey,
    DependencyRecord,
    DependencyRelation,
    PlanDependency,
)

__all__ = [
    "DependencyKey",
    "DependencyRecord",
    "DependencyRelation",
    "PlanDependency",
]
