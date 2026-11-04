"""DATANODE: the outbound view, lazy State and async views (AC-11, AC-12)."""

import asyncio
import subprocess
import sys
import textwrap
import threading

import pytest
from conftest import run_async

import liquers as lq


def test_state_value_is_lazy():
    """`evaluate_state` converts nothing; the first read of `.value` converts."""
    pytest.importorskip("polars")
    code = textwrap.dedent(
        """
        import sys
        import liquers as lq
        env = lq.Environment()
        env.store_set("data/sales.csv", b"region,amount\\nEU,10\\n")
        state = env.evaluate_state("-R/data/sales.csv/-/ns-pl/from_csv")
        assert "polars" not in sys.modules
        frame = state.value
        assert "polars" in sys.modules and frame.height == 1
        assert state.value is frame
        env.shutdown()
        """
    )
    result = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, timeout=120)
    assert result.returncode == 0, result.stderr


def test_frame_cell_without_conversion():
    """AC-11: a polars frame state is read cell by cell without importing polars."""
    pytest.importorskip("polars")
    code = textwrap.dedent(
        """
        import sys
        import liquers as lq
        env = lq.Environment()
        env.store_set("data/sales.csv", b"region,amount\\nEU,10\\nUS,20\\n")
        state = env.evaluate_state("-R/data/sales.csv/-/ns-pl/from_csv")
        assert lq.data_node(state)[0]["amount"].scalar == 10, lq.data_node(state)[0]["amount"].scalar
        assert "polars" not in sys.modules
        env.shutdown()
        """
    )
    result = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, timeout=120)
    assert result.returncode == 0, result.stderr


def _folder_environment(env):
    """`data/` holds `sales.csv` and a recipe for `slow.txt`, whose command waits for `release`
    and signals `started` when it begins."""
    started, release = threading.Event(), threading.Event()

    @env.first_command
    def held() -> str:
        started.set()
        release.wait(10)
        return "slow"

    env.store_set("data/sales.csv", b"region,amount\nEU,10\n")
    env.store_set("data/recipes.yaml", "recipes:\n  - query: held/slow.txt\n")
    return started, release


def test_async_nodes_at_top_level(env):
    """AC-12: a folder read at the top level is read with `_async`, each read in its own asset."""
    _folder_environment(env)[1].set()
    folder = env.data_node(env.evaluate_state("-R-dir/data"))
    assert folder.kind == lq.NodeKind.Map
    with pytest.raises(lq.NotSupportedError):
        folder.get("sales.csv")

    async def main():
        entry = await folder.get_async("sales.csv")
        names = await folder.to_json_async()
        slow = await folder.get_async("slow.txt")
        missing = await folder.get_async("missing")
        return entry, names, await slow.scalar_async(), missing

    entry, names, slow, missing = run_async(main)
    assert entry.kind == lq.data_node(env.evaluate_state("-R/data/sales.csv")).kind
    assert names == ["recipes.yaml", "sales.csv", "slow.txt"]
    assert slow == "slow"
    assert missing is None


def test_async_nodes_in_a_command(env):
    """AC-12: inside a command, `context.data_node(state)` reads through the command's context."""
    _folder_environment(env)[1].set()

    @env.command
    async def folder_keys(state, context):
        folder = context.data_node(state)
        slow = await folder.get_async("slow.txt")
        return ",".join(await folder.to_json_async()) + ":" + await slow.scalar_async()

    assert env.evaluate("-R-dir/data/-/folder_keys") == "recipes.yaml,sales.csv,slow.txt:slow"


def test_command_view_expires(env):
    """A command view read after the command returned raises GeneralError."""
    _folder_environment(env)[1].set()
    kept = []

    @env.command
    def keep(state, context):
        kept.append(context.data_node(state))
        return "kept"

    assert env.evaluate("-R-dir/data/-/keep") == "kept"
    with pytest.raises(lq.GeneralError):
        kept[0].get_async("sales.csv")


def test_env_view_after_shutdown():
    """An `Env` view read after `shutdown()` raises GeneralError and starts nothing."""
    env = lq.Environment()
    env.store_set("data/sales.csv", b"region,amount\nEU,10\n")
    folder = env.data_node(env.evaluate_state("-R-dir/data"))
    env.shutdown()
    with pytest.raises(lq.GeneralError):
        folder.get_async("sales.csv")


def test_env_read_cancelled_at_shutdown():
    """`shutdown()` while an `Env` read waits on a slow step: the read raises CancelledError."""
    env = lq.Environment()
    started, release = _folder_environment(env)
    folder = env.data_node(env.evaluate_state("-R-dir/data"))

    async def main():
        read = asyncio.ensure_future(folder.get_async("slow.txt"))
        await asyncio.get_running_loop().run_in_executor(None, started.wait, 10)
        await asyncio.get_running_loop().run_in_executor(None, env.shutdown)
        with pytest.raises(lq.CancelledError):
            await read

    try:
        run_async(main)
    finally:
        release.set()


def test_cancelled_await_cancels_the_read(env):
    """Cancelling the task awaiting `get_async` ends it with asyncio.CancelledError; the read's own
    asset is cancelled, the shared step it waited for is not, and a later read still works."""
    started, release = _folder_environment(env)
    folder = env.data_node(env.evaluate_state("-R-dir/data"))

    async def main():
        read = asyncio.ensure_future(folder.get_async("slow.txt"))
        await asyncio.get_running_loop().run_in_executor(None, started.wait, 10)
        read.cancel()
        with pytest.raises(asyncio.CancelledError):
            await read
        release.set()
        again = await folder.get_async("slow.txt")
        return await again.scalar_async()

    try:
        assert run_async(main) == "slow"
    finally:
        release.set()
