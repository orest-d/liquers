"""UIUSE: a Liquers UI model held by a Python environment (specs/design/liquers-py-ui/).

The model runs without a window or a display: elements evaluate in the background, and status
callbacks run on the thread that waits (`wait_idle`, `wait_idle_async`, `process_events`). The
window half is `test_UIUSE_window.py`.
"""

import asyncio
import gc
import threading
import time
import weakref

import pytest

import liquers as lq


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


def statuses_of(events, handle):
    return [str(e.status) for e in events if e.handle == handle]


# AC-1 (the problem example)
def test_UIUSE_show_a_query_without_a_window(ui):
    seen = []
    with ui.on("status", lambda e: seen.append((e.handle, str(e.status)))):
        handle = ui.show("report")
        assert ui.wait_idle(timeout=10)
    assert seen == [(handle, "Processing"), (handle, "Ready")]
    [info] = ui.elements()
    assert (info.handle, info.parent, info.status) == (handle, None, "Ready")
    assert info.type_name == "AssetViewElement"
    assert ui.asset(handle).get() == "rows: 3"


# AC-2
def test_UIUSE02_a_ui_value_reaches_python(env):
    element = env.evaluate("hello/q/ns-lui/query_console")
    assert isinstance(element, lq.UIElement)
    assert element.type_name == "QueryConsoleElement"
    assert element.title == "Query Console"
    with pytest.raises(lq.LiquersError):
        env.store_set("ui/console", element)


# AC-2: the element crosses back as the same element
def test_UIUSE02_a_ui_value_crosses_back(env, ui):
    @env.command
    def console() -> lq.UIElement:
        return env.evaluate("hello/q/ns-lui/query_console")

    handle = ui.show("console")
    assert ui.wait_idle(timeout=10)
    [info] = ui.elements()
    assert (info.handle, info.type_name) == (handle, "QueryConsoleElement")


# AC-3
def test_UIUSE03_an_action_reaches_the_owning_element(env, ui):
    calls = []

    @env.command
    def act(state, context) -> str:
        calls.append(context.element)
        return "done"

    handle = ui.show("report")
    ui.submit("act", handle)
    assert ui.wait_idle(timeout=10)
    assert calls == [handle]


def test_UIUSE03_context_element_is_none_outside_a_ui(env):
    @env.command
    def where(state, context) -> str:
        return repr(context.element)

    assert env.evaluate("where") == "None"


# AC-4: in order, on the waiting thread
def test_UIUSE04_statuses_arrive_in_order_on_the_waiting_thread(ui):
    seen = []

    def record(event):
        seen.append((event.handle, str(event.status), threading.current_thread()))

    worker_seen = []

    def worker():
        with ui.on("status", record):
            first = ui.show("report")
            second = ui.show("summary")
            assert ui.wait_idle(timeout=10)
            worker_seen.extend([first, second])

    thread = threading.Thread(target=worker)
    thread.start()
    thread.join(timeout=30)
    first, second = worker_seen
    assert {t for (_, _, t) in seen} == {thread}
    events = [(h, s) for (h, s, _) in seen]
    assert [s for h, s in events if h == first] == ["Processing", "Ready"]
    assert [s for h, s in events if h == second] == ["Processing", "Ready"]


def test_UIUSE04_process_events_delivers_on_the_caller(ui):
    seen = []
    with ui.on("status", lambda e: seen.append(threading.current_thread())):
        ui.show("report")
        deadline = time.monotonic() + 10
        delivered = 0
        while delivered < 2 and time.monotonic() < deadline:
            delivered += ui.process_events()
            time.sleep(0.02)
    assert delivered == 2
    assert seen == [threading.main_thread()] * 2


def test_UIUSE04_wait_idle_async_delivers_on_the_event_loop(ui):
    seen = []

    async def main():
        loop_thread = threading.current_thread()
        with ui.on("status", lambda e: seen.append(threading.current_thread() is loop_thread)):
            ui.show("report")
            return await ui.wait_idle_async(timeout=10)

    assert asyncio.run(main()) is True
    assert seen == [True, True]


# AC-5
def test_UIUSE05_disposing_a_subscription_releases_its_callback(ui):
    class Callback:
        def __init__(self):
            self.events = []

        def __call__(self, event):
            self.events.append(event)

    callback = Callback()
    ref = weakref.ref(callback)
    subscription = ui.on("status", callback)
    ui.show("report")
    assert ui.wait_idle(timeout=10)
    assert len(callback.events) == 2
    subscription.dispose()
    del callback
    gc.collect()
    assert ref() is None
    ui.show("summary")
    assert ui.wait_idle(timeout=10)


def test_UIUSE05_dispose_ends_the_model(env):
    ui = env.ui()
    called = []
    ui.on("status", called.append)
    ui.show("report")
    ui.dispose()
    ui.dispose()  # idempotent
    assert ui.disposed
    assert called == []
    for call in (lambda: ui.show("report"), lambda: ui.wait_idle(1), ui.elements):
        with pytest.raises(lq.LiquersError, match="disposed"):
            call()


def test_UIUSE05_a_with_block_disposes(env):
    with env.ui() as ui:
        ui.show("report")
    assert ui.disposed


def test_UIUSE05_shutdown_disposes_the_environments_models(env):
    ui = env.ui()
    env.shutdown()
    assert ui.disposed
    with pytest.raises(lq.LiquersError):
        env.ui()


# AC-6
def test_UIUSE06_a_stale_result_is_ignored(env, ui):
    started, release, finished = threading.Event(), threading.Event(), threading.Event()

    @env.command
    def slow() -> str:
        started.set()
        release.wait(10)
        finished.set()
        return "A"

    @env.command
    def fast() -> str:
        return "B"

    seen = []
    with ui.on("status", seen.append):
        handle = ui.show("slow")
        assert started.wait(10)
        assert ui.show("fast", handle) == handle
        release.set()
        assert finished.wait(10)
        assert ui.wait_idle(timeout=10)
        time.sleep(0.3)  # a late A would be recorded by a tick after it finished
        assert ui.wait_idle(timeout=10)
    assert statuses_of(seen, handle) == ["Processing", "Processing", "Ready"]
    assert ui.asset(handle).get() == "B"


# AC-7
def test_UIUSE07_an_unparsable_query_raises(ui):
    with pytest.raises(lq.ParseError):
        ui.show("report/(")
    with pytest.raises(lq.ParseError):
        ui.submit("report/(")


def test_UIUSE07_a_failing_query_is_an_error_event(ui):
    seen = []
    with ui.on("status", seen.append):
        handle = ui.show("-R/nothing/here.txt")
        assert ui.wait_idle(timeout=10)
    assert statuses_of(seen, handle) == ["Processing", "Error"]
    assert isinstance(seen[-1].error, lq.LiquersError)
    [info] = ui.elements()
    assert info.status == "Error"


def test_UIUSE07_unknown_handle_and_kind(ui):
    with pytest.raises(lq.LiquersError, match="no UI element 99"):
        ui.show("report", 99)
    with pytest.raises(lq.LiquersError, match="no UI element 99"):
        ui.submit("report", 99)
    with pytest.raises(ValueError):
        ui.on("click", print)


def test_UIUSE07_a_raising_callback_is_reported_and_delivery_goes_on(ui, monkeypatch):
    reported, seen = [], []
    monkeypatch.setattr("sys.unraisablehook", lambda u: reported.append(u.exc_value))

    def broken(event):
        raise RuntimeError("callback failed")

    with ui.on("status", broken), ui.on("status", seen.append):
        ui.show("report")
        assert ui.wait_idle(timeout=10)
    assert len(seen) == 2
    assert [str(e) for e in reported] == ["callback failed"] * 2


# AC-8
def test_UIUSE_a_callback_may_call_back_into_the_ui(ui):
    shown = []

    def on_ready(event):
        if str(event.status) == "Ready" and not shown:
            shown.append(ui.show("summary"))

    with ui.on("status", on_ready):
        ui.show("report")
        assert ui.wait_idle(timeout=10)
    assert len(ui.elements()) == 2


# AC-9
def test_UIUSE_the_ui_follows_late_registrations(env, ui):
    @env.command
    def late() -> str:
        return "late"

    handle = ui.show("late")
    assert ui.wait_idle(timeout=10)
    assert ui.asset(handle).get() == "late"
    assert ui.elements()[0].status == "Ready"


# Waits that could never return raise; a timeout returns False
def test_UIUSE_wait_idle_inside_an_own_action_raises(env, ui):
    errors = []

    @env.command
    def waits() -> str:
        try:
            ui.wait_idle(timeout=1)
        except lq.LiquersError as error:
            errors.append(str(error))
        return "x"

    ui.submit("waits")
    assert ui.wait_idle(timeout=10)
    assert len(errors) == 1 and "own action" in errors[0]


def test_UIUSE_wait_idle_times_out(env, ui):
    release = threading.Event()

    @env.command
    def blocked() -> str:
        release.wait(10)
        return "done"

    ui.show("blocked")
    try:
        assert ui.wait_idle(timeout=0.3) is False
    finally:
        release.set()
    assert ui.wait_idle(timeout=10)


def test_UIUSE_default_environment_ui():
    ui = lq.ui()
    try:
        assert isinstance(ui, lq.UI)
    finally:
        ui.dispose()


def test_UIUSE_wait_idle_inside_an_elements_command_raises(env, ui):
    errors = []

    @env.command
    def waits_too() -> str:
        try:
            ui.wait_idle(timeout=None)
        except lq.LiquersError as error:
            errors.append(str(error))
        return "x"

    handle = ui.show("waits_too")
    assert ui.wait_idle(timeout=10)
    assert len(errors) == 1 and "inside a command" in errors[0]
    assert ui.asset(handle).get() == "x"


def test_UIUSE_wait_idle_async_inside_an_async_command_raises(env, ui):
    errors = []

    @env.command
    async def awaits_too() -> str:
        try:
            await ui.wait_idle_async(timeout=None)
        except lq.LiquersError as error:
            errors.append(str(error))
        return "x"

    handle = ui.show("awaits_too")
    assert ui.wait_idle(timeout=10)
    assert len(errors) == 1 and "inside a command" in errors[0]
    assert ui.asset(handle).get() == "x"


def test_UIUSE_wait_idle_async_outside_a_command_is_not_marked(ui):
    async def main():
        ui.show("report")
        return await ui.wait_idle_async(timeout=10)

    assert asyncio.run(main())


def test_UIUSE05_on_after_dispose_raises(env):
    ui = env.ui()
    ui.dispose()
    with pytest.raises(lq.LiquersError, match="disposed"):
        ui.on("status", print)


def test_UIUSE04_a_callback_of_one_ui_may_wait_for_another(env):
    first, second = env.ui(), env.ui()
    try:
        inner = []

        def on_first(event):
            if str(event.status) == "Ready" and not inner:
                with second.on("status", lambda e: inner.append(str(e.status))):
                    second.show("summary")
                    assert second.wait_idle(timeout=10)

        with first.on("status", on_first):
            first.show("report")
            assert first.wait_idle(timeout=10)
        assert inner == ["Processing", "Ready"]
    finally:
        first.dispose()
        second.dispose()
