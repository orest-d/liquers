"""RUNTIME: threads, nesting and containment (LANGUAGE-INTEGRATION_GUIDE.md)."""

import threading

import pytest

import liquers as lq
from conftest import na


def test_RUNTIME02_wasm_accepts_non_send_callback():
    pass


test_RUNTIME02_wasm_accepts_non_send_callback = na("no Pyodide target")(
    test_RUNTIME02_wasm_accepts_non_send_callback
)


def test_RUNTIME04_nested_evaluation_does_not_deadlock(env):
    @env.first_command
    def deeper(context):
        return "deeper:" + context.evaluate("outer")

    assert env.evaluate("outer") == "outer:hello"
    assert env.evaluate("deeper") == "deeper:outer:hello"


def test_RUNTIME_two_threads_evaluate_concurrently(env):
    barrier = threading.Barrier(2, timeout=10)

    @env.first_command(volatile=True)
    def meet(n: int = 0):
        barrier.wait()  # both commands must be running at once to pass
        return n

    results = {}

    def run(n):
        results[n] = env.evaluate(f"meet-{n}")

    threads = [threading.Thread(target=run, args=(n,)) for n in (1, 2)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join(timeout=20)
    assert results == {1: 1, 2: 2}


def test_RUNTIME05_cancellation_and_shutdown_release_handles():
    """What is released today: the callables of an environment that was never built."""
    assert hasattr(lq.Environment, "live_handle_count"), "build with --features debug-handles"
    baseline = lq.Environment.live_handle_count()
    environment = lq.Environment()

    @environment.first_command
    def kept():
        return 1

    @environment.first_command
    def dropped():
        return 2

    assert lq.Environment.live_handle_count() == baseline + 2
    assert environment.unregister("dropped")
    assert lq.Environment.live_handle_count() == baseline + 1
    environment.shutdown()
    assert lq.Environment.live_handle_count() == baseline


@pytest.mark.xfail(
    strict=True,
    reason="BLOCKED: ENVIRONMENT-MANAGER-REFERENCE-CYCLE: a built environment and its asset "
    "manager hold each other with strong Arcs, so its registry and callables are never freed",
)
def test_RUNTIME05_built_environment_releases_handles_on_shutdown():
    baseline = lq.Environment.live_handle_count()
    environment = lq.Environment()

    @environment.first_command
    def kept():
        return 1

    assert environment.evaluate("kept") == 1
    environment.shutdown()
    assert lq.Environment.live_handle_count() == baseline
