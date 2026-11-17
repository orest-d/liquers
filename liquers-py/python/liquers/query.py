"""The query language: parsed queries, their segments and actions, keys, and links.

Rust counterpart: `liquers_core::query` and `liquers_core::parse`. Every name here is also available directly from `liquers`
(`liquers.Query is liquers.query.Query`); `import liquers as lq` and `lq.Query` stays
the usual style.

# Main components
- `Query`, from `parse(text)`: a parsed query, inspected and re-encoded without running it.
- `Key`, from `parse_key(text)`: a store key, the path part of a resource query.
- `ActionRequest`, `ActionParameter`: one action of a transform segment and its parameters.
- `Link` and `link(query)`: a query given as a command argument default (a default link).

# Where it is used
- `liquers.environment.Environment.evaluate` takes a query as text or as a `Query`.
- `liquers.plan.build_plan` turns a `Query` into a `Plan`.

# Related concepts
- A *query* names a value: a resource segment (`-R/…`) reads a key, a transform segment applies actions.
- Query text is the same in Rust, Python and JavaScript; the syntax is in the guide appendix.

# Examples
>>> from liquers.query import parse, parse_key
>>> parse("hello/to_text").encode()
'hello/to_text'
>>> parse_key("data/x.csv").encode()
'data/x.csv'
"""

from liquers._backend import (
    Query,
    Key,
    ActionRequest,
    ActionParameter,
    ResourceName,
    Position,
    SegmentHeader,
    TransformQuerySegment,
    ResourceQuerySegment,
    QuerySource,
    parse,
    parse_key,
    Link,
    link,
)

__all__ = [
    "Query",
    "Key",
    "ActionRequest",
    "ActionParameter",
    "ResourceName",
    "Position",
    "SegmentHeader",
    "TransformQuerySegment",
    "ResourceQuerySegment",
    "QuerySource",
    "parse",
    "parse_key",
    "Link",
    "link",
]
