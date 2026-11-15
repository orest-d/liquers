"""Plans: the steps a query becomes before it runs.

Rust counterpart: `liquers_core::plan`. Every name here is also available directly from `liquers`
(`liquers.Plan is liquers.plan.Plan`); `import liquers as lq` and `lq.Plan` stays
the usual style.

# Main components
- `build_plan(query, command_metadata_registry)`: the plan of a query, built without running it.
- `Plan`, `Step`: the steps in order.
- `ParameterValue`, `ResolvedParameterValues`: the resolved arguments of an action step.

# Where it is used
- `liquers.environment.Environment.evaluate` builds a plan, then runs it.

# Related concepts
- The `liquers-validate` tool shows the same plan from the command line.

# Examples
>>> from liquers.plan import Plan, build_plan
>>> Plan.__module__
'liquers.plan'
"""

from liquers._backend import (
    Plan,
    Step,
    ParameterValue,
    ResolvedParameterValues,
    build_plan,
)

__all__ = [
    "Plan",
    "Step",
    "ParameterValue",
    "ResolvedParameterValues",
    "build_plan",
]
