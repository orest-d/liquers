"""Data nodes: a uniform, read-only view of nested data (maps, sequences, scalars, tables), as templates and pipelines read it.

Rust counterpart: `liquers_core::data_node`. Every name here is also available directly from `liquers`
(`liquers.data_node is liquers.data_nodes.data_node`); `import liquers as lq` and `lq.data_node` stays
the usual style.

# Main components
- `data_node(value)`: the view of a Python value or a state.
- `DataNode`: one node, indexed by key or position.
- `NodeKind`: what a node is (`Map`, `Sequence`, `Scalar`, …).

# Where it is used
- Templates read values through data nodes.

# Related concepts
- Named `data_nodes` rather than `data_node` so that the function `liquers.data_node` keeps its name.
- A class opts in with `__liquers_data__`.

# Examples
>>> from liquers.data_nodes import NodeKind, data_node
>>> data_node({"a": 1}).kind == NodeKind.Map
True
"""

from liquers._backend import (
    data_node,
    DataNode,
    NodeKind,
)

__all__ = [
    "data_node",
    "DataNode",
    "NodeKind",
]
