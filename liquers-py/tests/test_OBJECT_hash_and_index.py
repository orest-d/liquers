"""OBJECT: `Status` hashes like its name; `Key` indexing raises `IndexError`.

Regressions for the Phase 4 soundness review (specs/design/liquers-py/phase4-implementation.md).
"""

import pytest

import liquers as lq


def test_OBJECT_status_hashes_like_its_name():
    status = lq.Status("Ready")
    assert status == "Ready"
    assert hash(status) == hash("Ready")
    assert "Ready" in {status}
    assert status in {"Ready"}


def test_OBJECT_key_index_out_of_range_raises_index_error():
    key = lq.parse_key("a/b")
    assert str(key[0]) == "a"
    assert str(key[-1]) == "b"
    with pytest.raises(IndexError):
        key[2]
    with pytest.raises(IndexError):
        key[-3]
