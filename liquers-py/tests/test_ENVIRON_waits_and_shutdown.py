"""ENVIRON: waits end on shutdown and on Ctrl-C; declarations after shutdown raise.

Regressions for the Phase 4 soundness review (specs/design/liquers-py/phase4-implementation.md).
"""

import _thread
import threading
import time

import pytest

import liquers as lq


def test_ENVIRON_wait_outstanding_at_shutdown_raises_cancelled():
    environment = lq.Environment()
    from conftest import register_fixture_commands

    register_fixture_commands(environment)
    asset = environment.get_asset("slow-5")
    timer = threading.Timer(0.3, environment.shutdown)
    timer.start()
    started = time.monotonic()
    with pytest.raises(lq.CancelledError):
        asset.get()
    timer.join()
    assert time.monotonic() - started < 3


def test_ENVIRON_ctrl_c_interrupts_a_waiting_evaluate(env):
    timer = threading.Timer(0.3, _thread.interrupt_main)
    timer.start()
    started = time.monotonic()
    with pytest.raises(KeyboardInterrupt):
        env.evaluate("slow-5")
    timer.join()
    assert time.monotonic() - started < 3


def test_ENVIRON_declarations_after_shutdown_raise():
    environment = lq.Environment()
    environment.shutdown()
    with pytest.raises(lq.GeneralError):
        environment.unregister("to_text")
    with pytest.raises(lq.GeneralError):
        environment.import_metadata("commands: []\n")
    with pytest.raises(lq.GeneralError):
        @environment.first_command
        def late():
            return "late"
