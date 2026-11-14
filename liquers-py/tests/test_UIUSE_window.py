"""UIUSE: the egui window of a Liquers UI (specs/design/liquers-py-ui-window/).

`UI.run()` blocks the main thread while the window shows; these tests close it from a helper
thread. They are marked `gui` and skipped without a display; CI runs them under `xvfb-run`.
The model without a window is `test_UIUSE_model.py`.
"""

import _thread
import threading
import time
import urllib.request

import pytest

import liquers as lq

pytestmark = pytest.mark.gui


@pytest.fixture
def env():
    environment = lq.Environment()

    @environment.command
    def report() -> str:
        return "rows: 3"

    @environment.command
    def summary() -> str:
        return "3 rows"

    yield environment
    environment.shutdown()


@pytest.fixture
def ui(env):
    model = env.ui()
    yield model
    model.dispose()


def when_open(ui, action, timeout=20):
    """Runs `action` on a helper thread once `ui`'s window shows (or after `timeout`)."""

    def wait_then_act():
        deadline = time.monotonic() + timeout
        while not ui.window_open and time.monotonic() < deadline:
            time.sleep(0.02)
        time.sleep(0.3)  # a few frames
        action()

    thread = threading.Thread(target=wait_then_act, daemon=True)
    thread.start()
    return thread


# AC-1 (the problem example)
def test_UIUSE_WINDOW01_run_shows_the_model_and_close_ends_it(ui):
    handle = ui.show("report")
    assert ui.wait_idle(timeout=10)
    helper = when_open(ui, ui.close)
    ui.run(title="Report")
    helper.join(timeout=10)
    assert not ui.window_open
    assert ui.asset(handle).get() == "rows: 3"


# AC-1: the model keeps running after the window closed, and a window opens again
def test_UIUSE_WINDOW01_the_model_runs_on_after_the_window(ui):
    when_open(ui, ui.close)
    ui.run()
    handle = ui.show("summary")
    assert ui.wait_idle(timeout=10)
    assert ui.asset(handle).get() == "3 rows"
    when_open(ui, ui.close)
    ui.run()


# AC-2
def test_UIUSE_WINDOW02_run_off_the_main_thread_raises(ui):
    errors = []

    def worker():
        try:
            ui.run()
        except lq.LiquersError as error:
            errors.append(str(error))

    thread = threading.Thread(target=worker)
    thread.start()
    thread.join(timeout=10)
    assert len(errors) == 1 and "main thread" in errors[0]
    assert not ui.window_open


# AC-2
def test_UIUSE_WINDOW02_run_after_dispose_raises(env):
    ui = env.ui()
    ui.dispose()
    with pytest.raises(lq.LiquersError, match="disposed"):
        ui.run()


# AC-2: dispose from another thread closes the window
def test_UIUSE_WINDOW02_dispose_from_another_thread_closes(ui):
    when_open(ui, ui.dispose)
    ui.run()
    assert ui.disposed


# AC-3
def test_UIUSE_WINDOW03_callbacks_run_on_the_main_thread(ui):
    seen = []
    with ui.on("status", lambda e: seen.append((str(e.status), threading.current_thread()))):

        def show_then_close():
            ui.show("report")
            deadline = time.monotonic() + 10
            while len(seen) < 2 and time.monotonic() < deadline:
                time.sleep(0.02)
            ui.close()

        when_open(ui, show_then_close)
        ui.run()
    assert [s for s, _ in seen] == ["Processing", "Ready"]
    assert {t for _, t in seen} == {threading.main_thread()}


# AC-3: wait_idle from a callback on the window's thread could never return
def test_UIUSE_WINDOW03_wait_idle_on_the_window_thread_raises(ui):
    errors = []

    def on_status(event):
        if str(event.status) == "Ready":
            try:
                ui.wait_idle(timeout=1)
            except lq.LiquersError as error:
                errors.append(str(error))
            ui.close()

    with ui.on("status", on_status):
        when_open(ui, lambda: ui.show("report"))
        ui.run()
    assert len(errors) == 1 and "window" in errors[0]


# AC-3: another thread waits without delivering while the window shows
def test_UIUSE_WINDOW03_wait_idle_elsewhere_waits_without_delivering(ui):
    seen, waited = [], []

    def wait_then_close():
        ui.show("report")
        waited.append(ui.wait_idle(timeout=10))
        ui.close()

    with ui.on("status", lambda e: seen.append(threading.current_thread())):
        when_open(ui, wait_then_close)
        ui.run()
    assert waited == [True]
    assert set(seen) <= {threading.main_thread()}
    assert len(seen) == 2


# AC-4
def test_UIUSE_WINDOW04_ctrl_c_closes_the_window_and_raises(ui):
    when_open(ui, _thread.interrupt_main)
    with pytest.raises(KeyboardInterrupt):
        ui.run()
    assert not ui.window_open
    handle = ui.show("summary")
    assert ui.wait_idle(timeout=10)
    assert ui.asset(handle).get() == "3 rows"


# AC-5
def test_UIUSE_WINDOW05_a_background_server_beside_the_window(env, ui):
    calls = []

    @env.command
    def shared() -> str:
        calls.append(1)
        return "shared"

    responses = []
    with env.serve(port=0) as server:

        def request_then_close():
            ui.show("shared")
            assert ui.wait_idle(timeout=10)
            with urllib.request.urlopen(f"{server.url}/liquer/q/shared", timeout=10) as response:
                responses.append(response.read())
            ui.close()

        when_open(ui, request_then_close)
        ui.run()
    assert responses == [b"shared"]
    assert len(calls) == 1, "the server finds the asset the window computed"
