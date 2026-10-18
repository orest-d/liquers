"""The LiquersError hierarchy (design liquers-py, step 5)."""

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
