"""The public submodules: each name is typed through its module and through the package."""

from typing import Any, assert_type

import liquers
import liquers as lq
from liquers.error import KeyNotFoundError, LiquersError
from liquers.lui import UI
from liquers.query import Key, Query, parse, parse_key
from liquers.records import RecordView

assert_type(parse("hello/to_text"), Query)
assert_type(parse_key("data/x.csv"), Key)
assert_type(liquers.query.parse_key("data/x.csv"), lq.Key)
assert_type(RecordView.from_records([{"a": 1}]), lq.RecordView)
assert_type(lq.records.RecordView.from_records([{"a": 1}]).to_list(), list[dict[str, Any]])
assert_type(lq.data_nodes.data_node({"a": 1}), lq.DataNode)


def caught(error: KeyNotFoundError) -> LiquersError:
    return error


def show(ui: UI) -> lq.UI:
    return ui
