"""Recipe provider factories: declaration, repeated entries, late registration and release
(design liquers-py-recipes, AC-5, AC-10, AC-12)."""

import threading

import pytest

import liquers as lq
from test_RECIPE_python_providers import Convert, Reports, convert_environment


def k(text):
    return lq.parse_key(text)


def _first_use_error(env, timeout=10):
    """The error the first use raises, failing the test instead of hanging past ``timeout``."""
    outcome = {}

    def run():
        try:
            env.recipes.recipe_opt(k("reports/x.txt"))
        except BaseException as e:  # reported on the test's thread
            outcome["error"] = e

    thread = threading.Thread(target=run, daemon=True)
    thread.start()
    thread.join(timeout)
    assert not thread.is_alive(), "the build hung"
    return outcome.get("error")


class NoRecipeOpt:
    def assets_with_recipes(self, folder):
        return []


def test_RECIPE_factory_and_protocol_checked():
    # AC-5: refused names at registration; each build failure a ConfigurationError naming the entry.
    env = lq.Environment()
    env.register_recipe_provider_factory("reports", Reports)
    for taken in ["reports", "recipes_yaml", "manifest", "default", "trivial", "none", "no_recipes", ""]:
        with pytest.raises(lq.ConfigurationError):
            env.register_recipe_provider_factory(taken, Reports)
    with pytest.raises(lq.ConfigurationError, match="reserved"):
        env.register_recipe_provider_factory("typed", lambda type: None)
    env.shutdown()

    def raises():
        raise ValueError("no backend")

    async def coroutine_factory():
        return Reports("x")

    holder = {}

    def uses_environment():
        holder["env"].evaluate("hello")
        return Reports("reports")

    cases = [
        ({"type": "needs"}, Reports, "required argument 'folder'"),
        ({"type": "needs", "folder": "reports", "extra": 1}, Reports, "'extra'"),
        ({"type": "needs"}, raises, "no backend"),
        ({"type": "needs"}, lambda: coroutine_factory(), "awaitable"),
        ({"type": "needs"}, NoRecipeOpt, "'recipe_opt'"),
        ({"type": "needs"}, uses_environment, "may not use its environment"),
    ]
    for entry, factory, expected in cases:
        env = lq.Environment(config={"recipes": [entry]})
        holder["env"] = env
        env.register_recipe_provider_factory("needs", factory)
        error = _first_use_error(env)
        assert isinstance(error, lq.ConfigurationError), (expected, error)
        assert expected in str(error) and "needs" in str(error), (expected, str(error))
        env.shutdown()
    env = lq.Environment(config={"recipes": [{"type": "needs"}]})
    env.register_recipe_provider_factory("needs", raises)
    assert "raises" in (_first_use_error(env).traceback or "")
    env.shutdown()


def test_RECIPE_late_registration_and_release():
    # AC-10: (a) a factory registered after a failed build lets the next use build.
    assert hasattr(lq.Environment, "live_handle_count"), "build with --features debug-handles"
    baseline = lq.Environment.live_handle_count()
    env = lq.Environment(config={
        "recipes": ["recipes_yaml", {"type": "reports", "folder": "reports"}],
        "store": {"stores": [{"type": "memory", "prefix": ""}]}})

    @env.first_command
    def hello():
        return "Hello"

    with pytest.raises(lq.LiquersError):
        env.evaluate("-R/reports/summary.txt")
    env.register_recipe_provider_factory("reports", Reports)
    assert env.evaluate("-R/reports/summary.txt") == "Hello"
    # (b) after a use and a store write, a late command rebuilds the chain with the write kept.
    env.store.set("data/kept.txt", b"kept")

    @env.command
    def shout(text):
        return text.upper()

    assert env.evaluate("-R/data/kept.txt/-/shout") == "KEPT"
    assert env.recipes.recipe(k("reports/a.txt")).query == "hello/a.txt"
    held = env.recipes
    assert lq.Environment.live_handle_count() > baseline
    env.shutdown()
    assert lq.Environment.live_handle_count() == baseline
    with pytest.raises(lq.LiquersError):
        held.recipe_opt(k("reports/a.txt"))


def test_RECIPE_one_factory_several_entries():
    # AC-12: one type, two argument sets, two members; an identical entry is refused; two members
    # claiming one key are named by position.
    env = convert_environment()
    assert env.recipes.recipe(k("art/logo.png")).query.endswith("to_png")
    assert env.recipes.recipe(k("art/logo.jpg")).query.endswith("to_jpg")
    env.shutdown()
    env = convert_environment({"type": "convert", "folder": "art", "ext": ".png", "command": "to_png"})
    with pytest.raises(lq.ConfigurationError, match="more than once"):
        env.recipes.recipe_opt(k("art/logo.png"))
    env.shutdown()
    env = convert_environment({"type": "convert", "folder": "art", "ext": ".png", "command": "to_webp"})
    with pytest.raises(lq.LiquersError) as e:
        env.recipes.recipe_opt(k("art/logo.png"))
    assert "convert#0" in str(e.value) and "convert#2" in str(e.value), str(e.value)
    assert env.recipes.recipe(k("art/logo.jpg")).query.endswith("to_jpg")
    env.shutdown()


def test_RECIPE_provider_type_is_reported_unknown_before_registration():
    env = lq.Environment(config={"recipes": [{"type": "convert", "folder": "art", "ext": ".png",
                                              "command": "to_png"}]})
    with pytest.raises(lq.LiquersError, match="convert"):
        env.recipes.recipe_opt(k("art/logo.png"))
    env.register_recipe_provider_factory("convert", Convert)
    assert env.recipes.recipe_opt(k("art/logo.png")) is not None
    env.shutdown()
