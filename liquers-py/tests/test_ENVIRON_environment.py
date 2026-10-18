"""ENVIRON: environments, isolation, validation and shutdown (LANGUAGE-INTEGRATION_GUIDE.md)."""

import pytest

import liquers as lq


def test_ENVIRON01_default_environment_evaluates_builtin(env):
    doc = env.evaluate("commands_doc")
    assert isinstance(doc, str)
    assert "to_text" in doc
    assert "to_text" in lq.evaluate("commands_doc")
    assert lq.default_environment() is lq.default_environment()


def test_ENVIRON06_shutdown_is_idempotent():
    environment = lq.Environment()
    environment.evaluate("commands_doc")
    environment.shutdown()
    environment.shutdown()
    with pytest.raises(lq.GeneralError):
        environment.evaluate("commands_doc")


def test_ENVIRON08_invalid_metadata_reports_before_failing(env):
    env.import_metadata("commands:\n  - name: ghost\n    label: Ghost\n")
    issues = env.validate()
    assert any(i.severity == "error" and i.key is not None and "ghost" in i.key for i in issues)
    with pytest.raises(lq.ConfigurationError):
        env.evaluate("commands_doc")


def test_ENVIRON_config_is_a_dict_or_yaml():
    for config in ({"assets": {"job_capacity": 1}}, "assets:\n  job_capacity: 1\n"):
        environment = lq.Environment(config=config)
        assert "to_text" in environment.evaluate("commands_doc")
        environment.shutdown()
    with pytest.raises(lq.ConfigurationError):
        lq.Environment(conversion="lenient")


def test_ENVIRON02_custom_services_are_the_ones_returned(tmp_path):
    config = {"store": {"stores": [{"type": "filesystem", "prefix": "d",
                                    "config": {"path": str(tmp_path)}}]}}
    environment = lq.Environment(config=config)
    environment.store_set("d/note.txt", "kept")
    environment.shutdown()
    assert (tmp_path / "d" / "note.txt").read_text() == "kept"  # the key, under the root
    reopened = lq.Environment(config=config)
    assert reopened.evaluate("-R/d/note.txt") == "kept"
    reopened.shutdown()


def test_ENVIRON03_repeated_initialization_follows_policy():
    first, second = lq.Environment(), lq.Environment()
    assert first is not second
    assert lq.default_environment() is lq.default_environment()
    first.shutdown()
    second.shutdown()


def test_ENVIRON04_failed_initialization_is_recoverable():
    broken = lq.Environment(config={"store": {"stores": [{"type": "no_such_store_type"}]}})
    with pytest.raises(lq.LiquersError):
        broken.evaluate("commands_doc")
    with pytest.raises(lq.LiquersError):  # still failing the same way, not half-built
        broken.evaluate("commands_doc")
    broken.shutdown()
    working = lq.Environment()
    assert "to_text" in working.evaluate("commands_doc")
    working.shutdown()


def test_ENVIRON05_isolated_test_environments_do_not_leak_registration():
    first, second = lq.Environment(), lq.Environment()

    @first.first_command
    def only_in_first():
        return 1

    @second.first_command
    def only_in_second():
        return 2

    assert first.evaluate("only_in_first") == 1
    with pytest.raises(lq.ActionNotRegisteredError):
        first.evaluate("only_in_second")
    with pytest.raises(lq.ActionNotRegisteredError):
        second.evaluate("only_in_first")
    with pytest.raises(lq.ActionNotRegisteredError):
        lq.evaluate("only_in_first")
    first.shutdown()
    second.shutdown()


OPERATIONS = {
    "evaluate": lambda e: e.evaluate("hello"),
    "evaluate_state": lambda e: e.evaluate_state("hello"),
    "get_asset": lambda e: e.get_asset("hello").get(),
    "store_set": lambda e: e.store_set("d/x.txt", "x"),
    "command_metadata": lambda e: e.command_metadata("hello"),
    "validate": lambda e: e.validate(),
    "import_metadata": lambda e: e.import_metadata("commands: []\n"),
    "unregister": lambda e: e.unregister("not_there"),
    "command": lambda e: e.first_command(name="extra")(lambda: 1),
}


@pytest.mark.parametrize("operation", sorted(OPERATIONS))
@pytest.mark.parametrize("lifecycle", ["pending", "built", "shut"])
def test_ENVIRON07_documented_operations_callable_in_every_state(operation, lifecycle):
    from conftest import register_fixture_commands

    environment = lq.Environment()
    register_fixture_commands(environment)
    if lifecycle in ("built", "shut"):
        environment.evaluate("hello")
    if lifecycle == "shut":
        environment.shutdown()
        if operation in ("validate", "unregister"):
            OPERATIONS[operation](environment)  # reading declarations stays possible
        else:
            with pytest.raises(lq.GeneralError):
                OPERATIONS[operation](environment)
    else:
        OPERATIONS[operation](environment)
        environment.shutdown()
