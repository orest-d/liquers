"""VALUE: the value bridge (LANGUAGE-INTEGRATION_GUIDE.md)."""

import pytest

import liquers as lq


def test_VALUE13_state_operations_preserve_or_discard_metadata(env):
    state = env.evaluate_state("hello/to_text")
    assert state.value == "hello"
    assert state.metadata.type_identifier() == "Text"
    with pytest.raises(AttributeError):
        state.value = "changed"  # a State is a read-only snapshot


def test_STATUS_names_every_variant_and_refuses_unknown_ones():
    for name in ["None", "Directory", "Recipe", "Submitted", "Dependencies", "Processing",
                 "Partial", "Error", "Storing", "Ready", "Expired", "Cancelled", "Source",
                 "Override", "Volatile"]:
        status = lq.Status(name)
        assert status.name() == name and status == name and status == lq.Status(name)
    with pytest.raises(ValueError):
        lq.Status("Finished")
