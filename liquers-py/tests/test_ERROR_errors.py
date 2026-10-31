"""The LiquersError hierarchy (design liquers-py, step 5)."""

import pytest

import liquers as lq


def test_every_error_type_has_a_subclass():
    for name in dir(lq.ErrorType):
        member = getattr(lq.ErrorType, name)
        if not isinstance(member, lq.ErrorType):
            continue
        class_name = name if name.endswith("Error") else name + "Error"
        cls = getattr(lq, class_name)
        assert issubclass(cls, lq.LiquersError)
        assert class_name in lq.__all__


def test_base_constructor_dispatches_to_the_subclass():
    err = lq.LiquersError(lq.ErrorType.KeyNotFound, "no such key", key="a/b")
    assert type(err) is lq.KeyNotFoundError
    assert err.type == lq.ErrorType.KeyNotFound
    assert err.message == "no such key"
    assert err.key == "a/b"


def test_subclass_constructor_sets_its_type():
    err = lq.ExecutionError("boom", query="x/y")
    assert err.type == lq.ErrorType.ExecutionError
    assert str(err) == "boom"
    assert err.query == "x/y"
    assert isinstance(err, lq.LiquersError)


def test_ERROR01_every_error_type_maps():
    members = [getattr(lq.ErrorType, n) for n in dir(lq.ErrorType)]
    members = [m for m in members if isinstance(m, lq.ErrorType)]
    assert len(members) > 20
    for member in members:
        err = lq.LiquersError(member, "m")
        assert isinstance(err, lq.LiquersError) and err.type == member
        assert type(err) is not lq.LiquersError


def test_ERROR02_fields_survive_a_command(env):
    @env.first_command
    def located():
        raise lq.ParseError("bad", query="a/b", key="k/x")

    with pytest.raises(lq.ParseError) as raised:
        env.evaluate("located")
    assert raised.value.message == "bad"
    assert raised.value.key == "k/x"
    # The asset layer records the query being evaluated, replacing the one the command set.
    assert raised.value.query in ("a/b", "located")


def test_ERROR03_language_exception_includes_class_and_stack(env):
    with pytest.raises(lq.ExecutionError) as raised:
        env.evaluate("hello/boom")
    assert "ValueError" in raised.value.message and "inner" in raised.value.message
    assert raised.value.traceback and "boom" in raised.value.traceback


def test_ERROR04_non_error_throw_has_safe_fallback(env):
    @env.command
    def interrupted(value):
        raise KeyboardInterrupt("not an Exception subclass")

    with pytest.raises(lq.ExecutionError) as raised:
        env.evaluate("hello/interrupted")
    assert "KeyboardInterrupt" in raised.value.message
    assert env.evaluate("hello") == "hello"


def test_ERROR06_language_raised_error_keeps_its_type(env):
    @env.first_command
    def missing():
        raise lq.LiquersError(lq.ErrorType.KeyNotFound, "no such key")

    with pytest.raises(lq.KeyNotFoundError) as raised:
        env.evaluate("missing")
    assert raised.value.type == lq.ErrorType.KeyNotFound
    assert "no such key" in raised.value.message


def test_ERROR_nested_python_tracebacks_are_concatenated(env):
    # Python -> Rust -> Python: the inner command's stack and the outer command's frames both
    # reach the caller, inner first. A traceback is a plain concatenation for now; see
    # MULTI-LANGUAGE-TRACEBACK-IS-A-FLAT-STRING.
    @env.first_command
    def inner_fails():
        raise ValueError("deep")

    @env.first_command
    def outer_calls(context):
        return context.evaluate("inner_fails")

    with pytest.raises(lq.ExecutionError) as raised:
        env.evaluate("outer_calls")
    traceback = raised.value.traceback
    assert "ValueError: deep" in raised.value.message
    assert "inner_fails" in traceback and "outer_calls" in traceback
    assert traceback.index("inner_fails") < traceback.index("outer_calls")


def test_ERROR05_no_panic_crosses_the_boundary(env):
    # `test_panic` is a Rust command that panics, registered only under `debug-handles`.
    assert hasattr(lq.Environment, "live_handle_count"), "build with --features debug-handles"
    with pytest.raises(lq.LiquersError, match="test_panic was evaluated") as raised:
        env.evaluate("test_panic")
    assert raised.value.type != lq.ErrorType.ActionNotRegistered


def test_ERROR_category_classes():
    """Each variant's class derives from exactly the classes of its categories (a DAG)."""
    categories = {name: getattr(lq, name + "Error") for name in lq.ErrorType.all_categories()}
    assert set(categories) == {"Store", "KeyedAsset", "NotFound"}
    for cls in categories.values():
        assert issubclass(cls, lq.LiquersError)
        assert cls.__name__ in lq.__all__
    assert issubclass(lq.NotFoundError, LookupError)
    for name in dir(lq.ErrorType):
        member = getattr(lq.ErrorType, name)
        if not isinstance(member, lq.ErrorType):
            continue
        class_name = name if name.endswith("Error") else name + "Error"
        assert class_name not in {c.__name__ for c in categories.values()}
        cls = getattr(lq, class_name)
        for category, category_cls in categories.items():
            assert issubclass(cls, category_cls) == (category in member.categories()), class_name
    assert lq.ErrorType.KeyNotFound.categories() == ["Store", "NotFound"]
    assert lq.ErrorType.AssetNotFound.categories() == ["KeyedAsset", "NotFound"]
    assert lq.ErrorType.General.categories() == []


def test_ERROR_not_found_catches_both_misses():
    for error_type in (lq.ErrorType.KeyNotFound, lq.ErrorType.AssetNotFound):
        with pytest.raises(lq.NotFoundError):
            raise lq.LiquersError(error_type, "missing", key="a/b")
        with pytest.raises(LookupError):
            raise lq.LiquersError(error_type, "missing", key="a/b")
    with pytest.raises(lq.StoreError):
        raise lq.KeyReadError("unreadable")


# Design py-metadata-typed-errors: the wrapper classes' own failures raise the typed LiquersError.
_JSON_CLASSES = [
    "Metadata",
    "MetadataRecord",
    "AssetInfo",
    "LogEntry",
    "RecipeList",
    "Recipe",
    "EnumArgument",
    "ArgumentType",
    "ArgumentInfo",
    "CommandDefinition",
    "CommandPreset",
    "CommandMetadata",
    "CommandMetadataRegistry",
    "Step",
    "ParameterValue",
    "ResolvedParameterValues",
    "Plan",
]
_YAML_CLASSES = ["Recipe", "ArgumentInfo", "CommandMetadata", "CommandMetadataRegistry", "Plan"]


@pytest.mark.parametrize(
    "class_name, method, text",
    [(name, "from_json", "{") for name in _JSON_CLASSES]
    + [(name, "from_yaml", "a: [") for name in _YAML_CLASSES],
)
def test_ERROR_wrapper_deserialization_raises_serialization_error(class_name, method, text):
    parse = getattr(getattr(lq, class_name), method)
    with pytest.raises(lq.SerializationError) as raised:
        parse(text)
    assert isinstance(raised.value, lq.LiquersError)
    assert raised.value.type == lq.ErrorType.SerializationError
    assert raised.value.message


def test_ERROR_stored_error_keeps_its_type(env):
    @env.first_command
    def missing():
        raise lq.KeyNotFoundError("no such key")

    asset = env.get_asset("missing")
    with pytest.raises(lq.KeyNotFoundError):
        asset.get()
    with pytest.raises(lq.KeyNotFoundError, match="no such key"):
        asset.metadata.error_result()

    metadata = lq.Metadata()
    metadata.with_error("boom")
    with pytest.raises(lq.GeneralError) as raised:
        metadata.error_result()
    assert raised.value.message == "boom"


def test_ERROR_wrapper_constructors_raise_typed_errors():
    with pytest.raises(lq.ParseError):
        lq.Recipe("a/b-~", "t", "d")
    with pytest.raises(lq.ParseError):
        lq.CommandPreset("a/b-~", "l", "d")
    with pytest.raises(lq.GeneralError):
        lq.Expires("not a time")
    with pytest.raises(lq.GeneralError):
        lq.ExpirationTime("not a time")


def test_ERROR_typed_wrapper_errors_are_still_exceptions():
    assert issubclass(lq.SerializationError, Exception)
    try:
        lq.Metadata.from_json("x")
    except Exception as e:  # noqa: BLE001 - an existing handler of this form must still catch it
        caught = e
    else:
        caught = None
    assert caught is not None
