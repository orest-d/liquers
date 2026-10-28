"""OBJECT: the reprs of `Key`, `Expires` and `ExpirationTime` quote their text form once.

Design: specs/design/py-encoded-reprs/DESIGN.md (AC-2, AC-4).
"""

import liquers as lq


def test_OBJECT_repr_key_quoted_once():
    key = lq.parse_key("data/a.txt")
    assert repr(key) == "Key('data/a.txt')"
    assert repr(key) == "Key(" + repr(key.encode()) + ")"
    assert eval(repr(key), vars(lq)) == key


def test_OBJECT_repr_expiration_uses_text_form():
    cases = [
        (lq.Expires("in 5 min"), "Expires('in 5 min')"),
        (lq.Expires("end of day"), "Expires('end of day')"),
        (lq.ExpirationTime("never"), "ExpirationTime('never')"),
    ]
    for value, expected in cases:
        assert repr(value) == expected
        assert eval(repr(value), vars(lq)) == value
