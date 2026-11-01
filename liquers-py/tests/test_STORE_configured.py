"""Configured stores: routing, store types, read_only and metadata sections (design
liquers-py-store, AC-9 to AC-14, AC-17; STORE09, STORE11-STORE18)."""

import pytest

import liquers as lq
from conftest import na
from store_fixtures import DataStore, Files, Notes, environment


def k(text):
    return lq.parse_key(text)


class CountingFiles(Files):
    def __init__(self):
        super().__init__({})
        self.writes = 0

    def set(self, key, data, metadata):
        self.writes += 1
        super().set(key, data, metadata)


def test_STORE09_read_only_store_refuses_writes():
    # AC-9: a store without writes refuses them, and a refusal never falls through.
    env = environment([{"type": "notes", "prefix": "ro"}, {"type": "memory", "prefix": "w"}],
                      notes=lambda: Notes({"ro/fixed.txt": b"hi"}))
    with pytest.raises(lq.KeyNotSupportedError):
        env.store.set(k("ro/new.txt"), b"x")
    assert not env.store.contains(k("w/new.txt"))
    env.store.set(k("w/ok.txt"), b"y")
    assert env.store.get(k("w/ok.txt")) == b"y"
    env.shutdown()
    # A read_only mount over a broader writable one: the write is refused, not redirected.
    env = environment([{"type": "memory", "prefix": "data", "read_only": True},
                       {"type": "memory", "prefix": ""}])
    with pytest.raises(lq.NotSupportedError):
        env.store.set(k("data/x.txt"), b"x")
    assert not env.store.contains(k("data/x.txt"))
    env.shutdown()


def test_STORE11_configured_router_routes_by_prefix():
    # AC-9: first match wins for overlapping prefixes; an unmatched key is absent.
    env = environment([{"type": "memory", "prefix": "a"}, {"type": "files", "prefix": "b"}],
                      files=lambda: Files({}))
    env.store.set(k("a/one.txt"), b"1")
    assert env.store.contains(k("a/one.txt"))
    assert not env.store.contains(k("b/one.txt"))
    with pytest.raises(lq.KeyNotFoundError):
        env.store.get(k("zzz/nope.txt"))
    assert env.store.listdir(k("a")) == ["one.txt"]
    env.shutdown()
    shadowed = CountingFiles()
    env = environment([{"type": "memory", "prefix": "data"},
                       {"type": "shadowed", "prefix": "data/scratch"}],
                      shadowed=lambda: shadowed)
    env.store.set(k("data/scratch/x"), b"1")
    assert shadowed.writes == 0 and env.store.get(k("data/scratch/x")) == b"1"
    env.shutdown()


def test_STORE12_integration_store_types_are_configurable(tmp_path):
    # AC-10: built-in and Python types from one document. The override half is NA: a Python type
    # cannot take a name the built-in chain resolves (test_STORE_type_declaration_and_failures).
    (tmp_path / "f").mkdir()
    (tmp_path / "o").mkdir()
    env = environment([
        {"type": "memory", "prefix": "m"},
        {"type": "filesystem", "prefix": "f", "config": {"path": str(tmp_path / "f")}},
        {"type": "fs", "prefix": "o", "config": {"root": str(tmp_path / "o")}},
        {"type": "notes", "prefix": "n"},
    ], notes=lambda prefix: Notes({f"{prefix}/hello.txt": b"Hello"}))
    for prefix in ["m", "f", "o"]:
        env.store.set(k(f"{prefix}/x.txt"), b"1")
        assert env.evaluate(f"-R/{prefix}/x.txt") == "1"
    assert env.evaluate("-R/n/hello.txt") == "Hello"
    env.shutdown()


def test_STORE13_unavailable_store_type_names_the_reason():
    # AC-10: a type that exists but is gated off in this build names the feature.
    types = {t["store_type"]: t for t in lq.Environment().store_types()}
    assert types["redis"]["availability"] != "Available", "redis is in this build; pick another type"
    env = environment([{"type": "redis", "prefix": "remote"}])
    with pytest.raises(lq.LiquersError) as e:
        env.evaluate("-R/remote/x.txt")
    message = str(e.value)
    assert "services-redis" in message and "Unknown store type" not in message
    env.shutdown()


def test_STORE14_read_only_mount_refuses_writes():
    # AC-11: `read_only` refuses every write before Python is called.
    source = CountingFiles()
    source.files["ro/fixed.txt"] = b"hi"
    env = environment([{"type": "files", "prefix": "ro", "read_only": True}], files=lambda: source)
    assert env.store.get(k("ro/fixed.txt")) == b"hi"
    with pytest.raises(lq.NotSupportedError):
        env.store.set(k("ro/new.txt"), b"x")
    with pytest.raises(lq.NotSupportedError):
        env.store.remove(k("ro/fixed.txt"))
    assert source.writes == 0 and "ro/fixed.txt" in source.files
    assert env.store.is_read_only()
    env.shutdown()


class OptedOut(Files):
    data_access = False


@pytest.mark.parametrize("factory", [lambda: Notes({}), lambda: OptedOut({})], ids=["no-listdir", "opted-out"])
def test_STORE15_metadata_without_data_access_fails_at_build(factory):
    # AC-12: at build, naming the mount and the missing capability.
    env = environment([{"type": "plain", "prefix": "d", "metadata": {"storage": "grouped"}}], plain=factory)
    with pytest.raises(lq.LiquersError) as e:
        env.evaluate("-R/d/x.txt")
    assert "'d'" in str(e.value) and "data-only access" in str(e.value)
    env.shutdown()


def _metadata_store(kind, tmp_path, storage):
    source = DataStore()
    entry = {
        "memory": {"type": "memory", "prefix": "n"},
        "filesystem": {"type": "filesystem", "prefix": "n", "config": {"path": str(tmp_path)}},
        "fs": {"type": "fs", "prefix": "n", "config": {"root": str(tmp_path)}},
        "python": {"type": "data", "prefix": "n"},
    }[kind]
    entry["metadata"] = {"storage": storage}
    return environment([entry], data=lambda: source), source


@pytest.mark.parametrize("storage", ["per_entry", "grouped"])
@pytest.mark.parametrize("kind", ["memory", "filesystem", "fs", "python"])
def test_STORE16_records_round_trip_as_data_objects(kind, storage, tmp_path):
    # AC-13: records round-trip; the default on_read chain applies; Python `get` is never called.
    env, source = _metadata_store(kind, tmp_path, storage)
    env.store.set(k("n/a.md"), b"# A", lq.MetadataRecord())
    env.store.set(k("n/b.txt"), b"bee")
    assert env.store.get_metadata(k("n/a.md")).get_asset_info().media_type == "text/markdown"
    assert sorted(env.store.listdir(k("n"))) == ["a.md", "b.txt"]
    assert env.store.get_metadata(k("n/b.txt")).get_asset_info().file_size == 3
    if kind == "python":
        present, absent = (("n/__metadata__.json", "n/a.md.__metadata__") if storage == "grouped"
                           else ("n/a.md.__metadata__", "n/__metadata__.json"))
        assert present in source.data and absent not in source.data
        assert source.gets == 0
    env.shutdown()


@pytest.mark.parametrize("kind", ["memory", "filesystem", "fs", "python"])
def test_STORE17_created_round_trips_from_backend_time(kind, tmp_path):
    # AC-14: `created` from the backend time compares equal after a re-read.
    env, _ = _metadata_store(kind, tmp_path, "per_entry")
    env.store.set(k("n/a.txt"), b"abc")
    first = env.store.get_metadata(k("n/a.txt")).get_asset_info()
    again = env.store.get_metadata(k("n/a.txt")).get_asset_info()
    assert first.created is not None and first.created == again.created
    if kind == "python":
        assert first.created == "2026-10-15T12:00:00.123Z" and first.etag == "e3"
    env.shutdown()


def test_STORE_minimal_store_keeps_configuration_effective():
    # AC-17: a store with get/set/remove/listdir/is_dir takes a metadata section through derived
    # data-only access, and the configured chains run through env.store and queries.
    files = Files({})
    env = environment([{"type": "files", "prefix": "n", "metadata": {
        "storage": "per_entry",
        "on_write": "defaults/media_type-fill/file_facts",
        "on_read": "media_type-fill/file_facts"}}], files=lambda prefix: files)
    env.store.set(k("n/a.md"), b"# A", lq.MetadataRecord())
    assert env.store.listdir(k("n")) == ["a.md"]
    assert "n/a.md.__metadata__" in files.files
    info = env.store.get_metadata(k("n/a.md")).get_asset_info()
    assert info.media_type == "text/markdown" and info.file_size == 3
    assert env.evaluate("-R/n/a.md") == "# A"
    env.shutdown()


@na("Python cannot define store_config handlers yet (LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE); "
    "a Python store is wrapped in the Rust pipeline instead (design liquers-py-store, Decision 5)")
def test_STORE18_language_store_config_handler():
    pass
