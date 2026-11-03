"""The awaitable API."""

from typing import Any, assert_type

import liquers as lq


async def main(env: lq.Environment, source: lq.RecordSource) -> None:
    q = lq.parse("hello")
    assert_type(await env.evaluate_async(q), Any)
    assert_type(await lq.evaluate_async("hello"), Any)
    assert_type(await env.evaluate_state_async("hello"), lq.State)
    asset = await env.get_asset_async("hello")
    assert_type(await asset.get_async(), Any)
    assert_type(await asset.get_state_async(), lq.State)
    k = lq.parse_key("a.txt")
    assert_type(await env.store.get_async(k), bytes)
    assert_type(await env.store.listdir_async(""), list[str])
    assert_type(await env.recipes.contains_async(k), bool)
    assert_type(await source.materialize_async(10), lq.RecordView)
    assert_type(await source.chunk_async(0), lq.RecordView)
    await asset.cancel_async()
