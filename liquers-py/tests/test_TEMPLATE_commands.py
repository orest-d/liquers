"""TEMPLATE: template commands and renders inside commands (`specs/design/liquers-py-templates/`,
AC-3 to AC-6, AC-10, AC-11, AC-13)."""

import asyncio
import sys
import time

import pytest
from conftest import run_async

import liquers as lq

GREET = "-R/templates/greet.md"


@pytest.fixture
def tenv(env):
    env.store_set("templates/greet.md", "Hello, {{.}}!")
    env.store_set("templates/header.md", "Header")
    env.store_set("templates/page.md", "{{> ./header.md}} body")
    return env


def test_builtin_commands(tenv):
    """AC-3: the `ns-template` commands run from Python as they do from Rust."""
    assert tenv.evaluate("hello/ns-template/render-~X~-R/templates/greet.md~E") == "Hello, hello!"
    assert tenv.evaluate("-R/templates/greet.md/-/ns-template/validate") is not None


def test_template_command_needs_no_function(tenv):
    """AC-4: a template command is a declaration; its metadata says it is an alias."""
    tenv.template_command("greeting", template=GREET, label="Greeting", doc="Greets the input")
    assert tenv.evaluate("hello/greeting") == "Hello, hello!"
    metadata = tenv.command_metadata("greeting")
    assert (metadata.label, metadata.doc) == ("Greeting", "Greets the input")
    definition = metadata.definition
    assert definition.kind == "alias"
    assert definition.alias_of.name == "render" and definition.alias_of.namespace == "template"
    assert definition.head_parameters[0] == lq.parse(GREET)
    assert definition.pipeline is None
    assert [a.name for a in metadata.arguments] == ["data", "errors", "cwd"]
    assert tenv.command_metadata("hello").definition.kind == "registered"


def test_template_command_survives_rebuild(tenv):
    """AC-4: a rebuild (a new Python command) keeps it; a redeclaration wins at once and after."""
    tenv.template_command("greeting", template=GREET)
    assert tenv.evaluate("hello/greeting") == "Hello, hello!"

    @tenv.first_command
    def later() -> str:
        return "later"

    assert tenv.evaluate("hello/greeting") == "Hello, hello!"
    tenv.store_set("templates/bye.md", "Bye, {{.}}!")
    tenv.template_command("greeting", template=lq.parse_key("templates/bye.md"))
    assert tenv.evaluate("later/greeting") == "Bye, later!"

    @tenv.first_command
    def again() -> str:
        return "again"

    assert tenv.evaluate("again/greeting") == "Bye, again!"


def test_editing_the_template_invalidates(tenv):
    """AC-5: the template is a dependency of the command's result."""
    tenv.template_command("greeting", template=GREET)
    assert tenv.evaluate("hello/greeting") == "Hello, hello!"
    tenv.store_set("templates/greet.md", "Hi, {{.}}!")
    assert tenv.evaluate("hello/greeting") == "Hi, hello!"


def test_in_command_render_records_dependencies(tenv):
    """AC-6, AC-13: a query tag rendered in a command is a dependency of the command."""
    tenv.store_set("d/count.txt", "1")

    @tenv.first_command
    def report(context) -> str:
        return context.render("count {{$ -R/d/count.txt}}")

    @tenv.first_command
    async def report_async(context) -> str:
        return await context.render_async("count {{$ -R/d/count.txt}}")

    assert tenv.evaluate("report/out.txt") == "count 1"
    assert tenv.evaluate("report_async/out.txt") == "count 1"
    tenv.store_set("d/count.txt", "2")
    assert tenv.evaluate("report/out.txt") == "count 2"
    assert tenv.evaluate("report_async/out.txt") == "count 2"


def test_nested_python_renders(tenv):
    """AC-10: a Python render whose tag runs another Python command that renders."""

    @tenv.first_command
    def inner_render(context) -> str:
        return context.expand("{{x}}", data={"x": "in"})

    @tenv.first_command
    def outer_render(context) -> str:
        return context.expand("[{{$ inner_render}}]")

    started = time.monotonic()
    assert tenv.evaluate("outer_render") == "[in]"
    assert time.monotonic() - started < 5


def test_link_default_cwd(tenv):
    """AC-11: `lq.link` declares a default link; both renders resolve a partial against it."""

    @tenv.command
    def page(state, cwd: lq.Key = lq.link("-R-key/."), context=None) -> str:
        return context.render(lq.parse("-R/templates/page.md"), cwd=cwd)

    default = tenv.command_metadata("page").arguments[0].default
    assert default == "-R-key/."
    assert tenv.evaluate("-R-cwd/templates/-/page") == "Header body"
    assert tenv.render(lq.parse("-R/templates/page.md"), cwd=lq.parse_key("templates")) == "Header body"
    assert lq.link("-R-key/.") == lq.link(lq.parse("-R-key/."))
    with pytest.raises(lq.LiquersError):
        lq.link("-R/a/-/b-~")


def test_sync_form_in_async_command_raises(tenv):
    """AC-13: `render` in an `async def` command raises, as `evaluate` does."""

    @tenv.command
    async def sync_inside(state, context):
        await asyncio.sleep(0)
        return context.render("{{.}}", state=state)

    with pytest.raises(lq.LiquersError) as raised:
        tenv.evaluate("hello/sync_inside")
    assert "render_async" in str(raised.value)


def test_template_link_must_be_text(tenv):
    @tenv.first_command
    def mapping() -> object:
        return {"a": 1}

    tenv.template_command("from_mapping", template="mapping")
    with pytest.raises(lq.LiquersError):
        tenv.evaluate("hello/from_mapping")


def test_declare_cost(tenv):
    """A measurement, not an assertion: each declaration builds a trial registry."""
    started = time.perf_counter()
    for i in range(20):
        tenv.template_command(f"greeting{i}", template=GREET)
    per_call = (time.perf_counter() - started) / 20
    print(f"template_command: {per_call * 1000:.1f} ms per declaration", file=sys.stderr)
    assert tenv.evaluate("hello/greeting19") == "Hello, hello!"


def test_declared_and_python_commands_replace_each_other(tenv):
    tenv.template_command("greeting", template=GREET)

    with pytest.warns(UserWarning, match="replaces a declared command"):

        @tenv.command
        def greeting(state):
            return "python"

    assert tenv.evaluate("hello/greeting") == "python"
    tenv.template_command("greeting", template=GREET)
    assert tenv.evaluate("hello/greeting") == "Hello, hello!"
    assert tenv.command_metadata("greeting").definition.kind == "alias"


def test_unregister_declared(tenv):
    tenv.template_command("greeting", template=GREET)
    assert tenv.unregister("greeting")
    with pytest.raises(lq.LiquersError):
        tenv.evaluate("hello/greeting")

    @tenv.first_command
    def rebuilt() -> str:
        return "x"

    with pytest.raises(lq.LiquersError):
        tenv.evaluate("hello/greeting")
    assert not tenv.unregister("greeting")


def test_failed_declare_changes_nothing(tenv):
    tenv.template_command("greeting", template=GREET)
    with pytest.raises(lq.LiquersError):
        tenv.template_command("render", template=GREET, namespace="template")  # aliases itself
    assert tenv.evaluate("hello/greeting") == "Hello, hello!"
    assert tenv.evaluate("hello/ns-template/render-~X~-R/templates/greet.md~E") == "Hello, hello!"
    with pytest.raises(TypeError):
        tenv.template_command("bad", template=42)
