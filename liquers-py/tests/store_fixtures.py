"""Python store objects the STORE suites run against (design liquers-py-store, Phase 3)."""

import liquers as lq


def _children(keys, folder):
    """The names directly under ``folder`` among ``keys`` (text keys, no leading slash)."""
    prefix = f"{folder}/" if folder else ""
    return sorted({k[len(prefix):].split("/")[0] for k in keys if k.startswith(prefix) and k != folder})


class Notes:
    """The Phase 1 problem example: a store with only ``get``."""

    def __init__(self, files):
        self.files = files

    def get(self, key):
        return self.files.get(str(key))


class Files(Notes):
    """A minimal writable store: ``get``, ``set``, ``remove``, ``listdir`` and ``is_dir``."""

    def set(self, key, data, metadata):
        self.files[str(key)] = data

    def remove(self, key):
        self.files.pop(str(key), None)

    def is_dir(self, key):
        return any(k.startswith(f"{key}/") for k in self.files)

    def listdir(self, key):
        return _children(self.files, str(key))


class DictStore:
    """The whole store protocol on a dict, metadata kept; no data-only methods."""

    def __init__(self):
        self.data, self.metadata, self.dirs, self.calls = {}, {}, set(), 0

    def _seen(self):
        self.calls += 1

    def get(self, key):
        self._seen()
        k = str(key)
        if k not in self.data:
            return None
        return self.data[k], self.metadata.get(k)

    def get_metadata(self, key):
        self._seen()
        k = str(key)
        if k not in self.data:
            return None
        return self.metadata.get(k) or lq.MetadataRecord()

    def set(self, key, data, metadata):
        self._seen()
        self.data[str(key)] = data
        self.metadata[str(key)] = metadata

    def set_metadata(self, key, metadata):
        self._seen()
        if str(key) not in self.data:
            raise lq.KeyNotFoundError(f"no data under {key}")
        self.metadata[str(key)] = metadata

    def remove(self, key):
        self._seen()
        self.data.pop(str(key), None)
        self.metadata.pop(str(key), None)

    def removedir(self, key):
        self._seen()
        prefix = f"{key}/"
        for k in [k for k in self.data if k.startswith(prefix)]:
            self.remove(lq.parse_key(k))
        self.dirs = {d for d in self.dirs if d != str(key) and not d.startswith(prefix)}

    def makedir(self, key):
        self._seen()
        self.dirs.add(str(key))

    def contains(self, key):
        self._seen()
        return str(key) in self.data or self.is_dir(key)

    def is_dir(self, key):
        self._seen()
        k = str(key)
        return k in self.dirs or any(d.startswith(f"{k}/") for d in list(self.data) + list(self.dirs))

    def listdir(self, key):
        self._seen()
        return _children(list(self.data) + list(self.dirs), str(key))


class DataStore:
    """Appendix A's language store with data-only access; ``gets`` counts calls of ``get``."""

    def __init__(self):
        self.data, self.gets = {}, 0

    def get(self, key):
        self.gets += 1
        return None if str(key) not in self.data else (self.data[str(key)], None)

    def get_data(self, key):
        return self.data.get(str(key))

    def set_data(self, key, data):
        self.data[str(key)] = data

    def remove_data(self, key):
        self.data.pop(str(key), None)

    def listdir_data(self, key):
        return _children(self.data, str(key))

    def data_facts(self, key):
        if str(key) not in self.data:
            return None
        b = self.data[str(key)]
        return {"size": len(b), "modified": "2026-10-15T12:00:00.123Z", "etag": f"e{len(b)}"}


def environment(stores, **factories):
    """An environment with the given mounts and Python store types registered."""
    env = lq.Environment(config={"store": {"stores": stores}})
    for name, factory in factories.items():
        env.register_store_factory(name, factory)
    return env
