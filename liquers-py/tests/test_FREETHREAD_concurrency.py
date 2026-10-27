"""FREETHREAD: the free-threaded audit of liquers-py (specs/design/liquers-py-async/, AC-8).

A scenario that can deadlock runs in a subprocess with a timeout: a deadlocked thread may hold the
GIL, so a thread-and-join check inside this process would hang the whole suite instead of failing.
"""

import subprocess
import sys
import textwrap


def run_isolated(script, timeout=30):
    """Runs ``script`` in a fresh interpreter; fails the test if it hangs or exits non-zero."""
    try:
        completed = subprocess.run(
            [sys.executable, "-c", textwrap.dedent(script)],
            capture_output=True,
            text=True,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        raise AssertionError(f"the scenario did not finish within {timeout} s (deadlock)")
    assert completed.returncode == 0, completed.stderr
    return completed.stdout


def test_FREETHREAD03_finalizer_reentry_does_not_deadlock():
    # Re-registering a key drops the replaced, never-built command. If that happens while the
    # environment's lock is held, the callable's __del__ re-enters the environment on the same
    # thread and waits on the lock forever.
    out = run_isolated(
        """
        import gc
        import liquers as lq

        env = lq.Environment()
        finalized = []

        class Reentrant:
            def __call__(self, state):
                return state

            def __del__(self):
                env.validate()
                finalized.append(True)

        env.command(name="reentrant")(Reentrant())
        gc.collect()

        @env.command(name="reentrant")
        def plain(state):
            return state

        gc.collect()
        env.shutdown()
        print("finalized" if finalized else "kept")
        """
    )
    assert out.strip() == "finalized"
