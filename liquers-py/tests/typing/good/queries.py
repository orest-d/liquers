"""Typed use of the query classes."""

from typing import assert_type

import liquers as lq

q = lq.parse("-R/data/x.csv/-/to_text")
assert_type(q, lq.Query)
assert_type(q.encode(), str)
assert_type(q.filename(), str | None)
key = lq.parse_key("data/x.csv")
assert_type(key, lq.Key)
assert_type(key[0], lq.ResourceName)
assert_type(len(key), int)
assert_type(key.parent(), lq.Key)
assert_type(key < lq.Key("data/y.csv"), bool)
assert_type(q.resource_query(), lq.ResourceQuerySegment | None)
action = q.action()
if action is not None:
    assert_type(action.name, str)
assert_type(lq.Query(), lq.Query)
