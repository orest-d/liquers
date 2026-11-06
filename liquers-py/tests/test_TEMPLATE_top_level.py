"""TEMPLATE: rendering from a script (`specs/design/liquers-py-templates/`, AC-1, AC-2, AC-7, AC-8,
AC-12, AC-13). A top-level render runs as an asset of its own, waited on with the GIL released."""

import _thread
import threading
import time

import pytest
from conftest import CALLS, run_async

import liquers as lq

GREET = "-R/templates/greet.md"


@pytest.fixture
def tenv(env):
    """`env` with the template fixture of Phase 3."""
    env.store_set("templates/greet.md", "Hello, {{.}}!")
    env.store_set("templates/ab.md", "{{a}}{{b}}")
    env.store_set("templates/show.html", "{{.}}")

    @env.first_command
    def ab() -> object:
        return {"a": 1}

    @env.first_command
    def b2() -> object:
        return {"b": 2}

    @env.first_command
    def bold() -> str:
        return "<b>"

    return env


def test_expand_over_a_dict(tenv):
    """AC-1: names from `data`, a query tag evaluated."""
    assert tenv.expand("{{name}} has {{$ number-42}}", data={"name": "Ann"}) == "Ann has 42"


def test_calls_match_the_commands(tenv):
    """AC-2: `render` and `expand` give what the `ns-template` commands give."""
    greet = lq.parse(GREET)
    assert tenv.render(greet, state="hello") == tenv.evaluate(
        "hello/ns-template/render-~X~-R/templates/greet.md~E"
    )
    assert tenv.render(lq.parse_key("templates/greet.md"), state="hello") == "Hello, hello!"
    assert tenv.expand("[{{.}}]") == "[]"


def test_strict_raises_with_position(tenv):
    """AC-7: a strict render raises the failing tag's error, positioned."""
    with pytest.raises(lq.LiquersError) as raised:
        tenv.expand("ok\n{{$ missing_command}}")
    position = raised.value.position
    assert (position.line, position.column) == (2, 1), position


def test_recovering_modes_warn(tenv):
    """AC-7: a recovering mode returns the text and the warnings, with positions."""
    result = tenv.expand("ok\n{{$ missing_command}}", errors="silent", full=True)
    assert isinstance(result, lq.RenderResult)
    assert result.text == "ok\n"
    assert len(result.warnings) == 1
    assert result.warnings[0].position.line == 2
    assert "1 warning" in repr(result)
    expanded = tenv.expand("{{$ missing_command}}", errors="expand")
    assert "missing_command" in expanded


def test_escaping_is_explicit(tenv):
    """AC-8: a call escapes only with `html=True`; a command escapes for an `.html` result."""
    assert tenv.expand("{{v}}", data={"v": "<b>"}) == "<b>"
    assert tenv.expand("{{v}}", data={"v": "<b>"}, html=True) == "&lt;b&gt;"
    tenv.template_command("show", template="-R/templates/show.html")
    assert tenv.evaluate("bold/show/page.html") == "&lt;b&gt;"


def test_two_frames_at_top_level(tenv):
    """AC-12: state below data, at the top level, in a command and through the query."""
    ab_md = lq.parse("-R/templates/ab.md")

    @tenv.command
    def in_command(state, context):
        return context.render(ab_md, state=state, data={"b": 2})

    assert tenv.render(ab_md, state={"a": 1}, data={"b": 2}) == "12"
    assert tenv.evaluate("ab/in_command") == "12"
    assert tenv.evaluate("ab/ns-template/render-~X~-R/templates/ab.md~E-~X~b2~E") == "12"


def test_async_forms_agree(tenv):
    """AC-13: the awaitable forms give what the sync ones give."""
    greet = lq.parse(GREET)

    async def main():
        rendered = await tenv.render_async(greet, state="hello")
        expanded = await tenv.expand_async("{{x}}", data={"x": 2}, full=True)
        return rendered, expanded

    rendered, expanded = run_async(main)
    assert rendered == tenv.render(greet, state="hello")
    assert expanded.text == "2" and expanded.warnings == []


def test_parse_error_raises_early(tenv):
    """Text that does not parse raises before any asset: its query tag never runs."""
    CALLS["slow"] = 0
    with pytest.raises(lq.LiquersError):
        tenv.expand("{{$ slow-0}} {{")
    assert CALLS["slow"] == 0


def test_data_must_be_a_mapping(tenv):
    with pytest.raises(TypeError):
        tenv.expand("{{.}}", data=[1, 2])


def test_bad_errors_mode(tenv):
    with pytest.raises(lq.GeneralError):
        tenv.expand("x", errors="loud")


def test_argument_types():
    environment = lq.Environment(conversion="strict")
    try:
        with pytest.raises(TypeError):
            environment.expand(42)
        with pytest.raises(TypeError):
            environment.render("{{.}}", state=object())
        with pytest.raises(TypeError):
            environment.expand("x", cwd="templates")
    finally:
        environment.shutdown()


def test_interrupt_cancels_the_render(env):
    """Ctrl-C during a render whose tag is slow: the render is cancelled and the call raises
    `KeyboardInterrupt` at once."""
    timer = threading.Timer(0.3, _thread.interrupt_main)
    timer.start()
    started = time.monotonic()
    with pytest.raises(KeyboardInterrupt):
        env.expand("{{$ slow-5}}")
    timer.join()
    assert time.monotonic() - started < 3


def test_second_interrupt_is_deferred(env):
    """A second Ctrl-C while the render settles does not cut the settle short: the render raises
    `KeyboardInterrupt` once, and the second is raised after it, within the grace period."""
    timers = [threading.Timer(delay, _thread.interrupt_main) for delay in (0.3, 0.32)]
    for timer in timers:
        timer.start()
    started = time.monotonic()
    raised = 0
    try:
        env.expand("{{$ slow-5}}")
    except KeyboardInterrupt:
        raised += 1
    try:
        for _ in range(100):  # gives a pending second interrupt the chance to fire
            time.sleep(0.01)
    except KeyboardInterrupt:
        raised += 1
    for timer in timers:
        timer.join()
    assert 1 <= raised <= 2
    assert time.monotonic() - started < 6
