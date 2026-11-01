"""A Python store and a recipe provider, registered as factories."""

from typing import Any, assert_type

import liquers as lq


class DictStore:
    def __init__(self, prefix: str) -> None:
        self.prefix = prefix
        self.data: dict[str, bytes] = {}

    def get(self, key: lq.Key) -> tuple[bytes, dict[str, Any]] | None:
        found = self.data.get(key.encode())
        return None if found is None else (found, {})

    def set(self, key: lq.Key, data: bytes, metadata: lq.Metadata) -> None:
        self.data[key.encode()] = data


class Recipes:
    def recipe_opt(self, key: lq.Key) -> lq.Recipe | None:
        return lq.Recipe("hello", "Hello") if key.encode() == "hello.txt" else None


env = lq.Environment(config={"stores": [{"type": "dict", "prefix": "py"}]})
env.register_store_factory("dict", DictStore, label="Dict", doc="In memory")
env.register_recipe_provider_factory(
    "mine", Recipes, arguments=[lq.StoreArgumentInfo("x", "string", required=False, default="y")]
)
store = env.store
assert_type(store, lq.Store)
store.set("py/a.txt", b"x", {"type_identifier": "bytes"})
assert_type(store.get(lq.Key("py/a.txt")), bytes)
assert_type(store.get_metadata("py/a.txt"), lq.Metadata)
assert_type(store.listdir("py"), list[str])
assert_type(store.contains("py/a.txt"), bool)
assert_type(env.recipes.recipe_opt("hello.txt"), lq.Recipe | None)
assert_type(env.recipes.assets_with_recipes(""), list[lq.ResourceName])
wrapped = lq.Store.from_object(DictStore("x"), "x")
assert_type(wrapped, lq.Store)
