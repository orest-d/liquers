"""DATANODE: the outbound view and lazy State (AC-11)."""

import subprocess
import sys
import textwrap

import pytest

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
