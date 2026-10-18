"""COMMAND: defaults, versions, unregistering overrides, unnamed callables and warnings.

Regressions for the Phase 4 soundness review (specs/design/liquers-py/phase4-implementation.md).
"""

import functools
import warnings

import pytest

import liquers as lq
from liquers._commands import _implementation_bytes


def test_COMMAND_none_default_keeps_the_argument_optional(env):
    @env.first_command
    def joined(sep=None):
        return "none" if sep is None else sep

    assert env.evaluate("joined") == "none"
    assert env.evaluate("joined-x") == "x"


@pytest.mark.parametrize("value", [float("nan"), float("inf"), float("-inf")])
def test_COMMAND_non_finite_default_is_refused(env, value):
    def f(text, x=value):
        return text

    with pytest.raises(TypeError, match="finite"):
        env.command(f)


def test_COMMAND_closure_and_bound_method_get_no_code_version():
    def make(suffix):
        def add(text):
            return text + suffix

        return add

    class Holder:
        def method(self, text):
            return text

    def plain(text):
        return text

    assert _implementation_bytes(make("a")) is None
    assert _implementation_bytes(Holder().method) is None
    assert _implementation_bytes(plain) is not None


def test_COMMAND_closures_with_other_captures_do_not_share_results(env):
    def make(suffix):
        def tag(text):
            return text + suffix

        return tag

    env.command(make("-a"), name="tag")
    assert env.evaluate("hello/tag") == "hello-a"
    with pytest.warns(UserWarning):
        env.command(make("-b"), name="tag")
    assert env.evaluate("hello/tag") == "hello-b"


def test_COMMAND_unregistering_an_override_restores_the_builtin(env):
    with pytest.warns(UserWarning, match="built-in"):
        @env.command(name="to_text")
        def shadow(value):
            return "shadowed"

    assert env.evaluate("hello/to_text") == "shadowed"
    assert env.unregister("to_text") is True
    assert env.evaluate("hello/to_text") == "hello"
    assert env.unregister("to_text") is True
    with pytest.raises(lq.ActionNotRegisteredError):
        env.evaluate("hello/to_text")
    assert env.unregister("to_text") is False


def test_COMMAND_a_callable_without_a_name_needs_one(env):
    def base(suffix, text):
        return text + suffix

    partial = functools.partial(base, "!")
    with pytest.raises(TypeError, match="name="):
        env.command(partial)
    env.command(partial, name="exclaim")
    assert env.evaluate("hello/exclaim") == "hello!"


def test_COMMAND_context_after_star_args_is_refused(env):
    def f(text, *rest, context):
        return text

    with pytest.raises(TypeError, match="before \\*args"):
        env.command(f)


def test_COMMAND_a_state_parameter_with_any_name_issues_no_warning(env):
    with warnings.catch_warnings():
        warnings.simplefilter("error")

        @env.command
        def size(df):
            return len(df)

    assert env.evaluate("hello/size") == 5


def test_COMMAND_replacement_warning_points_at_the_caller(env):
    with pytest.warns(UserWarning, match="built-in") as record:
        @env.command(name="to_text")
        def shadow(value):
            return "shadowed"

    assert record[0].filename == __file__
