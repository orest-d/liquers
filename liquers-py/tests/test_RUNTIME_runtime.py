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


def test_RUNTIME05_built_environment_releases_handles_on_shutdown():
    from conftest import register_fixture_commands

    baseline = lq.Environment.live_handle_count()
    environment = lq.Environment()
    register_fixture_commands(environment)
    running = environment.get_asset("slow-60")
    running.cancel()
    with pytest.raises(lq.CancelledError):
        running.get()
    assert running.status == "Cancelled"
    environment.shutdown()
    del running
    assert lq.Environment.live_handle_count() == baseline


def test_RUNTIME01_native_adapter_satisfies_thread_bounds(env):
    seen = {}

    @env.first_command
    def where():
        seen["thread"] = threading.get_ident()
        return "ok"

    result = {}

    def call():
        result["caller"] = threading.get_ident()
        result["v"] = env.evaluate("where")

    worker = threading.Thread(target=call)
    worker.start()
    worker.join(timeout=20)
    assert result["v"] == "ok"
    # The command ran on a Liquers blocking thread, not on the thread that called evaluate.
    assert seen["thread"] not in (result["caller"], threading.get_ident())


def test_RUNTIME03_stored_callback_outlives_registration_scope(env):
    import gc

    def register():
        captured = ["from the closure"]

        @env.first_command
        def remembered():
            return captured[0]

    register()
    gc.collect()
    assert env.evaluate("remembered") == "from the closure"


def test_RUNTIME06_panic_and_exception_containment(env):
    with pytest.raises(lq.ExecutionError):
        env.evaluate("hello/boom")
    assert hasattr(lq.Environment, "live_handle_count"), "build with --features debug-handles"
    with pytest.raises(lq.LiquersError, match="test_panic was evaluated") as raised:
        env.evaluate("test_panic")
    assert raised.value.type != lq.ErrorType.ActionNotRegistered
    assert env.evaluate("hello") == "hello"  # the process and the environment carry on
