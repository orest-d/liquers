import pytest

from liquers_py import (
    CommandMetadata,
    DependencyKey,
    DependencyRecord,
    DependencyRelation,
    Expires,
    ExpirationTime,
    Key,
    PlanDependency,
    QuerySource,
    Recipe,
    RecipeList,
    Version,
    parse,
    parse_key,
)


def test_query_key_roundtrip_and_query_source():
    q = parse("a/b/-/x")
    assert q.encode() == "-R/a/b/-/x"

    k = parse_key("a/b/c")
    assert k.encode() == "a/b/c"

    src = QuerySource.key(k)
    assert "key" in src.encode()


def test_key_invalid_constructor_raises():
    with pytest.raises(Exception):
        Key("???")


def test_command_metadata_json_roundtrip_and_equality():
    cm = CommandMetadata("hello")
    payload = cm.to_json()
    cm2 = CommandMetadata.from_json(payload)
    assert cm == cm2


def test_expires_and_expiration_time_parse():
    e = Expires("never")
    assert e.encode() == "never"

    t = ExpirationTime("never")
    assert t.is_never()


def test_dependency_wrappers_and_hashable_relation():
    key = DependencyKey("-R/a/b")
    version = Version(1)
    record = DependencyRecord(key, version)
    assert "-R/a/b" in record.key.encode()

    rel = DependencyRelation.parameter_link("arg")
    dep = PlanDependency(key, rel)
    assert dep.relation == rel


def test_recipe_list_roundtrip():
    recipe = Recipe("a/b/c.txt", "Title", "Description")
    recipes = RecipeList()
    recipes.add_recipe(recipe)
    assert len(recipes) == 1

    as_json = recipes.to_json()
    loaded = RecipeList.from_json(as_json)
    assert len(loaded) == 1
