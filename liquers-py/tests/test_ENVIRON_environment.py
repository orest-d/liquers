"""ENVIRON: environments, isolation, validation and shutdown (LANGUAGE-INTEGRATION_GUIDE.md)."""

import pytest

import liquers as lq


@pytest.fixture
def env():
    environment = lq.Environment()
    yield environment
    environment.shutdown()


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
