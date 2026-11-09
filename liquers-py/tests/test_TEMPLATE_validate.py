"""TEMPLATE: validation runs nothing (`specs/design/liquers-py-templates/`, AC-9)."""

from conftest import CALLS

import liquers as lq


def test_validation_runs_nothing(env):
    CALLS["slow"] = 0
    diagnostics = env.validate_template("{{$ slow-0}}\n{{$ no_such_command}}")
    assert CALLS["slow"] == 0
    assert len(diagnostics) == 1
    diagnostic = diagnostics[0]
    assert isinstance(diagnostic, lq.Diagnostic)
    assert diagnostic.severity == "error"
    assert "no_such_command" in diagnostic.message
    assert (diagnostic.position.line, diagnostic.position.column) == (2, 5)  # the action
    assert env.validate_template("{{name}}") == []

    @env.command
    def check(state, context):
        return len(context.validate_template("{{$ no_such_command}}"))

    assert env.evaluate("hello/check") == 1
