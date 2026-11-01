"""The Python store protocol (design liquers-py-store, AC-1 to AC-6)."""

import asyncio
import threading

import pytest

import liquers as lq
from conftest import run_async
from store_fixtures import Files, Notes, environment


def notes(prefix, greeting: str = "Hi"):
    return Notes({f"{prefix}/hello.txt": greeting.encode()})


def test_STORE_problem_example():
    # AC-1: the Phase 1 problem example.
    env = lq.Environment(config={"store": {"stores": [
        {"type": "notes", "prefix": "notes", "config": {"greeting": "Hello"}},
        {"type": "memory", "prefix": "scratch"}]}})
    env.register_store_factory("notes", notes, doc="Notes kept in a dict")
    assert env.evaluate("-R/notes/hello.txt") == "Hello"
    with pytest.raises(lq.AssetNotFoundError):
        env.evaluate("-R/notes/absent.txt")
    assert env.store_types()[-1]["store_type"] == "notes"
    assert env.store.get_metadata("notes/hello.txt").get_asset_info().media_type == "text/plain"
    with pytest.raises(lq.KeyNotSupportedError, match="'set'"):
        env.store.set("notes/new.txt", b"x")
    env.shutdown()


@pytest.mark.parametrize("source, method", [
    ("class S:\n    def listdir(self, key): return []\n", "'get'"),
    ("class S:\n    def get(self, key): return None\n    def listdir(self, key): return []\n", "'is_dir'"),
    ("class S:\n    def get(self, key): return None\n    async def is_supported(self, key): return True\n",
     "is_supported"),
    ("class S:\n    def get(self, key): return None\n    set = 3\n", "'set'"),
])
def test_STORE_protocol_resolved_once(source, method):
    # AC-2: refused at build, naming the mount and the method.
    namespace = {}
    exec(source, namespace)
    env = environment([{"type": "bad", "prefix": "b"}], bad=lambda: namespace["S"]())
    with pytest.raises(lq.ConfigurationError) as e:
        env.evaluate("-R/b/x.txt")
    assert method in str(e.value) and "'bad'" in str(e.value)
    env.shutdown()


def test_STORE_method_added_after_the_build_is_not_used():
    # AC-2: the method table is fixed at adaptation.
    store = Notes({"n/a.txt": b"a"})
    env = environment([{"type": "late", "prefix": "n"}], late=lambda: store)
    assert env.evaluate("-R/n/a.txt") == "a"
    store.listdir = lambda key: ["a.txt"]
    with pytest.raises(lq.KeyNotSupportedError, match="listdir"):
        env.store.listdir("n")
    env.shutdown()


class Raising(Files):
    def __init__(self):
        super().__init__({"r/ok.txt": b"ok"})

    def get(self, key):
        name = key.filename()
        if name == "missing.txt":
            raise lq.KeyNotFoundError("gone")
        if name == "boom.txt":
            raise ValueError("boom")
        if name == "text.txt":
            return "not bytes"
        return super().get(key)

    def set(self, key, data, metadata):
        raise OSError("disk full")


def test_STORE_none_raise_and_error_are_distinct():
    # AC-3: absent, a raised LiquersError, another exception and a wrong type stay distinct.
    store = lq.Store.from_object(Raising(), "r")
    with pytest.raises(lq.KeyNotFoundError):
        store.get("r/absent.txt")
    with pytest.raises(lq.KeyNotFoundError, match="gone"):
        store.get("r/missing.txt")
    with pytest.raises(lq.KeyReadError) as e:
        store.get("r/boom.txt")
    assert "ValueError: boom" in str(e.value)
    assert "in get" in (e.value.traceback or ""), e.value.traceback
    with pytest.raises(lq.KeyReadError, match="must return bytes"):
        store.get("r/text.txt")
    with pytest.raises(lq.KeyWriteError, match="disk full"):
        store.set("r/new.txt", b"x")
    store.close()


def test_STORE_absent_optional_method():
    # AC-4: absent methods are loud, or derived where question 1 says so.
    store = lq.Store.from_object(Notes({"f/a.txt": b"a"}), "f")
    with pytest.raises(lq.KeyNotSupportedError, match="'listdir'"):
        store.listdir("f")
    assert store.is_dir("f") is False                       # a flat store
    assert store.contains("f/a.txt") and not store.contains("f/b.txt")
    assert store.get_metadata("f/a.txt").file_size() == 1
    assert store.is_supported("f/a.txt")                    # no is_supported: supported
    assert store.is_read_only()                             # no write method at all
    with pytest.raises(lq.KeyNotSupportedError, match="'makedir'"):
        store.makedir("f/d")
    store.close()


class Counting(Files):
    def __init__(self):
        super().__init__({})
        self.calls = 0

    def get(self, key):
        self.calls += 1
        return super().get(key)

    def set(self, key, data, metadata):
        self.calls += 1
        super().set(key, data, metadata)


def test_STORE_relative_key_never_reaches_python():
    # AC-5: KeyNotAbsolute before Python, held directly and routed.
    source = Counting()
    store = lq.Store.from_object(source, "")
    env = environment([{"type": "counting", "prefix": "c"}], counting=lambda: source)
    for text in ["../escape", "a/../../etc", "a/./b"]:
        with pytest.raises(lq.KeyNotAbsoluteError):
            store.get(text)
        with pytest.raises(lq.KeyNotAbsoluteError):
            store.set(text, b"x")
        assert not store.is_supported(text)
    with pytest.raises(lq.LiquersError):
        env.store.get("c/../../etc")
    assert source.calls == 0
    store.close()
    env.shutdown()


class Blocked(Files):
    def __init__(self, event):
        super().__init__({"slow/a.txt": b"a"})
        self.event = event

    def get(self, key):
        self.event.wait(10)
        return super().get(key)


class AsyncBlocked(Files):
    def __init__(self, event):
        super().__init__({"slow/a.txt": b"a"})
        self.event = event

    async def get(self, key):
        while not self.event.is_set():
            await asyncio.sleep(0.01)
        return super().get(key)


@pytest.mark.parametrize("kind", [Blocked, AsyncBlocked])
def test_STORE_rust_mount_does_not_wait_on_python(kind):
    # AC-6: a query on a Rust mount completes while a Python get is blocked.
    event = threading.Event()
    env = environment([{"type": "slow", "prefix": "slow"}, {"type": "memory", "prefix": "scratch"}],
                      slow=lambda: kind(event))
    env.store.set("scratch/x.txt", b"fast")

    async def main():
        slow = asyncio.ensure_future(env.evaluate_async("-R/slow/a.txt"))
        await asyncio.sleep(0.1)
        fast = await asyncio.wait_for(env.evaluate_async("-R/scratch/x.txt"), 5)
        assert not slow.done()
        event.set()
        return fast, await asyncio.wait_for(slow, 10)

    try:
        assert run_async(main) == ("fast", "a")
    finally:
        event.set()
        env.shutdown()
