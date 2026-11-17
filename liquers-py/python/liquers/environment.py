"""The environment: the store, commands, recipes and assets that queries run against, and the context a command runs in.

Rust counterpart: `liquers_core::context` and `liquers_core::environment_builder`. Every name here is also available directly from `liquers`
(`liquers.Environment is liquers.environment.Environment`); `import liquers as lq` and `lq.Environment` stays
the usual style.

# Main components
- `Environment`: configured once, then used to evaluate queries.
- `default_environment()`, `evaluate`, `evaluate_async`: the process-wide default environment and its shortcuts.
- `Context`: what a running command sees of its evaluation.
- `Issue`: a problem found while the environment was built.
- `require(extra)`: checks that an optional extra is installed.

# Where it is used
- Every query runs in an environment; `liquers.evaluate` uses the default one.

# Related concepts
- A *configuration document* (YAML or JSON) can configure the environment, its store and its server.

# Examples
>>> import liquers as lq
>>> from liquers.environment import Environment
>>> env = Environment()
>>> @env.first_command
... def hello() -> str:
...     return "hello"
>>> env.evaluate("hello")
'hello'
>>> env.shutdown()
"""

from liquers._backend import (
    Environment,
    Context,
    Issue,
)
from liquers._env import (
    default_environment,
    evaluate,
    evaluate_async,
)
from liquers._extras import require

__all__ = [
    "Environment",
    "Context",
    "Issue",
    "default_environment",
    "evaluate",
    "evaluate_async",
    "require",
]
