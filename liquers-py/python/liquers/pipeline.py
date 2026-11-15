"""Pipelines: a query as structured data, built and edited step by step.

Rust counterpart: `liquers_core::pipeline`. Every name here is also available directly from `liquers`
(`liquers.Pipeline is liquers.pipeline.Pipeline`); `import liquers as lq` and `lq.Pipeline` stays
the usual style.

# Main components
- `Pipeline`: a query as a list of elements, converted to and from query text.
- `PipelineEditor`, `QueryEditor`, `ActionEditor`: validating editors.

# Where it is used
- `liquers.environment.Environment.evaluate` accepts a `Pipeline`.

# Related concepts
- A pipeline and a query text describe the same value; `Pipeline.to_query` converts.

# Examples
>>> from liquers.pipeline import Pipeline
>>> isinstance(Pipeline(["hello/to_text"]), Pipeline)
True
"""

from liquers._backend import (
    Pipeline,
    PipelineEditor,
    QueryEditor,
    ActionEditor,
)

__all__ = [
    "Pipeline",
    "PipelineEditor",
    "QueryEditor",
    "ActionEditor",
]
