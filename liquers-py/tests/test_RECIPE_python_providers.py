"""Python recipe providers in the chain (design liquers-py-recipes, AC-1 to AC-9; RECIPE01-RECIPE07)."""

import asyncio
import json
import threading

import pytest

import liquers as lq
from conftest import run_async


def k(text):
    return lq.parse_key(text)


class Reports:
    """Example 1: one report per name in its folder; no recipes.yaml could list them."""

    def __init__(self, folder):
        self.folder = folder

    def recipe_opt(self, key):
        if str(key.parent()) != self.folder:
            return None
        if key.filename() == "fresh.txt":
            return lq.Recipe("hello", volatile=True)
        if key.filename() == "daily.txt":
            return lq.Recipe("hello", expires=lq.Expires("in 5 min"))
        return f"hello/{key.filename()}"


def reports_environment(*entries, **types):
    env = lq.Environment(config={"recipes": list(entries) or
                                 ["recipes_yaml", {"type": "reports", "folder": "reports"}]})
    env.register_recipe_provider_factory("reports", Reports)
    for name, factory in types.items():
        env.register_recipe_provider_factory(name, factory)

    @env.first_command
    def hello():
        return "Hello"

    return env


def test_RECIPE06_end_to_end_keyed_evaluation():
    # AC-1: a key only the Python provider knows evaluates; an unowned key is absent.
    env = reports_environment()
    assert env.evaluate("-R/reports/summary.txt") == "Hello"
    with pytest.raises(lq.AssetNotFoundError):
        env.evaluate("-R/elsewhere/x.txt")
    env.shutdown()


class Claims:
    def __init__(self, query):
        self.query = query

    def recipe_opt(self, key):
        return self.query if str(key) == "reports/x.txt" else None


def test_RECIPE_not_mine_and_ownership_conflict():
    # AC-2: `None` defers to recipes_yaml; two members claiming one key fail naming both.
    env = reports_environment()
    env.store.set("other/recipes.yaml", b"recipes:\n  - query: hello/listed.txt\n")
    assert env.evaluate("-R/other/listed.txt") == "Hello"
    env.shutdown()
    env = reports_environment({"type": "claims", "query": "hello/x.txt"},
                              {"type": "claims", "query": "hello/y.txt"}, claims=Claims)
    with pytest.raises(lq.LiquersError) as e:
        env.recipes.recipe_opt(k("reports/x.txt"))
    message = str(e.value)
    assert "claims#0" in message and "claims#1" in message, message
    env.shutdown()


class Forms:
    def recipe_opt(self, key):
        name = key.filename()
        return {
            "recipe.txt": lq.Recipe("hello", "A title"),
            "text.txt": "hello",
            "own_cwd.txt": lq.Recipe("hello", cwd="elsewhere"),
            "dict.txt": {"query": "hello"},
            "int.txt": 3,
            "bad.txt": "hello/-/(((",
        }.get(name)


def test_RECIPE_return_forms():
    # AC-3: lq.Recipe, query text and a recipe with its own cwd; other types are refused.
    env = reports_environment({"type": "forms"}, forms=Forms)
    recipes = env.recipes
    assert recipes.recipe(k("f/recipe.txt")).title == "A title"
    assert recipes.recipe(k("f/recipe.txt")).cwd == "f"
    text = recipes.recipe(k("f/text.txt"))
    assert (text.query, text.cwd) == ("hello", "f")
    assert recipes.recipe(k("f/own_cwd.txt")).cwd == "elsewhere"
    for name, kind in [("dict.txt", "dict"), ("int.txt", "int")]:
        with pytest.raises(lq.ExecutionError, match=kind):
            recipes.recipe_opt(k(f"f/{name}"))
    with pytest.raises(lq.LiquersError) as e:
        recipes.recipe_opt(k("f/bad.txt"))
    assert not isinstance(e.value, lq.ExecutionError)
    assert recipes.recipe_opt(k("f/absent.txt")) is None
    env.shutdown()


def test_RECIPE_recipe_keyword_constructor_and_properties():
    # AC-3, AC-7: keywords and setters round-trip; positional calls keep working.
    recipe = lq.Recipe("hello", cwd="a/b", volatile=True, expires=lq.Expires("in 5 min"))
    assert (recipe.query, recipe.title, recipe.description) == ("hello", "", "")
    assert recipe.cwd == "a/b" and recipe.volatile
    assert str(recipe.expires) == str(lq.Expires("in 5 min"))
    recipe.cwd = None
    recipe.volatile = False
    recipe.expires = lq.Expires.never()
    assert recipe.cwd is None and not recipe.volatile
    assert str(recipe.expires) == str(lq.Expires.never())
    positional = lq.Recipe("hello", "Title", "Description")
    assert (positional.title, positional.description, positional.cwd) == ("Title", "Description", None)
    assert lq.Recipe.from_json(recipe.to_json()).to_json() == recipe.to_json()
    with pytest.raises(lq.LiquersError):
        lq.Recipe("hello/-/(((")


class Raises:
    def recipe_opt(self, key):
        if key.filename() == "missing.txt":
            raise lq.KeyNotFoundError("not here")
        raise ValueError("provider failed")


def test_RECIPE04_provider_error_maps_through_error():
    # AC-4: a LiquersError keeps its type; another exception is an ExecutionError with traceback.
    env = reports_environment({"type": "raises"}, raises=Raises)
    with pytest.raises(lq.KeyNotFoundError, match="not here"):
        env.recipes.recipe_opt(k("r/missing.txt"))
    with pytest.raises(lq.ExecutionError) as e:
        env.recipes.recipe_opt(k("r/x.txt"))
    assert "ValueError: provider failed" in str(e.value)
    assert "in recipe_opt" in (e.value.traceback or ""), e.value.traceback
    with pytest.raises(lq.LiquersError) as e:
        env.evaluate("-R/r/x.txt")
    assert "provider failed" in str(e.value)
    env.shutdown()


class Convert:
    """Example 2: one factory, several entries."""

    def __init__(self, folder, ext, command):
        self.folder, self.ext, self.command = folder, ext, command

    def recipe_opt(self, key):
        name = key.filename()
        if str(key.parent()) == self.folder and name.endswith(self.ext):
            return f"-R/src/{name[:-len(self.ext)]}.svg/-/{self.command}"
        return None

    def assets_with_recipes(self, folder):
        return [f"logo{self.ext}"] if str(folder) == self.folder else []


def register_converters(env):
    for name in ["to_png", "to_jpg", "to_webp"]:
        env.command(lambda text: text, name=name)


def convert_environment(*extra):
    env = lq.Environment(config={"recipes": [
        {"type": "convert", "folder": "art", "ext": ".png", "command": "to_png"},
        {"type": "convert", "folder": "art", "ext": ".jpg", "command": "to_jpg"}, *extra]})
    env.register_recipe_provider_factory("convert", Convert)
    register_converters(env)
    return env


def test_RECIPE02_list_and_contains_are_consistent():
    # AC-6: every listed name is contained and has a recipe; an unlisted key can still be made.
    env = convert_environment()
    names = env.recipes.assets_with_recipes(k("art"))
    assert all(isinstance(n, lq.ResourceName) for n in names)
    assert sorted(map(str, names)) == ["logo.jpg", "logo.png"]
    for name in names:
        assert env.recipes.contains(k(f"art/{name}"))
        assert env.recipes.recipe_opt(k(f"art/{name}")) is not None
    assert not env.recipes.contains(k("art/other.png"))
    assert env.recipes.can_make(k("art/other.png"))
    assert env.recipes.recipe(k("art/logo.jpg")).query == "-R/src/logo.svg/-/to_jpg"
    env.shutdown()


def test_RECIPE_unlisted_provider_still_resolves():
    # AC-6: without assets_with_recipes a provider lists nothing, contains nothing, and still makes.
    env = reports_environment()
    assert list(env.recipes.assets_with_recipes(k("reports"))) == []
    assert not env.recipes.contains(k("reports/summary.txt"))
    assert env.recipes.can_make(k("reports/summary.txt"))
    assert env.evaluate("-R/reports/summary.txt") == "Hello"
    env.shutdown()


class Strays:
    def __init__(self, listing):
        self.listing = listing

    def recipe_opt(self, key):
        return None

    def assets_with_recipes(self, folder):
        return self.listing


@pytest.mark.parametrize("listing", [["sub/x.txt"], [lq.parse_key("other/x.txt")], [3], 5])
def test_RECIPE_listing_outside_folder_is_refused(listing):
    # AC-6: a listed entry outside `folder`, or not a name, is an ExecutionError.
    env = reports_environment({"type": "strays"}, strays=lambda: Strays(listing))
    with pytest.raises(lq.ExecutionError, match="assets_with_recipes"):
        env.recipes.assets_with_recipes(k("art"))
    env.shutdown()


def test_RECIPE_listing_accepts_keys_in_the_folder():
    env = reports_environment({"type": "strays"}, strays=lambda: Strays([lq.parse_key("art/a.txt"), "b.txt"]))
    assert sorted(map(str, env.recipes.assets_with_recipes(k("art")))) == ["a.txt", "b.txt"]
    env.shutdown()


def test_RECIPE03_recipe_produces_a_valid_plan():
    # AC-7: the plan sets the folder, runs the action and names the file.
    env = reports_environment()
    plan = json.loads(env.recipes.recipe_plan(k("reports/summary.txt")).to_json())
    text = json.dumps(plan["steps"])
    assert "SetCwd" in text and "hello" in text and "summary.txt" in text, text
    env.shutdown()


def test_RECIPE05_volatility_and_expiration_metadata_survive():
    # AC-7: volatile and expires reach the plan and the evaluated asset (separate recipes, since a
    # volatile result expires immediately).
    env = reports_environment()
    fresh = json.loads(env.recipes.recipe_plan(k("reports/fresh.txt")).to_json())
    assert fresh["is_volatile"]
    daily = json.loads(env.recipes.recipe_plan(k("reports/daily.txt")).to_json())
    expected = json.loads(lq.Recipe("hello", expires=lq.Expires("in 5 min")).to_json())["expires"]
    assert not daily["is_volatile"] and daily["expires"] == expected
    assert json.loads(env.evaluate_state("-R/reports/daily.txt").metadata.to_json())["expires"] == expected
    assert json.loads(env.evaluate_state("-R/reports/fresh.txt").metadata.to_json())["is_volatile"]
    env.shutdown()


class Nested:
    def __init__(self, env):
        self.env = env

    def recipe_opt(self, key):
        if str(key.parent()) != "nested":
            return None
        assert self.env.evaluate("hello") == "Hello"
        return "hello"


class AsyncNested:
    def __init__(self, env):
        self.env = env

    async def recipe_opt(self, key):
        if str(key.parent()) != "nested":
            return None
        assert await self.env.evaluate_async("hello") == "Hello"
        return "hello"


@pytest.mark.parametrize("kind", [Nested, AsyncNested], ids=["def", "async"])
def test_RECIPE07_nested_environment_use_follows_policy(kind):
    # AC-8: a provider evaluating through its own environment completes without deadlock.
    holder = {}
    env = reports_environment({"type": "nested"}, nested=lambda: kind(holder["env"]))
    holder["env"] = env
    outcome = {}

    def run():
        try:
            outcome["value"] = env.evaluate("-R/nested/x.txt")
        except BaseException as e:  # reported on the test's thread
            outcome["error"] = e

    thread = threading.Thread(target=run, daemon=True)
    thread.start()
    thread.join(5)
    assert not thread.is_alive(), "nested evaluation deadlocked"
    assert outcome.get("value") == "Hello", outcome
    env.shutdown()


def test_RECIPE01_found_and_missing_recipe():
    # AC-9.
    env = reports_environment()
    found = env.recipes.recipe_opt(k("reports/a.txt"))
    assert isinstance(found, lq.Recipe) and found.query == "hello/a.txt"
    assert env.recipes.recipe_opt("elsewhere/a.txt") is None
    # The chain raises GeneralError here (RECIPE-MISSING-RECIPE-IS-A-GENERAL-ERROR).
    with pytest.raises(lq.LiquersError, match="No recipe found"):
        env.recipes.recipe(k("elsewhere/a.txt"))
    env.shutdown()


class AsyncConvert(Convert):
    async def recipe_opt(self, key):
        await asyncio.sleep(0)
        return super().recipe_opt(key)

    async def assets_with_recipes(self, folder):
        return super().assets_with_recipes(folder)


def test_RECIPE_inspection_methods_and_async_twins():
    # AC-9: every inspection method and its awaitable twin, over a def and an async def provider.
    env = lq.Environment(config={"recipes": [
        {"type": "convert", "folder": "art", "ext": ".png", "command": "to_png"},
        {"type": "async_convert", "folder": "art", "ext": ".jpg", "command": "to_jpg"}]})
    env.register_recipe_provider_factory("convert", Convert)
    env.register_recipe_provider_factory("async_convert", AsyncConvert)
    register_converters(env)
    r = env.recipes

    async def main():
        return (
            await r.recipe_opt_async(k("art/logo.jpg")),
            await r.recipe_async(k("art/logo.png")),
            await r.recipe_plan_async(k("art/logo.jpg")),
            await r.assets_with_recipes_async(k("art")),
            await r.contains_async(k("art/logo.jpg")),
            await r.can_make_async(k("art/x.png")),
        )

    opt, recipe, plan, names, contains, can_make = run_async(main)
    assert opt.query == r.recipe_opt(k("art/logo.jpg")).query == "-R/src/logo.svg/-/to_jpg"
    assert recipe.query == r.recipe(k("art/logo.png")).query
    assert isinstance(plan, lq.Plan)
    assert plan.to_json() == r.recipe_plan(k("art/logo.jpg")).to_json()
    assert sorted(map(str, names)) == sorted(map(str, r.assets_with_recipes(k("art")))) == ["logo.jpg", "logo.png"]
    assert contains and r.contains(k("art/logo.jpg"))
    assert can_make and r.can_make(k("art/x.png"))
    env.shutdown()
