"""Typed use of environments, assets and states."""

from typing import Any, assert_type

import liquers as lq

env = lq.Environment(config={"stores": []}, conversion="strict", pickle=False, async_loops=2)
assert_type(env.liquers_version, str)
assert_type(lq.__version__, str)
lq.require("arrow")
assert_type(env.evaluate("hello"), Any)
assert_type(lq.evaluate(lq.parse("hello")), Any)
state = env.evaluate_state("hello")
assert_type(state, lq.State)
assert_type(state.value, Any)
assert_type(state.metadata, lq.Metadata)
assert_type(state.record_view(), lq.RecordView | None)
asset = env.get_asset("hello")
assert_type(asset, lq.AssetRef)
assert_type(asset.status, lq.Status)
assert_type(asset.get(), Any)
assert_type(env.validate(), list[lq.Issue])
assert_type(env.store_types(), list[dict[str, Any]])
assert_type(lq.default_environment(), lq.Environment)
env.shutdown()
