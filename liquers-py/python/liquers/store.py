"""Stores: where Liquers reads and writes data by key.

Rust counterpart: `liquers_core::store`. Every name here is also available directly from `liquers`
(`liquers.Store is liquers.store.Store`); `import liquers as lq` and `lq.Store` stays
the usual style.

# Main components
- `Store`: a handle on a store, for reading, writing and listing keys.
- `StoreArgumentInfo`: one configuration argument of a store type.

# Where it is used
- `liquers.environment.Environment.store` is the environment's `Store`.

# Related concepts
- A store written in Python follows the protocol in the Python guide; its keys are `liquers.query.Key`.

# Examples
>>> import liquers as lq
>>> from liquers.store import Store
>>> env = lq.Environment()
>>> isinstance(env.store, Store)
True
>>> env.shutdown()
"""

from liquers._backend import (
    Store,
    StoreArgumentInfo,
)

__all__ = [
    "Store",
    "StoreArgumentInfo",
]
