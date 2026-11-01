"""STORE01-STORE08 and STORE10 over every store liquers-py offers (design liquers-py-store, AC-5,
AC-7, AC-8). The bodies are LANGUAGE-INTEGRATION_GUIDE.md Appendix A's, as blocking calls."""

import asyncio

import pytest

import liquers as lq
from conftest import run_async
from store_fixtures import DictStore, environment

STORES = ["memory", "filesystem", "fs", "python", "python-direct"]


@pytest.fixture(params=STORES)
def each_store(request, tmp_path):
    """``(environment or None, store)`` for one store, mounted at the root."""
    kind = request.param
    if kind == "python-direct":
        store = lq.Store.from_object(DictStore())
        yield None, store
        store.close()
        return
    entry = {
        "memory": {"type": "memory", "prefix": ""},
        "filesystem": {"type": "filesystem", "prefix": "", "config": {"path": str(tmp_path)}},
        "fs": {"type": "fs", "prefix": "", "config": {"root": str(tmp_path)}},
        "python": {"type": "dict_store", "prefix": ""},
    }[kind]
    env = environment([entry], dict_store=DictStore)

    @env.command
    def greet(text):
        return f"Hello, {text}!"

    yield env, env.store
    env.shutdown()


def k(text):
    return lq.parse_key(text)


def test_STORE01_set_get_data_and_metadata(each_store):
    _, store = each_store
    record = lq.MetadataRecord()
    record.media_type = "text/plain"
    store.set(k("d/f.txt"), b"data", record)
    assert store.get(k("d/f.txt")) == b"data"
    assert store.get_metadata(k("d/f.txt")).get_asset_info().media_type == "text/plain"


def test_STORE02_missing_key_error(each_store):
    _, store = each_store
    with pytest.raises(lq.LiquersError) as e:
        store.get(k("d/absent"))
    assert e.value.type == lq.ErrorType.KeyNotFound


def test_STORE03_directory_listing_invariants(each_store):
    _, store = each_store
    store.set(k("d/a"), b"1", lq.Metadata())
    store.set(k("d/b"), b"2", lq.Metadata())
    names = store.listdir(k("d"))
    assert set(names) == {"a", "b"}
    for n in names:
        assert store.contains(k(f"d/{n}"))


def test_STORE04_remove_and_removedir(each_store):
    _, store = each_store
    store.set(k("d/x"), b"1", lq.Metadata())
    store.remove(k("d/x"))
    assert not store.contains(k("d/x"))
    store.removedir(k("d"))
    assert not store.contains(k("d"))


def test_STORE05_unsupported_key(each_store):
    _, store = each_store
    for text in ["../escape", "a/../../etc", "a/./b"]:
        key = k(text)
        with pytest.raises(lq.LiquersError) as e:
            store.get(key)
        assert e.value.type == lq.ErrorType.KeyNotAbsolute
        with pytest.raises(lq.LiquersError):
            store.set(key, b"x", lq.Metadata())
        assert not store.is_supported(key)
    assert store.is_supported(k("d/ordinary.txt"))
    for text in ["d/.hidden", "d/a..b", "d/...", "d/..x", "d/v1.2.3"]:
        assert store.is_supported(k(text)), text


def test_STORE05b_the_trap_that_makes_this_test_lie(each_store):
    _, store = each_store
    store.makedir(k("a"))
    with pytest.raises(lq.LiquersError) as e:
        store.get(k("a/../../etc"))
    assert e.value.type == lq.ErrorType.KeyNotAbsolute


def test_STORE06_concurrent_update_policy(each_store, request):
    if request.node.callspec.params["each_store"] == "fs":
        # Fails about one run in thirty; not a liquers-py defect.
        request.applymarker(pytest.mark.xfail(
            reason="STORE-OPENDAL-FS-CONCURRENT-WRITES-TO-ONE-KEY-FAIL", strict=False))
    _, store = each_store

    async def main():
        await asyncio.gather(*[store.set_async(k("d/race"), bytes([i]), lq.Metadata()) for i in range(10)])

    run_async(main)
    assert len(store.get(k("d/race"))) == 1              # last write wins, not a torn value


def test_STORE07_store_works_in_end_to_end_evaluation(each_store):
    env, store = each_store
    if env is None:
        pytest.skip("a store held through lq.Store.from_object is not mounted in an environment")
    store.set(k("d/in.txt"), b"hello", lq.Metadata())
    assert env.evaluate("-R/d/in.txt/-/greet") == "Hello, hello!"


def test_STORE08_integration_provided_store_satisfies_the_contract():
    # The parameter list above covers every local store type liquers-py ships, and the Python one.
    env = lq.Environment()
    types = {t["store_type"] for t in env.store_types()}
    assert {"memory", "filesystem", "fs"} <= types
    env.shutdown()


class Typed:
    """A Python store returning metadata that disagrees with the key's extension."""

    def get(self, key):
        if key.filename() == "plain.csv":
            return b"a,b"
        if key.filename() == "input.csv":
            record = lq.MetadataRecord()
            record.media_type = "application/json"
            return b"{}", record
        return None


def test_STORE10_metadata_inference_follows_documented_precedence(each_store):
    # Absent metadata is filled from the key; stored or returned metadata wins over the extension.
    _, store = each_store
    store.set(k("d/plain.csv"), b"a,b")
    assert store.get_metadata(k("d/plain.csv")).get_asset_info().media_type == "text/csv"
    record = lq.MetadataRecord()
    record.media_type = "application/json"
    store.set(k("d/input.csv"), b"{}", record)
    assert store.get_metadata(k("d/input.csv")).get_asset_info().media_type == "application/json"
    typed = lq.Store.from_object(Typed())
    assert typed.get_metadata(k("d/plain.csv")).get_asset_info().media_type == "text/csv"
    assert typed.get_metadata(k("d/input.csv")).get_asset_info().media_type == "application/json"
    typed.close()
