"""COMMAND: registering Python callables as commands (LANGUAGE-INTEGRATION_GUIDE.md)."""

import warnings

import pytest

import liquers as lq


def test_COMMAND01_register_and_execute_first_command(env):
    @env.first_command
    def constant() -> str:
        return "c"

    assert env.evaluate("constant") == "c"


def test_COMMAND02_transform_receives_state_and_parameter(env):
    @env.command
    def repeat(text: str, count: int = 2) -> str:
        return text * count

    assert env.evaluate("hello/repeat-3") == "hellohellohello"
    assert env.evaluate("hello/repeat") == "hellohello"


def test_COMMAND03_exception_crosses_command_boundary(env):
    with pytest.raises(lq.ExecutionError) as raised:
        env.evaluate("hello/boom")
    assert "ValueError" in raised.value.message and "inner" in raised.value.message


def test_COMMAND04_defaults_and_variadics_bind(env):
    @env.command
    def f(value, mode: str = "a", *rest: int) -> str:
        return f"{mode}:{sum(rest)}"

    assert env.evaluate("hello/f") == "a:0"
    assert env.evaluate("hello/f-b-1-2") == "b:3"


def test_COMMAND05_metadata_matches_the_declaration(env):
    @env.command(label="Repeat it", doc="Repeat the input.")
    def repeat(text: str, count: int = 2) -> str:
        return text * count

    meta = env.command_metadata("repeat")
    assert meta.label == "Repeat it" and meta.doc == "Repeat the input."
    assert [(a.name, a.default) for a in meta.arguments] == [("count", 2)]
    assert meta.arguments[0].argument_type == "int"

    @env.first_command
    def typed(a: str, b: float, c: bool, d: "int | None" = None, e=None):
        return a

    assert [a.argument_type for a in env.command_metadata("typed").arguments] == [
        "string", "float", "bool", "int_opt", "any"]

    @env.first_command
    def nullary():
        return 1

    assert env.command_metadata("nullary").arguments == []


def test_COMMAND05_bare_decorator_derives_label_and_doc():
    @lq.command
    def repeat_bare(text: str, count: int = 2) -> str:
        """Repeat the input text."""
        return text * count

    meta = lq.default_environment().command_metadata("repeat_bare")
    assert meta.label == "Repeat bare" and meta.doc == "Repeat the input text."
    assert lq.default_environment().unregister("repeat_bare")


def test_COMMAND05_keyword_only_and_kwargs_are_refused(env):
    with pytest.raises(TypeError, match="flag"):
        @env.command
        def kw(text, *, flag: bool = False):
            return text

    with pytest.raises(TypeError, match="extra"):
        @env.command
        def kwargs(text, **extra):
            return text


def test_COMMAND06_duplicate_and_unregister_policy(env):
    assert env.evaluate("hello") == "hello"

    @env.first_command
    def late():
        return "late"

    assert env.evaluate("late") == "late"

    with pytest.warns(UserWarning, match="Python"):
        @env.first_command(name="late")
        def late_again():
            return "again"

    assert env.evaluate("late") == "again"

    with pytest.warns(UserWarning, match="built-in"):
        @env.command(name="to_text")
        def shadow(value):
            return "shadowed"

    assert env.evaluate("hello/to_text") == "shadowed"
    assert env.unregister("late") is True
    assert env.unregister("late") is False
    with pytest.raises(lq.ActionNotRegisteredError):
        env.evaluate("late")


def test_COMMAND_late_registration_keeps_store(env):
    env.store_set("d/in.txt", "abc")
    assert env.evaluate("-R/d/in.txt") == "abc"

    @env.command
    def shout(text):
        return text.upper()

    assert env.evaluate("-R/d/in.txt/-/shout") == "ABC"


def test_COMMAND_replaced_command_does_not_reuse_stored_results(env):
    @env.first_command
    def answer():
        return 1

    env.store_set("x/seed.txt", "seed")
    assert env.evaluate("answer") == 1

    with pytest.warns(UserWarning):
        @env.first_command(name="answer")
        def answer2():
            return 2

    assert env.evaluate("answer") == 2


def test_COMMAND07_context_injection(env):
    @env.command
    def logs(text, context):
        context.info("hello from command")
        return text

    state = env.evaluate_state("hello/logs")
    assert state.value == "hello"
    assert any("hello from command" in str(entry) for entry in state.metadata.log())
    assert env.evaluate("outer") == "outer:hello"


def test_COMMAND08_returned_opaque_value_follows_value_rules():
    strict = lq.Environment(conversion="strict")

    class Unregistered:
        pass

    @strict.first_command
    def unopted():
        return Unregistered()

    with pytest.raises(lq.ConversionError, match="Unregistered"):
        strict.evaluate("unopted")
    strict.shutdown()

    permissive = lq.Environment()
    sentinel = object()

    @permissive.first_command
    def give():
        return sentinel

    assert permissive.evaluate("give") is sentinel
    permissive.shutdown()


def test_COMMAND09_minimal_declaration_has_useful_metadata_defaults(env):
    @env.command
    def plain(text: str) -> str:
        return text

    meta = env.command_metadata("plain")
    assert meta.name == "plain" and meta.namespace == "" and meta.label == "Plain"


def test_COMMAND10_complete_declaration_preserves_every_field(env):
    @env.command(label="L", doc="D", namespace="nsx", filename="out.txt", volatile=True,
                 cached=False)
    def full(value):
        return value

    meta = env.command_metadata("full", namespace="nsx")
    assert (meta.label, meta.doc, meta.namespace, meta.filename, meta.volatile, meta.cached) == (
        "L", "D", "nsx", "out.txt", True, False)
    assert env.evaluate("hello/ns-nsx/full") == "hello"


def test_COMMAND11_closure_captures_retained_per_runtime_rules(env):
    def make(n):
        @env.command(name=f"add{n}")
        def add(value: int) -> int:
            return value + n

    make(5)
    assert env.evaluate("number-1/add5") == 6


def test_COMMAND12_declared_planner_flags_take_effect(env):
    calls = []

    @env.first_command(volatile=True)
    def tick():
        calls.append(1)
        return len(calls)

    assert env.command_metadata("tick").volatile is True
    assert env.evaluate("tick") == 1
    assert env.evaluate("tick") == 2, "a volatile command must not be served from cache"


def test_COMMAND13_every_state_mode_delivers_its_content(env):
    @env.command
    def as_value(value):
        return type(value).__name__

    @env.command
    def as_text(text):
        return text.upper()

    @env.command
    def as_state(state):
        return f"{state.value}|{state.metadata.status()}|{state.metadata.type_identifier()}"

    @env.first_command
    def as_none():
        return "none"

    assert env.evaluate("number-3/as_value") == "int"
    assert env.evaluate("number-3/as_text") == "3"
    value, status, identifier = env.evaluate("hello/as_state").split("|")
    assert value == "hello" and identifier == "Text" and status
    assert env.evaluate("hello/as_none") == "none"


def test_COMMAND14_retained_declaration_is_immune_to_caller_mutation(env):
    options = {"label": "Original"}

    def snap(text):
        return text

    env.command(**options)(snap)
    options["label"] = "Mutated"

    @env.first_command
    def unrelated():  # forces a rebuild, which replays the retained declaration
        return 0

    assert env.command_metadata("snap").label == "Original"
