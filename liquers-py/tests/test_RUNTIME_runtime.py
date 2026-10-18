"""RUNTIME: threads, nesting and containment (LANGUAGE-INTEGRATION_GUIDE.md)."""

import threading

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
