"""Assets and states: a value together with its metadata, and a handle on a value being computed.

Rust counterpart: `liquers_core::state` and `liquers_core::assets`. Every name here is also available directly from `liquers`
(`liquers.State is liquers.assets.State`); `import liquers as lq` and `lq.State` stays
the usual style.

# Main components
- `State`: a value with its metadata, as `Environment.evaluate_state` returns it.
- `AssetRef`: a handle on an asset, to wait for it, read its progress or cancel it.

# Where it is used
- `liquers.environment.Environment.evaluate_state` returns a `State`.

# Related concepts
- Asset metadata is in `liquers.metadata`.

# Examples
>>> import liquers as lq
>>> from liquers.assets import State
>>> env = lq.Environment()
>>> isinstance(env.evaluate_state("hello/to_text"), State)
True
>>> env.shutdown()
"""

from liquers._backend import (
    State,
    AssetRef,
)

__all__ = [
    "State",
    "AssetRef",
]
