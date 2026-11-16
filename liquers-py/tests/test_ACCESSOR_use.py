"""ACCESSOR: using existing settable value accessors from Python (`lq.Accessor`).

Design `specs/design/settable-value-accessor/` (Phase 3 test 7); the IDs are the `ACCESSOR` feature
of `LANGUAGE-INTEGRATION_GUIDE.md`, direction 1 (use). Defining a root or a step in Python is out
of scope (`ACCESSOR-CANNOT-BE-IMPLEMENTED-IN-PYTHON-OR-JAVASCRIPT`).
"""

import asyncio

import pytest

import liquers as lq

TITLE = "-R-key/config.yaml/-/ns-acc/accessor/field-title"


@pytest.fixture
def env():
    environment = lq.Environment()
    environment.store_set("config.yaml", "title: Dashboard\nrows: 3\n")
    yield environment
    environment.shutdown()


def stored(env, key):
    return env.evaluate(f"-R/{key}")


def wait_for(acc, kind, limit=20):
    """Waits for an event of `kind`, returning every kind seen on the way."""
    kinds = []
    for _ in range(limit):
        event = acc.wait_event(10)
        assert event is not None, f"no {kind} event; saw {kinds}"
        kinds.append(event.kind)
        if event.kind == kind:
            return kinds
    raise AssertionError(f"no {kind} event in {limit}; saw {kinds}")


def test_ACCESSOR01_get_and_set_with_a_token(env):
    acc = env.accessor(TITLE)
    assert isinstance(acc, lq.Accessor)
    assert acc.capabilities.write
    snap = acc.snapshot()
    assert snap.value == "Dashboard"
    assert isinstance(snap.token, str) and snap.token
    new = acc.set_if("Sales", snap.token)
    assert new != snap.token
    assert acc.get() == "Sales"
    assert stored(env, "config.yaml") == "title: Sales\nrows: 3\n"
    # `set` uses the token this wrapper last saw.
    third = acc.set("Report")
    assert third != new
    assert acc.snapshot().token == third


def test_ACCESSOR02_project_and_describe(env):
    root = env.accessor("-R-key/config.yaml/-/ns-acc/accessor")
    title = root.project("field-title")
    assert title.address.encode() == TITLE
    assert title.get() == "Dashboard"
    part = title.describe()
    assert isinstance(part, lq.PartDescription)
    assert part.type_identifier == "Text"
    assert part.read_only is False
    # A query that is not an accessor is a read-only accessor of its asset.
    plain = env.accessor("-R/config.yaml")
    assert not plain.capabilities.write
    assert plain.describe().read_only


def test_ACCESSOR03_events_reach_callbacks(env):
    acc = env.accessor(TITLE)
    token = acc.snapshot().token
    seen = []
    with acc.on(lambda e: seen.append(e.kind)):
        new = acc.set_if("Sales", token)
        kinds = wait_for(acc, "changed")
        assert "changed" in seen and seen == kinds
    changed = [k for k in seen if k == "changed"]
    assert changed
    # Disposed: the callback no longer runs.
    count = len(seen)
    acc.set_if("Again", new)
    acc.wait_event(10)
    acc.process_events()
    assert len(seen) == count


def test_ACCESSOR03b_events_async(env):
    acc = env.accessor(TITLE)

    async def main():
        token = (await acc.snapshot_async()).token
        events = acc.events()
        first = asyncio.ensure_future(events.__anext__())
        await asyncio.sleep(0)
        await acc.set_if_async("Sales", token)
        kinds = []
        event = await asyncio.wait_for(first, 10)
        kinds.append(event.kind)
        while event.kind != "changed":
            event = await asyncio.wait_for(events.__anext__(), 10)
            kinds.append(event.kind)
        await events.aclose()
        return kinds, await acc.get_async()

    kinds, value = asyncio.run(main())
    assert kinds[-1] == "changed"
    assert value == "Sales"


def test_ACCESSOR04_conflict_is_status_conflict(env):
    acc = env.accessor(TITLE)
    old = acc.snapshot().token
    acc.set_if("Sales", old)
    with pytest.raises(lq.StatusConflictError) as caught:
        acc.set_if("Stale", old)
    assert caught.value.token == acc.info().token
    assert stored(env, "config.yaml") == "title: Sales\nrows: 3\n"
    # Creating a value that exists conflicts too.
    with pytest.raises(lq.StatusConflictError):
        acc.set_if("New", None)
    # A conflict from elsewhere has no token.
    assert lq.StatusConflictError("conflict").token is None


def test_ACCESSOR05_memory_accessor_is_shared(env):
    counter = env.memory_accessor(1, address="counter")
    assert counter.get() == 1
    assert counter.address.encode() == "counter"

    @env.command
    def shared():
        return counter

    other = env.evaluate("shared")
    assert isinstance(other, lq.Accessor) and other is not counter
    other.set(2)
    assert counter.get() == 2
    # An empty variable: get fails, a create sets it.
    empty = env.memory_accessor()
    with pytest.raises(lq.LiquersError):
        empty.get()
    empty.set_if("first", None)
    assert empty.get() == "first"


def test_ACCESSOR06_accessor_crosses_back_unchanged(env):
    acc = env.accessor(TITLE)
    holder = {}

    @env.command
    def give():
        return acc

    @env.command
    def address_of(state):
        holder["crossed"] = state.value
        return state.value.address.encode()

    assert env.evaluate("give/address_of") == TITLE
    crossed = holder["crossed"]
    # The same Rust accessor: a write through one wrapper is seen by the other, and each wrapper
    # keeps its own token, so the first holder now conflicts.
    first = acc.snapshot().token
    crossed.set_if("Sales", crossed.snapshot().token)
    assert acc.get() == "Sales"
    with pytest.raises(lq.StatusConflictError):
        acc.set_if("Stale", first)


def test_ACCESSOR07_info_and_log(env):
    acc = env.accessor(TITLE)
    snap = acc.snapshot()
    info = acc.info()
    assert isinstance(info, lq.AccessorInfo)
    assert info.token == snap.token
    assert info.status == "Source"  # a stored file's accessor
    assert info.log_len >= 0
    assert isinstance(acc.log(), list)
    assert acc.log(since=info.log_len) == []
    missing = env.accessor("-R-key/none.txt/-/ns-acc/store")
    assert missing.info().status == "None"
    assert missing.info().token is None
